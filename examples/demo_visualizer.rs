use deloxide::{Deloxide, Mutex, showcase_this};
use std::sync::{Arc, Barrier};
use std::thread;
use std::time::Duration;

fn main() {
    Deloxide::new()
        .with_log("deadlock_demo.json")
        .callback(|info| {
            println!("Deadlock detected! Opening web visualizer...");
            println!("Cycle: {:?}", info.thread_cycle);
            showcase_this().expect("Failed to launch web visualizer");
            std::process::exit(0);
        })
        .start()
        .expect("Failed to start Deloxide");

    let a = Arc::new(Mutex::new(1));
    let b = Arc::new(Mutex::new(2));
    let barrier = Arc::new(Barrier::new(2));

    let (a1, b1, bar1) = (Arc::clone(&a), Arc::clone(&b), Arc::clone(&barrier));
    let t1 = thread::spawn(move || {
        let _ga = a1.lock();
        bar1.wait();
        let _gb = b1.lock();
    });

    let (a2, b2, bar2) = (Arc::clone(&a), Arc::clone(&b), Arc::clone(&barrier));
    let t2 = thread::spawn(move || {
        let _gb = b2.lock();
        bar2.wait();
        let _ga = a2.lock();
    });

    thread::sleep(Duration::from_secs(5));
    let _ = (t1.join(), t2.join());
}
