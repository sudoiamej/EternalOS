use core::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use crate::sync::{Mutex, Semaphore, WaitQueue, SpinLock};
use crate::task::{current_task_id, spawn_task, yield_now, block_current_task, wake_task};
use crate::timer::get_ticks;

// Test state structures
static SHARED_MUTEX: Mutex<u32> = Mutex::new(0);
static TEST_SEMAPHORE: Semaphore = Semaphore::new(1); // 1 initial permit
static TEST_WAIT_QUEUE: SpinLock<WaitQueue> = SpinLock::new(WaitQueue::new());

static TASK1_MUTEX_PROGRESS: AtomicU32 = AtomicU32::new(0);
static TASK2_MUTEX_PROGRESS: AtomicU32 = AtomicU32::new(0);
static TASK_BLOCKED_AND_WOKEN: AtomicBool = AtomicBool::new(false);
static SEMAPHORE_CONSUMER_DONE: AtomicBool = AtomicBool::new(false);

extern "C" fn mutex_worker_1() {
    for _ in 0..5 {
        let mut guard = SHARED_MUTEX.lock();
        *guard += 1;
        TASK1_MUTEX_PROGRESS.fetch_add(1, Ordering::SeqCst);
        // Yield while holding or immediately after to exercise contention
        yield_now();
    }
}

extern "C" fn mutex_worker_2() {
    for _ in 0..5 {
        let mut guard = SHARED_MUTEX.lock();
        *guard += 1;
        TASK2_MUTEX_PROGRESS.fetch_add(1, Ordering::SeqCst);
        yield_now();
    }
}

extern "C" fn semaphore_consumer() {
    // Acquire permit (should succeed first time)
    TEST_SEMAPHORE.acquire();
    crate::log_info!("  [SEM CONSUMER] Acquired first permit.");

    // Acquire second permit (permits was 0, now -1, must block until released)
    crate::log_info!("  [SEM CONSUMER] Attempting second acquire (will block)...");
    TEST_SEMAPHORE.acquire();
    crate::log_info!("  [SEM CONSUMER] Successfully woken and acquired second permit!");
    SEMAPHORE_CONSUMER_DONE.store(true, Ordering::SeqCst);
}

extern "C" fn waitqueue_sleeper() {
    let my_id = current_task_id();
    crate::log_info!("  [WQ SLEEPER] Task {} enqueueing and blocking on wait queue...", my_id);
    {
        let mut wq = TEST_WAIT_QUEUE.lock();
        wq.enqueue(my_id);
    }
    block_current_task();
    crate::log_info!("  [WQ SLEEPER] Task {} woken up successfully!", my_id);
    TASK_BLOCKED_AND_WOKEN.store(true, Ordering::SeqCst);
}

pub fn run_phase7_sync_tests() {
    crate::log_info!("====================================================");
    crate::log_info!(">>> RUNNING PHASE 7 TASK SYNCHRONIZATION TESTS");
    crate::log_info!("====================================================");

    // TEST 1: Mutex uncontended lock & unlock
    crate::log_info!("[TEST 1/8] Verifying uncontended Mutex lock and unlock...");
    assert!(!SHARED_MUTEX.is_locked());
    {
        let mut guard = SHARED_MUTEX.lock();
        assert!(SHARED_MUTEX.is_locked());
        *guard = 42;
        assert_eq!(*guard, 42);
    }
    assert!(!SHARED_MUTEX.is_locked());
    crate::log_info!("[PASS] Test 1: Uncontended Mutex lock/unlock succeeded.");

    // TEST 2: Mutex invalid unlock error detection
    crate::log_info!("[TEST 2/8] Testing invalid unlock detection...");
    // Main task doesn't hold the mutex, unlock should return Err
    assert!(SHARED_MUTEX.unlock().is_err());
    crate::log_info!("[PASS] Test 2: Invalid unlock rejected deterministically.");

    // TEST 3: Multi-task Mutex contention and mutual exclusion
    crate::log_info!("[TEST 3/8] Spawning two contending tasks for shared Mutex...");
    *SHARED_MUTEX.lock() = 0; // Reset counter
    let _id1 = spawn_task("mutex_worker_1", mutex_worker_1).expect("Failed to spawn worker 1");
    let _id2 = spawn_task("mutex_worker_2", mutex_worker_2).expect("Failed to spawn worker 2");

    let timeout_ticks = get_ticks() + 100; // Watchdog: 1 second
    while TASK1_MUTEX_PROGRESS.load(Ordering::SeqCst) < 5 || TASK2_MUTEX_PROGRESS.load(Ordering::SeqCst) < 5 {
        yield_now();
        if get_ticks() > timeout_ticks {
            panic!("Watchdog timeout: Mutex contention test deadlocked!");
        }
    }

    let final_counter = *SHARED_MUTEX.lock();
    crate::log_info!("  -> Final Protected Counter Value: {} (expected 10)", final_counter);
    assert_eq!(final_counter, 10, "Protected counter must exactly equal 10");
    crate::log_info!("[PASS] Test 3: Mutex mutual exclusion verified with {} cycles.", final_counter);

    // TEST 4: Counting Semaphore initial permits & try_acquire
    crate::log_info!("[TEST 4/8] Testing Semaphore permit accounting & try_acquire...");
    let sem = Semaphore::new(2);
    assert_eq!(sem.available_permits(), 2);
    assert!(sem.try_acquire());
    assert_eq!(sem.available_permits(), 1);
    assert!(sem.try_acquire());
    assert_eq!(sem.available_permits(), 0);
    assert!(!sem.try_acquire(), "try_acquire must fail when permits are 0");
    sem.release();
    assert_eq!(sem.available_permits(), 1);
    assert!(sem.try_acquire());
    crate::log_info!("[PASS] Test 4: Semaphore permit tracking verified.");

    // TEST 5: Semaphore blocking and wakeup
    crate::log_info!("[TEST 5/8] Testing Semaphore blocking exhaustion and release wakeup...");
    let _id3 = spawn_task("sem_consumer", semaphore_consumer).expect("Failed to spawn sem consumer");

    // Allow consumer to run and block on second acquire
    let blocked_ticks = get_ticks() + 10;
    while get_ticks() < blocked_ticks {
        yield_now();
    }

    assert!(!SEMAPHORE_CONSUMER_DONE.load(Ordering::SeqCst), "Consumer must be blocked on semaphore");
    crate::log_info!("  -> Consumer is confirmed blocked. Releasing permit...");
    TEST_SEMAPHORE.release();

    let timeout_ticks = get_ticks() + 50;
    while !SEMAPHORE_CONSUMER_DONE.load(Ordering::SeqCst) {
        yield_now();
        if get_ticks() > timeout_ticks {
            panic!("Watchdog timeout: Semaphore consumer failed to wake up!");
        }
    }
    crate::log_info!("[PASS] Test 5: Semaphore block and wakeup verified.");

    // TEST 6: WaitQueue block and wake
    crate::log_info!("[TEST 6/8] Testing WaitQueue explicit block and wake...");
    let id4 = spawn_task("wq_sleeper", waitqueue_sleeper).expect("Failed to spawn wq sleeper");

    let wait_ticks = get_ticks() + 10;
    while get_ticks() < wait_ticks {
        yield_now();
    }
    assert!(!TASK_BLOCKED_AND_WOKEN.load(Ordering::SeqCst), "Sleeper must be blocked");

    // Dequeue sleeper and wake it
    let target_task = {
        let mut wq = TEST_WAIT_QUEUE.lock();
        wq.dequeue()
    };
    assert_eq!(target_task, Some(id4), "Wait queue must have dequeued sleeper task ID");
    let woken = wake_task(id4);
    assert!(woken, "wake_task must return true for blocked task");

    let timeout_ticks = get_ticks() + 50;
    while !TASK_BLOCKED_AND_WOKEN.load(Ordering::SeqCst) {
        yield_now();
        if get_ticks() > timeout_ticks {
            panic!("Watchdog timeout: WaitQueue task failed to resume!");
        }
    }
    crate::log_info!("[PASS] Test 6: WaitQueue block and wake verified.");

    // TEST 7: Waking non-blocked / invalid task ID
    crate::log_info!("[TEST 7/8] Testing edge cases (invalid wakeups)...");
    assert!(!wake_task(999), "Waking nonexistent task must return false");
    assert!(!wake_task(0), "Waking already running task must return false");
    crate::log_info!("[PASS] Test 7: Invalid wakeups handled deterministically.");

    // TEST 8: Repeated scheduling & Timer preemption resilience
    crate::log_info!("[TEST 8/8] Testing synchronization resilience under active timer interrupts...");
    let start_ticks = get_ticks();
    while get_ticks() < start_ticks + 15 {
        // Repeatedly lock and unlock in main loop with interrupts firing
        let mut guard = SHARED_MUTEX.lock();
        *guard += 1;
        core::hint::spin_loop();
    }
    crate::log_info!("[PASS] Test 8: Mutex stable across continuous hardware timer interrupts.");

    crate::log_info!("====================================================");
    crate::log_info!(">>> ALL PHASE 7 SYNCHRONIZATION TESTS PASSED");
    crate::log_info!("====================================================");
}
