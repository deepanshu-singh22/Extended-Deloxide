<p align="center">
  <img src="images/deloxide_logo_orange.png" alt="Deloxide" width="140">
</p>

<h1 align="center">Extended-Deloxide</h1>

<p align="center">
  Blazingly fast runtime deadlock detection, sharded WFG scaling, and cooperative deadlock recovery for Rust.<br>
  Turn an unexplained hang into a concrete thread-and-lock cycle — or recover from it dynamically.
</p>

<p align="center">
  <a href="https://crates.io/crates/deloxide"><img src="https://img.shields.io/crates/v/deloxide.svg" alt="crates.io release"></a>
  <a href="https://docs.rs/deloxide"><img src="https://docs.rs/deloxide/badge.svg" alt="docs.rs"></a>
  <a href="https://www.rust-lang.org/"><img src="https://img.shields.io/badge/rust-1.85%2B-black.svg" alt="Rust 1.85+"></a>
  <a href="https://github.com/Emivvvvv/deloxide/actions/workflows/ci.yml"><img src="https://github.com/Emivvvvv/deloxide/actions/workflows/ci.yml/badge.svg" alt="CI"></a>
  <a href="https://crates.io/crates/deloxide"><img src="https://img.shields.io/crates/d/deloxide.svg" alt="downloads"></a>
  <a href="LICENSE-MIT"><img src="https://img.shields.io/badge/license-MIT%20OR%20Apache--2.0-green.svg" alt="MIT or Apache-2.0"></a>
</p>

<p align="center">
  <a href="https://emivvvvv.github.io/deloxide/"><strong>Read the user manual »</strong></a>
  <br />
  <a href="https://docs.rs/deloxide">API reference</a>
  ·
  <a href="https://github.com/deepanshu-singh22/Extended-Deloxide/tree/main/examples">Examples</a>
  ·
  <a href="https://github.com/deepanshu-singh22/Extended-Deloxide/issues">Report a bug</a>
</p>

---

**Extended-Deloxide** builds on top of [Deloxide](https://doi.org/10.1016/j.infsof.2026.108343) (Tala & Baydere, *Information and Software Technology*, 2026) to solve two architectural limitations of the baseline detector: **centralized WFG lock contention** under high concurrency and **passive-only deadlock reporting** without runtime recovery.

## What's New in Extended-Deloxide

### 1. Sharded Wait-For Graph (`ShardedWaitForGraph`, K = 8)
- **Implementation:** `src/core/graph/sharded_wfg.rs`
- Replaces the single global WFG mutex with **K = 8 independently locked shards** indexed via `thread_id % 8`.
- **O(1) Zero-Incoming-Edge Bypass:** When inserting edge `u -> v`, if `incoming[u]` is empty (in-degree = 0), no cycle can form — skipping cross-shard BFS completely in O(1) time.
- **Deadlock-Free Shard Locking:** Enforces strict ascending shard-index ordering (`min(s_u, s_v) -> max(s_u, s_v)` for two-shard updates; `0 -> 7` for cross-shard BFS), guaranteeing the detector itself can never deadlock.

### 2. Policy-Driven Cooperative Deadlock Recovery
- **Implementation:** `src/core/recovery.rs`, `src/core/locks/mutex.rs`, `src/core/detector/mutex.rs`
- Adds `Mutex::lock_recoverable() -> Result<MutexGuard<T>, DeadlockError>` so threads can safely abort, release held guards (breaking Coffman's Hold-and-Wait), back off, and retry instead of hanging permanently.
- **WFG Wait-Edge Rollback:** Invokes `cancel_wait(thread_id)` before returning `Err(DeadlockError)` to prevent stale edges and false deadlock reports.
- **Configurable Victim-Selection Policies:**
  - `RecoveryPolicy::MinLocksHeld` (default): Selects the cycle participant holding the fewest locks.
  - `RecoveryPolicy::YoungestThread`: Selects the most recently spawned thread (highest `ThreadId`).

---

## Extended-Deloxide Benchmarks & Verification

Multi-threaded release-mode benchmark (`bench_sharded_vs_global_wfg`) comparing baseline single-mutex `WaitForGraph` vs. `ShardedWaitForGraph` (K = 8) on Apple Silicon (macOS):

| Concurrent Threads | Global WFG (ms) | Sharded WFG (K=8) (ms) | Speedup |
| :---: | :---: | :---: | :---: |
| **8** | 19.40 | 3.89 | **4.98×** |
| **16** | 35.89 | 5.39 | **6.66×** |
| **32** | 76.56 | 12.84 | **5.96×** *(Peak: **7.15×**)* |

- **Live AB-BA Recovery:** Deterministic two-thread AB-BA deadlock breaks and completes cleanly in **0.01 s** (`tests/recoverable_deadlock.rs`).
- **Full Regression Suite:** **82/82 tests passing** (30 unit tests, 15 integration test suites, 36 doc-tests).

### Run Extended-Deloxide Benchmarks & Tests

```console
# Full test suite (all features)
cargo test --all-features

# Sharded vs. Global WFG scalability benchmark (release mode)
cargo test --release --lib bench_sharded_vs_global_wfg -- --nocapture

# Live cooperative AB-BA deadlock recovery test
cargo test --test recoverable_deadlock -- --nocapture

# Launch interactive Web Visualizer demo
cargo run --example demo_visualizer --features logging-and-visualization
```

### Cooperative Recovery Usage

```rust
use deloxide::{Mutex, RecoveryPolicy, set_recovery_policy};
use std::time::Duration;

set_recovery_policy(RecoveryPolicy::MinLocksHeld);

loop {
    let mut ga = lock_a.lock();
    match lock_b.lock_recoverable() {
        Ok(mut gb) => {
            *ga += 10;
            *gb += 1;
            break;
        }
        Err(_err) => {
            drop(ga); // Release held lock safely to break Hold-and-Wait
            std::thread::sleep(Duration::from_millis(5));
        }
    }
}
```

---

## Why Deloxide

- **Instant active detection:** validates wait-for cycles when they occur, without
  waiting for a polling interval.
- **Sharded WFG scaling:** distributes slow-path WFG updates across 8 shards with an
  O(1) acyclic bypass.
- **Cooperative deadlock recovery:** opt-in `Mutex::lock_recoverable()` with
  policy-driven victim selection.
- **Custom callbacks:** run your own application logic when a deadlock is found,
  such as recording diagnostics, notifying a supervisor, or sending an alert.
- **Interactive visualization:** inspect the event timeline and thread-lock graph
  in a browser.
- **Lock-order analysis:** find acquisition-order risks before they become active
  deadlocks.
- **Stress testing:** use random or component-based scheduling disturbance to
  reproduce timing-sensitive failures.
- **Low-overhead default path:** keeps eligible uncontended operations away from
  global graph work.
- **Rust first, C supported:** tracked Mutexes, RwLocks, condition variables, and
  thread registration are available through both interfaces.

![Deloxide visualization showing the event timeline and thread-lock cycle](docs/src/assets/visualization.png)

## Comparison

The Rust ecosystem's existing approaches make a difficult trade-off. Static
analysis can be noisy on complex paths. Passive monitors preserve performance but
do not force rare schedules and report only when polled. Synchronous graph
debuggers detect immediately but can make every lock operation expensive.

Extended-Deloxide bridges that gap: synchronous active detection with an Optimistic
Fast Path, sharded WFG scaling, cooperative recovery, lock-order analysis, stress
testing, structured callbacks, and interactive visualization in one toolkit.

| Feature | STD | PL+DD | ND | Base DX | **Extended-Deloxide** |
| :--- | :---: | :---: | :---: | :---: | :---: |
| **Mutex overhead** | 0.88× | 1.00× | 1063.33× | 1.09× | **1.09× (Preserved)** |
| **Raytracing at 1080p** | 0.94× | 1.00× | 17.96× | 0.91× | **0.91× (Preserved)** |
| **WFG concurrency** | N/A | Async (poll) | Global lock | Global mutex | **8-Shard WFG (4.98×–7.15×)** |
| **Deadlock recovery** | None | None | None | Passive only | **Cooperative (`lock_recoverable`)** |
| **Lock-order analysis** | No | No | No | Yes | **Yes** |
| **Stress testing** | No | No | No | Yes | **Yes** |
| **Visualization** | No | No | Text dump | Interactive URL | **Interactive URL** |
| **False-positive rate (WFG)** | N/A | Zero | Zero | Zero | **Zero** |

*STD = `std::sync`, PL+DD = `parking_lot` with `deadlock_detection`, ND =
`no_deadlocks`, Base DX = Baseline Deloxide.*

See [Why Deloxide](https://emivvvvv.github.io/deloxide/comparison.html) for the
detection-model differences behind the table.

## Quick start

```toml
[dependencies]
deloxide = "1.1"
```

Initialize once before the tracked workload:

```rust
use deloxide::{DeadlockSource, Deloxide};

Deloxide::new()
    .callback(|report| match report.source {
        DeadlockSource::WaitForGraph => {
            eprintln!("active cycle: {:?}", report.thread_cycle);
            eprintln!("waited locks: {:?}", report.thread_waiting_for_locks);
        }
        DeadlockSource::LockOrderViolation => {
            eprintln!("potential lock order: {:?}", report.lock_order_cycle);
        }
    })
    .start()
    .expect("start Deloxide");
```

The callback is application-controlled. It can record the report, export
telemetry, notify an incident system, capture additional diagnostics, or signal a
supervisor. Keep slow work outside the callback by handing the report to an
application-owned queue.

Then use Deloxide locks where the competing paths meet. An opposite-order cycle
produces evidence like:

```text
source: WaitForGraph
thread_cycle: [ThreadId(2), ThreadId(3)]
thread_waiting_for_locks: [(ThreadId(2), LockId(7)), (ThreadId(3), LockId(4))]
```

Run the complete example:

```console
cargo run --example diagnose_deadlock
```

## Baseline Performance Evaluation

The baseline Deloxide evaluation tested isolated lock latency, heavily contended
Mutex and RwLock workloads, deterministic deadlock detection, timing-sensitive
manifestation, nine complex deadlock-free patterns, and a shared-state raytracer.

| Evaluated result                                 | PL+DD | ND | **Deloxide** |
|--------------------------------------------------| ---: | ---: | ---: |
| Mutex lock latency                               | 9.9 ns | 10,527 ns | **10.8 ns** |
| 1080p raytracing                                 | 18.32 s | 329.1 s | **16.67 s** |
| Average passive manifestation                    | 63.2% | 89.6% | 57.2% |
| Manifestation with stress testing                | N/A | N/A | **99.6%** |
| False deadlock reports across nine safe patterns | Zero | Zero | **Zero** |

The focused 1.1 microbenchmark measured an uncontended Deloxide Mutex at
**9.12 ns** and the same-harness `parking_lot` Mutex at **10.28 ns**.

The [performance chapter](https://emivvvvv.github.io/deloxide/production/performance.html)
contains the methodology, complete tables, limitations, and reproduction record.
Base paper: E. Tala and S. Baydere, *"Deloxide: Low-overhead real-time deadlock
detection and visualization framework for Rust,"* **Information and Software
Technology**, vol. 200, art. no. 108343, 2026 (`10.1016/j.infsof.2026.108343`).

## Features and modes

| Mode | Enable | Use it for |
| --- | --- | --- |
| Active wait-for detection | Default | Report a current validated cycle (Sharded WFG) |
| Cooperative deadlock recovery | Default | Recover via `Mutex::lock_recoverable()` |
| Custom callback | Default | Run application-defined incident handling |
| Logging and visualization | `logging-and-visualization` | Reconstruct the tracked event timeline |
| Lock-order analysis | `lock-order-graph` | Find potential acquisition-order risks |
| Random stress | `stress-test` + `with_random_stress()` | Broad schedule perturbation |
| Component stress | `stress-test` + `with_component_stress()` | Targeted reproduction delays |

`WaitForGraph` is active evidence. `LockOrderViolation` is a potential historical
risk; it does not mean threads are blocked now.

## Rust first, C supported

Rust is the primary interface. Deloxide also builds a C library using
[`include/deloxide.h`](include/deloxide.h), with tracked mutexes, RwLocks,
condition variables, callbacks, and thread registration.

- [User manual](https://emivvvvv.github.io/deloxide/)
- [Rust API](https://docs.rs/deloxide)
- [Examples](examples)
- [C guide](https://emivvvvv.github.io/deloxide/c-guide.html)
- [Choosing Deloxide or another tool](https://emivvvvv.github.io/deloxide/comparison.html)
- [Contributing](CONTRIBUTING.md)
- [Changelog](CHANGELOG.md)

## License

Deloxide is available under the [MIT License](LICENSE-MIT) or
[Apache License 2.0](LICENSE-APACHE).
