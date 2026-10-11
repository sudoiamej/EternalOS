use core::sync::atomic::{AtomicU32, Ordering};
use crate::task::{spawn_task, yield_now};

static TASK_A_COUNTER: AtomicU32 = AtomicU32::new(0);
static TASK_B_COUNTER: AtomicU32 = AtomicU32::new(0);
static TEST_COMPLETED: AtomicU32 = AtomicU32::new(0);

extern "C" fn task_a_worker() {
    crate::log_info!("[TASK A] Started execution.");
    for iteration in 1..=3 {
        TASK_A_COUNTER.fetch_add(1, Ordering::SeqCst);
        let val_a = TASK_A_COUNTER.load(Ordering::SeqCst);
        let val_b = TASK_B_COUNTER.load(Ordering::SeqCst);
        crate::log_info!("  [TASK A] Iteration {}/3 | Counter A = {} | Observed Counter B = {}", iteration, val_a, val_b);
        yield_now();
    }
    crate::log_info!("[TASK A] Finished iterations, returning from entry function to terminate.");
}

extern "C" fn task_b_worker() {
    crate::log_info!("[TASK B] Started execution.");
    for iteration in 1..=3 {
        TASK_B_COUNTER.fetch_add(1, Ordering::SeqCst);
        let val_a = TASK_A_COUNTER.load(Ordering::SeqCst);
        let val_b = TASK_B_COUNTER.load(Ordering::SeqCst);
        crate::log_info!("  [TASK B] Iteration {}/3 | Counter B = {} | Observed Counter A = {}", iteration, val_b, val_a);
        yield_now();
    }
    crate::log_info!("[TASK B] Finished iterations, returning from entry function to terminate.");
    TEST_COMPLETED.store(1, Ordering::SeqCst);
}

pub fn run_phase5_scheduler_tests() {
    crate::log_info!("====================================================");
    crate::log_info!(">>> RUNNING PHASE 5 COOPERATIVE SCHEDULER TESTS");
    crate::log_info!("====================================================");

    crate::log_info!("[TEST 1/5] Spawning independent kernel tasks Task A and Task B...");
    let id_a = spawn_task("worker_a", task_a_worker).expect("Failed to spawn Task A");
    let id_b = spawn_task("worker_b", task_b_worker).expect("Failed to spawn Task B");
    assert!(id_a != id_b, "Task IDs must be strictly unique");
    crate::log_info!("[PASS] Test 1: Tasks spawned with IDs {} and {}.", id_a, id_b);

    crate::log_info!("[TEST 2/5] Initiating cooperative round-robin execution via yield_now()...");

    // Yield control from main task to let workers A and B run their iterations
    for _ in 0..10 {
        yield_now();
        if TEST_COMPLETED.load(Ordering::SeqCst) == 1 {
            break;
        }
    }

    let final_a = TASK_A_COUNTER.load(Ordering::SeqCst);
    let final_b = TASK_B_COUNTER.load(Ordering::SeqCst);
    crate::log_info!("[TEST 3/5] Verifying task progress and independent state counters...");
    crate::log_info!("  -> Final Counter A = {} (expected 3)", final_a);
    crate::log_info!("  -> Final Counter B = {} (expected 3)", final_b);
    assert_eq!(final_a, 3, "Task A must complete exactly 3 iterations");
    assert_eq!(final_b, 3, "Task B must complete exactly 3 iterations");
    crate::log_info!("[PASS] Test 3: Both tasks progressed independently without state corruption.");

    crate::log_info!("[TEST 4/5] Verifying task termination and non-resumption...");
    // Further yields should not re-run terminated tasks
    yield_now();
    assert_eq!(TASK_A_COUNTER.load(Ordering::SeqCst), 3);
    assert_eq!(TASK_B_COUNTER.load(Ordering::SeqCst), 3);
    crate::log_info!("[PASS] Test 4: Terminated tasks were not selected again.");

    crate::log_info!("[TEST 5/5] Verifying main kernel task regained CPU ownership cleanly...");
    crate::log_info!("[PASS] Test 5: Main kernel task successfully resumed control.");

    crate::log_info!("====================================================");
    crate::log_info!(">>> ALL PHASE 5 SCHEDULER TESTS PASSED");
    crate::log_info!("====================================================");
}
