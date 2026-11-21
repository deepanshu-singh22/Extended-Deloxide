use deloxide::{Mutex, RecoveryPolicy, get_recovery_policy, set_recovery_policy};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Barrier};
use std::thread;
use std::time::Duration;

#[test]
fn test_cooperative_deadlock_recovery_breaks_ab_ba_cycle() {
    set_recovery_policy(RecoveryPolicy::MinLocksHeld);
    assert_eq!(get_recovery_policy(), RecoveryPolicy::MinLocksHeld);

    let lock_a = Arc::new(Mutex::new(0usize));
    let lock_b = Arc::new(Mutex::new(0usize));
    let barrier = Arc::new(Barrier::new(2));
    let recoveries = Arc::new(AtomicUsize::new(0));

    let a1 = Arc::clone(&lock_a);
    let b1 = Arc::clone(&lock_b);
    let bar1 = Arc::clone(&barrier);
    let rec1 = Arc::clone(&recoveries);

    let t1 = thread::spawn(move || {
        let mut first_attempt = true;
        loop {
            let mut ga = a1.lock_recoverable().unwrap();
            if first_attempt {
                bar1.wait();
                first_attempt = false;
            }
            match b1.lock_recoverable() {
                Ok(mut gb) => {
                    *ga += 1;
                    *gb += 1;
                    break;
                }
                Err(err) => {
                    assert!(err.cycle.len() >= 2);
                    rec1.fetch_add(1, Ordering::SeqCst);
                    drop(ga);
                    thread::sleep(Duration::from_millis(5));
                }
            }
        }
    });

    let a2 = Arc::clone(&lock_a);
    let b2 = Arc::clone(&lock_b);
    let bar2 = Arc::clone(&barrier);
    let rec2 = Arc::clone(&recoveries);

    let t2 = thread::spawn(move || {
        let mut first_attempt = true;
        loop {
            let mut gb = b2.lock_recoverable().unwrap();
            if first_attempt {
                bar2.wait();
                first_attempt = false;
            }
            match a2.lock_recoverable() {
                Ok(mut ga) => {
                    *ga += 10;
                    *gb += 10;
                    break;
                }
                Err(err) => {
                    assert!(err.cycle.len() >= 2);
                    rec2.fetch_add(1, Ordering::SeqCst);
                    drop(gb);
                    thread::sleep(Duration::from_millis(5));
                }
            }
        }
    });

    t1.join().expect("Thread 1 should recover and finish");
    t2.join().expect("Thread 2 should recover and finish");

    assert_eq!(*lock_a.lock(), 11);
    assert_eq!(*lock_b.lock(), 11);
    assert!(
        recoveries.load(Ordering::SeqCst) >= 1,
        "At least one deadlock recovery should have occurred"
    );
}

#[test]
fn test_youngest_thread_policy_selection() {
    set_recovery_policy(RecoveryPolicy::YoungestThread);
    assert_eq!(get_recovery_policy(), RecoveryPolicy::YoungestThread);
}
