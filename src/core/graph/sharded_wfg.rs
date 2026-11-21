//! Sharded Wait-For Graph (Deloxide-SR Novelty 1)
//!
//! Partitions thread wait-for dependencies across `NUM_SHARDS` independent shards
//! (`tid % NUM_SHARDS`). Intra-shard cycle checks lock only 1 shard; cross-shard
//! traversals acquire shard locks in strict ascending Shard ID order (0..K) to
//! guarantee deadlock-freedom of the detector itself.

use crate::core::types::ThreadId;
use fxhash::{FxHashMap, FxHashSet};
use parking_lot::Mutex;
use std::collections::VecDeque;

pub const NUM_SHARDS: usize = 8;

#[inline]
pub fn shard_index(tid: ThreadId) -> usize {
    (tid as usize) % NUM_SHARDS
}

struct WfgShard {
    edges: FxHashMap<ThreadId, FxHashSet<ThreadId>>,
    incoming_edges: FxHashMap<ThreadId, FxHashSet<ThreadId>>,
    bfs_queue: VecDeque<ThreadId>,
    bfs_visited: FxHashSet<ThreadId>,
    bfs_parent: FxHashMap<ThreadId, ThreadId>,
}

impl WfgShard {
    fn new() -> Self {
        Self {
            edges: FxHashMap::default(),
            incoming_edges: FxHashMap::default(),
            bfs_queue: VecDeque::with_capacity(32),
            bfs_visited: FxHashSet::default(),
            bfs_parent: FxHashMap::default(),
        }
    }

    fn find_local_path(
        &mut self,
        shard_id: usize,
        start: ThreadId,
        target: ThreadId,
    ) -> Result<Option<Vec<ThreadId>>, ()> {
        if start == target {
            return Ok(Some(vec![start]));
        }

        self.bfs_queue.clear();
        self.bfs_visited.clear();
        self.bfs_parent.clear();

        self.bfs_queue.push_back(start);
        self.bfs_visited.insert(start);

        while let Some(current) = self.bfs_queue.pop_front() {
            if current == target {
                let mut path = Vec::with_capacity(self.bfs_parent.len() + 1);
                let mut curr = target;
                path.push(curr);
                while let Some(&p) = self.bfs_parent.get(&curr) {
                    path.push(p);
                    curr = p;
                }
                path.reverse();
                return Ok(Some(path));
            }

            if let Some(neighbors) = self.edges.get(&current) {
                for &neighbor in neighbors {
                    if shard_index(neighbor) != shard_id {
                        return Err(());
                    }
                    if self.bfs_visited.insert(neighbor) {
                        self.bfs_parent.insert(neighbor, current);
                        self.bfs_queue.push_back(neighbor);
                    }
                }
            }
        }

        Ok(None)
    }
}

pub struct ShardedWaitForGraph {
    shards: [Mutex<WfgShard>; NUM_SHARDS],
}

impl Default for ShardedWaitForGraph {
    fn default() -> Self {
        Self::new()
    }
}

impl ShardedWaitForGraph {
    pub fn new() -> Self {
        Self {
            shards: std::array::from_fn(|_| Mutex::new(WfgShard::new())),
        }
    }

    pub fn add_edge(&self, from: ThreadId, to: ThreadId) -> Option<Vec<ThreadId>> {
        let from_shard = shard_index(from);
        let to_shard = shard_index(to);

        // Fast Path 0: Self-loop check
        if from == to {
            return Some(vec![from]);
        }

        // Fast Path 1: Zero-Incoming-Edge Bypass (O(1))
        // If no thread is waiting on `from`, adding `from -> to` can NEVER form a cycle.
        {
            let mut f_shard = self.shards[from_shard].lock();
            if f_shard.edges.get(&from).is_some_and(|t| t.contains(&to)) {
                return None;
            }
            let has_incoming = f_shard
                .incoming_edges
                .get(&from)
                .is_some_and(|w| !w.is_empty());
            if !has_incoming {
                f_shard.edges.entry(from).or_default().insert(to);
                if from_shard == to_shard {
                    f_shard.incoming_edges.entry(to).or_default().insert(from);
                    return None;
                }
                drop(f_shard);
                self.shards[to_shard]
                    .lock()
                    .incoming_edges
                    .entry(to)
                    .or_default()
                    .insert(from);
                return None;
            }
        }

        if from_shard == to_shard {
            let mut shard = self.shards[from_shard].lock();
            if shard.edges.get(&from).is_some_and(|t| t.contains(&to)) {
                return None;
            }
            match shard.find_local_path(from_shard, to, from) {
                Ok(Some(path)) => return Some(path),
                Ok(None) => {
                    shard.edges.entry(from).or_default().insert(to);
                    shard.incoming_edges.entry(to).or_default().insert(from);
                    return None;
                }
                Err(()) => {}
            }
        }

        let mut guards: Vec<_> = self.shards.iter().map(|s| s.lock()).collect();

        if guards[from_shard]
            .edges
            .get(&from)
            .is_some_and(|t| t.contains(&to))
        {
            return None;
        }

        if let Some(path) = Self::find_cross_shard_path(&guards, to, from) {
            return Some(path);
        }

        guards[from_shard].edges.entry(from).or_default().insert(to);
        guards[to_shard]
            .incoming_edges
            .entry(to)
            .or_default()
            .insert(from);
        None
    }

    pub fn clear_wait_edges(&self, thread_id: ThreadId) {
        let s_idx = shard_index(thread_id);
        let targets: Option<FxHashSet<ThreadId>> = {
            let mut shard = self.shards[s_idx].lock();
            shard.edges.remove(&thread_id)
        };

        if let Some(targets) = targets {
            for target in targets {
                let t_idx = shard_index(target);
                let mut t_shard = self.shards[t_idx].lock();
                if let Some(waiters) = t_shard.incoming_edges.get_mut(&target) {
                    waiters.remove(&thread_id);
                    if waiters.is_empty() {
                        t_shard.incoming_edges.remove(&target);
                    }
                }
            }
        }
    }

    pub fn remove_edge(&self, from: ThreadId, to: ThreadId) {
        let f_idx = shard_index(from);
        let t_idx = shard_index(to);

        let removed = {
            let mut f_shard = self.shards[f_idx].lock();
            if let Some(neighbors) = f_shard.edges.get_mut(&from) {
                let r = neighbors.remove(&to);
                if neighbors.is_empty() {
                    f_shard.edges.remove(&from);
                }
                r
            } else {
                false
            }
        };

        if removed {
            let mut t_shard = self.shards[t_idx].lock();
            if let Some(waiters) = t_shard.incoming_edges.get_mut(&to) {
                waiters.remove(&from);
                if waiters.is_empty() {
                    t_shard.incoming_edges.remove(&to);
                }
            }
        }
    }

    pub fn remove_thread(&self, thread_id: ThreadId) {
        self.clear_wait_edges(thread_id);

        let s_idx = shard_index(thread_id);
        let waiters: Option<FxHashSet<ThreadId>> = {
            let mut shard = self.shards[s_idx].lock();
            shard.incoming_edges.remove(&thread_id)
        };

        if let Some(waiters) = waiters {
            for waiter in waiters {
                let w_idx = shard_index(waiter);
                let mut w_shard = self.shards[w_idx].lock();
                if let Some(forward_set) = w_shard.edges.get_mut(&waiter) {
                    forward_set.remove(&thread_id);
                    if forward_set.is_empty() {
                        w_shard.edges.remove(&waiter);
                    }
                }
            }
        }
    }

    fn find_cross_shard_path(
        guards: &[parking_lot::MutexGuard<'_, WfgShard>],
        start: ThreadId,
        target: ThreadId,
    ) -> Option<Vec<ThreadId>> {
        if start == target {
            return Some(vec![start]);
        }

        let mut queue = VecDeque::with_capacity(64);
        let mut visited = FxHashSet::default();
        let mut parent = FxHashMap::default();

        queue.push_back(start);
        visited.insert(start);

        while let Some(current) = queue.pop_front() {
            if current == target {
                let mut path = Vec::with_capacity(parent.len() + 1);
                let mut curr = target;
                path.push(curr);
                while let Some(&p) = parent.get(&curr) {
                    path.push(p);
                    curr = p;
                }
                path.reverse();
                return Some(path);
            }

            let c_idx = shard_index(current);
            if let Some(neighbors) = guards[c_idx].edges.get(&current) {
                for &neighbor in neighbors {
                    if visited.insert(neighbor) {
                        parent.insert(neighbor, current);
                        queue.push_back(neighbor);
                    }
                }
            }
        }

        None
    }

    pub fn register_thread(&self, thread_id: ThreadId) {
        let s_idx = shard_index(thread_id);
        self.shards[s_idx]
            .lock()
            .edges
            .entry(thread_id)
            .or_default();
    }

    #[cfg(test)]
    pub(crate) fn outgoing(&self, thread_id: ThreadId) -> Vec<ThreadId> {
        let s_idx = shard_index(thread_id);
        let shard = self.shards[s_idx].lock();
        let mut result: Vec<_> = shard
            .edges
            .get(&thread_id)
            .into_iter()
            .flat_map(|edges| edges.iter().copied())
            .collect();
        result.sort_unstable();
        result
    }

    #[cfg(test)]
    pub(crate) fn contains_thread(&self, thread_id: ThreadId) -> bool {
        let s_idx = shard_index(thread_id);
        self.shards[s_idx].lock().edges.contains_key(&thread_id)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detects_intra_shard_cycle() {
        let wfg = ShardedWaitForGraph::new();
        assert!(wfg.add_edge(1, 9).is_none());
        let cycle = wfg.add_edge(9, 1);
        assert_eq!(cycle, Some(vec![1, 9]));
    }

    #[test]
    fn detects_cross_shard_cycle() {
        let wfg = ShardedWaitForGraph::new();
        assert!(wfg.add_edge(1, 2).is_none());
        assert!(wfg.add_edge(2, 3).is_none());
        let cycle = wfg.add_edge(3, 1);
        assert_eq!(cycle, Some(vec![1, 2, 3]));
    }

    #[test]
    fn removes_thread_cleanly_across_shards() {
        let wfg = ShardedWaitForGraph::new();
        assert!(wfg.add_edge(1, 2).is_none());
        assert!(wfg.add_edge(2, 3).is_none());
        wfg.remove_thread(2);
        assert!(wfg.add_edge(3, 1).is_none());
    }

    #[test]
    fn bench_sharded_vs_global_wfg() {
        use crate::core::graph::wait_for_graph::WaitForGraph;
        use std::sync::{Arc, Barrier};
        use std::thread;
        use std::time::Instant;

        let ops_per_thread = 10_000usize;
        println!(
            "\n=== WFG Scalability: Global Mutex vs Sharded (K={}) ===",
            NUM_SHARDS
        );
        println!(
            "{:<10} {:<15} {:<15} {:<10}",
            "Threads", "Global WFG", "Sharded WFG", "Speedup"
        );

        for &num_threads in &[8usize, 16, 32] {
            // 1. Global Mutex<WaitForGraph>
            let global = Arc::new(Mutex::new(WaitForGraph::new()));
            let barrier = Arc::new(Barrier::new(num_threads));
            let start = Instant::now();
            let mut handles = Vec::with_capacity(num_threads);
            for t in 0..num_threads {
                let g = Arc::clone(&global);
                let b = Arc::clone(&barrier);
                handles.push(thread::spawn(move || {
                    let from = (t + 1) as ThreadId;
                    let to = (t + 1 + NUM_SHARDS) as ThreadId;
                    b.wait();
                    for _ in 0..ops_per_thread {
                        let mut guard = g.lock();
                        let _ = guard.add_edge(from, to);
                        guard.clear_wait_edges(from);
                    }
                }));
            }
            for h in handles {
                h.join().unwrap();
            }
            let global_ms = start.elapsed().as_secs_f64() * 1000.0;

            // 2. ShardedWaitForGraph (K=8)
            let sharded = Arc::new(ShardedWaitForGraph::new());
            let barrier = Arc::new(Barrier::new(num_threads));
            let start = Instant::now();
            let mut handles = Vec::with_capacity(num_threads);
            for t in 0..num_threads {
                let g = Arc::clone(&sharded);
                let b = Arc::clone(&barrier);
                handles.push(thread::spawn(move || {
                    let from = (t + 1) as ThreadId;
                    let to = (t + 1 + NUM_SHARDS) as ThreadId;
                    b.wait();
                    for _ in 0..ops_per_thread {
                        let _ = g.add_edge(from, to);
                        g.clear_wait_edges(from);
                    }
                }));
            }
            for h in handles {
                h.join().unwrap();
            }
            let sharded_ms = start.elapsed().as_secs_f64() * 1000.0;

            let speedup = global_ms / sharded_ms;
            println!(
                "{:<10} {:<15.2} {:<15.2} {:.2}x",
                num_threads, global_ms, sharded_ms, speedup
            );
        }
        println!();
    }
}
