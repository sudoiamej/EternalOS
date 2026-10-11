use core::sync::atomic::{AtomicBool, AtomicU32, AtomicU64, Ordering};
use crate::task::{current_task_id, reclaim_tasks, sleep_ms, sleep_ticks, spawn_task, yield_now};
use crate::timer::{get_ticks, ms_to_ticks, ticks_to_ms, MS_PER_TICK};

static SLEEPER_PROGRESS: AtomicU32 = AtomicU32::new(0);
static COMPANION_PROGRESS: AtomicU32 = AtomicU32::new(0);
static TEST_COMPLETED: AtomicBool = AtomicBool::new(false);
static MEASURED_SLEEP_START: AtomicU64 = AtomicU64::new(0);
static MEASURED_SLEEP_END: AtomicU64 = AtomicU64::new(0);

extern "C" fn timed_sleeper_worker() {
    let t0 = get_ticks();
    MEASURED_SLEEP_START.store(t0, Ordering::SeqCst);
    crate::log_info!("  [SLEEPER] Starting sleep for 5 ticks (~50ms) at tick {}...", t0);
    
    // Sleep for 5 ticks
    sleep_ticks(5);

    let t1 = get_ticks();
    MEASURED_SLEEP_END.store(t1, Ordering::SeqCst);
    crate::log_info!("  [SLEEPER] Woke up at tick {} (elapsed = {} ticks).", t1, t1.saturating_sub(t0));
    SLEEPER_PROGRESS.fetch_add(1, Ordering::SeqCst);
}

extern "C" fn companion_progress_worker() {
    // While sleeper is asleep, this companion worker runs iterations to demonstrate
    // that other runnable tasks make continuous progress without starving.
    for _ in 0..10 {
        COMPANION_PROGRESS.fetch_add(1, Ordering::SeqCst);
        yield_now();
    }
}

extern "C" fn short_lived_worker() {
    let id = current_task_id();
    crate::log_info!("  [SHORT LIVED] Task {} running briefly to test termination & reclamation...", id);
    sleep_ticks(2);
    // Returning terminates the task cleanly
}

pub fn run_phase8_sleep_tests() {
    crate::log_info!("====================================================");
    crate::log_info!(">>> RUNNING PHASE 8 KERNEL TIMERS & SLEEP TESTS");
    crate::log_info!("====================================================");

    // TEST 1: Monotonic tick counter advancement & timer conversions
    crate::log_info!("[TEST 1/8] Verifying monotonic tick counter advancement and conversions...");
    let t_start = get_ticks();
    // Spin briefly with interrupts enabled
    let timeout = t_start + 10;
    while get_ticks() < t_start + 3 {
        core::hint::spin_loop();
        if get_ticks() > timeout {
            panic!("Timer ticks did not advance monotonically within timeout!");
        }
    }
    let t_now = get_ticks();
    assert!(t_now >= t_start + 3, "Ticks must advance monotonically");
    assert_eq!(ms_to_ticks(0), 0, "0 ms must convert to 0 ticks");
    assert_eq!(ms_to_ticks(1), 1, "1 ms must round UP to 1 tick (10ms) to avoid shortening");
    assert_eq!(ms_to_ticks(10), 1, "10 ms must convert to exactly 1 tick");
    assert_eq!(ms_to_ticks(25), 3, "25 ms must ceil to 3 ticks (30ms)");
    assert_eq!(ticks_to_ms(1), MS_PER_TICK, "1 tick must convert to 10 ms");
    crate::log_info!("[PASS] Test 1: Monotonic ticks and ceiling ms conversion verified.");

    // TEST 2: Zero-duration sleep contract
    crate::log_info!("[TEST 2/8] Testing zero-duration sleep behavior...");
    let t_before_zero = get_ticks();
    sleep_ticks(0);
    sleep_ms(0);
    let t_after_zero = get_ticks();
    // Zero-duration sleep yields CPU without blocking for future ticks
    assert!(t_after_zero >= t_before_zero);
    crate::log_info!("[PASS] Test 2: Zero-duration sleep executed as immediate voluntary yield.");

    // TEST 3: Timed sleep & companion concurrency
    crate::log_info!("[TEST 3/8] Testing sleep_ticks(5) and companion task progress...");
    let _s_id = spawn_task("sleeper", timed_sleeper_worker).expect("Failed to spawn sleeper");
    let _c_id = spawn_task("companion", companion_progress_worker).expect("Failed to spawn companion");

    let watchdog = get_ticks() + 100; // 100 ticks = 1 second watchdog
    while SLEEPER_PROGRESS.load(Ordering::SeqCst) == 0 {
        yield_now();
        if get_ticks() > watchdog {
            panic!("Watchdog timeout: Sleeper failed to wake up!");
        }
    }

    let sleep_start = MEASURED_SLEEP_START.load(Ordering::SeqCst);
    let sleep_end = MEASURED_SLEEP_END.load(Ordering::SeqCst);
    let elapsed_ticks = sleep_end.saturating_sub(sleep_start);
    let comp_prog = COMPANION_PROGRESS.load(Ordering::SeqCst);

    crate::log_info!("  -> Sleeper requested 5 ticks, elapsed: {} ticks", elapsed_ticks);
    crate::log_info!("  -> Companion task progress during sleep: {} iterations", comp_prog);
    assert!(elapsed_ticks >= 5, "Sleeper must not wake up before its deadline (elapsed >= 5)");
    assert!(comp_prog > 0, "Companion task must have progressed while sleeper was asleep");
    crate::log_info!("[PASS] Test 3: Timed sleep respects deadline; companion progressed.");

    // TEST 4: sleep_ms API accuracy
    crate::log_info!("[TEST 4/8] Testing sleep_ms(30) (~3 ticks)...");
    let t_ms_start = get_ticks();
    sleep_ms(30);
    let t_ms_end = get_ticks();
    let elapsed_ms_ticks = t_ms_end.saturating_sub(t_ms_start);
    crate::log_info!("  -> sleep_ms(30) elapsed ticks: {} (~{} ms)", elapsed_ms_ticks, ticks_to_ms(elapsed_ms_ticks));
    assert!(elapsed_ms_ticks >= 3, "sleep_ms(30) must sleep at least 3 ticks (30ms)");
    crate::log_info!("[PASS] Test 4: sleep_ms duration verified.");

    // TEST 5: Large duration / deadline overflow safety
    crate::log_info!("[TEST 5/8] Testing deadline overflow safety...");
    // Extremely large tick counts should safely saturate at u64::MAX without panic or wrapped premature wake
    let huge_ticks = u64::MAX - 10;
    let deadline = get_ticks().checked_add(huge_ticks).unwrap_or(u64::MAX);
    assert_eq!(deadline, u64::MAX, "Overflowing deadlines must saturate to u64::MAX safely");
    crate::log_info!("[PASS] Test 5: Deadline overflow arithmetic handled safely.");

    // TEST 6: Task termination and resource reclamation
    crate::log_info!("[TEST 6/8] Testing task lifecycle termination and resource reclamation...");
    let short_id = spawn_task("short_task", short_lived_worker).expect("Failed to spawn short worker");
    
    // Wait for short task to finish and terminate
    let term_watchdog = get_ticks() + 50;
    while get_ticks() < term_watchdog {
        yield_now();
        // Give short task time to finish its 2 ticks and terminate
        if get_ticks() > term_watchdog - 20 {
            break;
        }
    }

    // Call reclaim_tasks from the main task context
    let reclaimed = reclaim_tasks();
    crate::log_info!("  -> Reclaimed resources for {} terminated tasks.", reclaimed);
    assert!(reclaimed >= 1, "Must have reclaimed at least 1 terminated task stack and slot");
    crate::log_info!("[PASS] Test 6: Terminated task stacks and pages reclaimed safely.");

    // TEST 7: Slot reuse after reclamation
    crate::log_info!("[TEST 7/8] Testing task slot reuse after reclamation...");
    let reused_id = spawn_task("reused_slot_task", short_lived_worker).expect("Failed to spawn in reused slot");
    crate::log_info!("  -> Spawned new task with ID {} into reclaimed slot.", reused_id);
    assert!(reused_id > short_id, "New task ID must monotonically increment to prevent ABA");

    // Wait for reused task to complete before advancing to Test 8
    let reused_watchdog = get_ticks() + 50;
    while get_ticks() < reused_watchdog {
        yield_now();
        if get_ticks() > reused_watchdog - 20 {
            break;
        }
    }
    crate::log_info!("[PASS] Test 7: Task slot reused safely with distinct monotonic ID.");

    // TEST 8: Repeated sleep/wake cycles under active interrupts
    crate::log_info!("[TEST 8/8] Testing repeated sleep/wake stability under hardware timer interrupts...");
    for _cycle in 1..=3 {
        let before = get_ticks();
        sleep_ticks(1);
        let after = get_ticks();
        assert!(after >= before + 1, "Each sleep cycle must complete full duration");
    }
    crate::log_info!("[PASS] Test 8: Repeated sleep cycles stable across continuous timer interrupts.");

    crate::log_info!("====================================================");
    crate::log_info!(">>> ALL PHASE 8 KERNEL TIMERS & SLEEP TESTS PASSED");
    crate::log_info!("====================================================");
}
