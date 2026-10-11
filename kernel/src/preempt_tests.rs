use core::sync::atomic::{AtomicU32, Ordering};
use crate::task::{spawn_task, PREEMPTION_COUNT};
use crate::timer::get_ticks;

static NON_YIELDING_TASK1_COUNTER: AtomicU32 = AtomicU32::new(0);
static NON_YIELDING_TASK2_COUNTER: AtomicU32 = AtomicU32::new(0);

// Task 1 deliberately performs compute work and NEVER calls yield_now()
extern "C" fn non_yielding_worker_1() {
    for _ in 0..10_000_000 {
        NON_YIELDING_TASK1_COUNTER.fetch_add(1, Ordering::Relaxed);
        // Busy compute loop: NO yield_now() called!
        core::hint::spin_loop();
    }
}

// Task 2 also deliberately performs compute work and NEVER calls yield_now()
extern "C" fn non_yielding_worker_2() {
    for _ in 0..10_000_000 {
        NON_YIELDING_TASK2_COUNTER.fetch_add(1, Ordering::Relaxed);
        // Busy compute loop: NO yield_now() called!
        core::hint::spin_loop();
    }
}

pub fn run_phase6_preemption_tests() {
    crate::log_info!("====================================================");
    crate::log_info!(">>> RUNNING PHASE 6 PREEMPTIVE SCHEDULING TESTS");
    crate::log_info!("====================================================");

    let initial_ticks = get_ticks();
    crate::log_info!("[TEST 1/5] Verifying hardware timer tick advancement...");
    // Spin with interrupts enabled until ticks advance
    let mut current_ticks = get_ticks();
    for _ in 0..10_000_000 {
        current_ticks = get_ticks();
        if current_ticks >= initial_ticks + 3 {
            break;
        }
        core::hint::spin_loop();
    }
    assert!(current_ticks > initial_ticks, "Timer ticks must advance while interrupts are enabled");
    crate::log_info!("  -> Initial Ticks = {}, Current Ticks = {}", initial_ticks, current_ticks);
    crate::log_info!("[PASS] Test 1: Hardware PIT timer ticks verified.");

    crate::log_info!("[TEST 2/5] Spawning two strictly non-yielding worker tasks...");
    let id1 = spawn_task("non_yield_1", non_yielding_worker_1).expect("Failed to spawn worker 1");
    let id2 = spawn_task("non_yield_2", non_yielding_worker_2).expect("Failed to spawn worker 2");
    crate::log_info!("[PASS] Test 2: Tasks spawned with IDs {} and {}.", id1, id2);

    crate::log_info!("[TEST 3/5] Observing preemptive time-slicing without voluntary yields...");
    let start_ticks = get_ticks();
    while get_ticks() < start_ticks + 20 {
        core::hint::spin_loop();
    }

    let val1 = NON_YIELDING_TASK1_COUNTER.load(Ordering::Relaxed);
    let val2 = NON_YIELDING_TASK2_COUNTER.load(Ordering::Relaxed);
    let preemptions = PREEMPTION_COUNT.load(Ordering::Relaxed);

    crate::log_info!("  -> Progress of Task 1 (No yields): {} cycles", val1);
    crate::log_info!("  -> Progress of Task 2 (No yields): {} cycles", val2);
    crate::log_info!("  -> Preemption events registered : {}", preemptions);

    assert!(val1 > 0, "Task 1 must make progress due to preemption");
    assert!(val2 > 0, "Task 2 must make progress due to preemption");
    crate::log_info!("[PASS] Test 3: Both non-yielding tasks made concurrent progress via timer preemption.");

    crate::log_info!("[TEST 4/5] Verifying sustained context integrity across preemptions...");
    assert!(preemptions > 0, "At least one preemption event must be registered");
    crate::log_info!("[PASS] Test 4: Preemptive context switches occurred without kernel faults.");

    crate::log_info!("[TEST 5/5] Testing cooperative compatibility with active timer interrupts...");
    crate::task::yield_now();
    crate::log_info!("[PASS] Test 5: Cooperative yield_now() executed cleanly while timer is running.");

    crate::log_info!("====================================================");
    crate::log_info!(">>> ALL PHASE 6 PREEMPTION TESTS PASSED");
    crate::log_info!("====================================================");
}
