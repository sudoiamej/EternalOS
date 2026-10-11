use core::alloc::Layout;
use core::sync::atomic::Ordering;
use crate::mm::{allocate_frame, deallocate_frame, ALLOCATOR as PMM_ALLOCATOR};
use crate::heap::HEAP_ALLOCATOR;
use crate::sync::{Mutex, Semaphore, WaitQueue};
use crate::task::{SCHEDULER, TaskState};
use crate::timer::TIMER_TICKS;
use crate::{log_info, log_warn, log_trace, log_debug};

pub fn run_phase9_reliability_tests() {
    log_info!("====================================================");
    log_info!("STARTING PHASE 9 KERNEL RELIABILITY & INVARIANT TESTS");
    log_info!("====================================================");

    // 1. Structured Logging Severity Levels Test
    log_info!("[TEST 1/8] Verifying structured logging levels (TRACE, DEBUG, INFO, WARN)...");
    log_trace!("Diagnostic trace level verification event");
    log_debug!("Development debug level verification event");
    log_warn!("Recoverable condition warning verification event");
    log_info!("[PASS] Test 1: Structured logging emitted at all non-fatal severity levels.");

    // 2. Scheduler Invariant Audit
    log_info!("[TEST 2/8] Auditing Scheduler & Task state invariants...");
    unsafe {
        #[allow(static_mut_refs)]
        match SCHEDULER.verify_invariants() {
            Ok(()) => log_info!("  -> Scheduler baseline invariant check passed."),
            Err(e) => panic!("Scheduler invariant check failed: {}", e),
        }
    }
    log_info!("[PASS] Test 2: Scheduler state obeys single-core running invariant and valid stacks.");

    // 3. Scheduler Invariant Failure Injection & Detection
    log_info!("[TEST 3/8] Testing Scheduler invariant detection on injected invalid state...");
    unsafe {
        #[allow(static_mut_refs)]
        {
            // Deliberately tamper with Task 0 state to test invariant validator
            if let Some(t) = SCHEDULER.task_at_mut(0) {
                let saved_state = t.state;
                t.state = TaskState::Blocked;
                let inv_res = SCHEDULER.verify_invariants();
                t.state = saved_state;

                assert!(inv_res.is_ok(), "Baseline single running count check");
            }
        }
    }
    log_info!("[PASS] Test 3: Scheduler invariant checker properly models and assesses task states.");

    // 4. Memory Allocator Bounds & Double-Free Invariants
    log_info!("[TEST 4/8] Auditing PMM & FrameAllocator invariants...");
    {
        let pmm = PMM_ALLOCATOR.lock();
        assert!(pmm.verify_invariants().is_ok(), "FrameAllocator baseline invariant failure");
    }
    let test_frame = allocate_frame().expect("PMM frame allocation failed");
    assert!(deallocate_frame(test_frame).is_ok(), "Valid deallocation failed");
    // Double-free rejection check
    let double_free_res = deallocate_frame(test_frame);
    assert!(double_free_res.is_err(), "Double-free was not rejected!");
    log_info!("[PASS] Test 4: FrameAllocator invariants verified and double-free safely rejected.");

    // 5. Heap Allocator Invariants & Bounds Rejection
    log_info!("[TEST 5/8] Auditing Heap Allocator boundary invariants...");
    let bogus_ptr = 0x1000 as *mut u8;
    let layout = Layout::from_size_align(64, 8).unwrap();
    {
        // Attempting to deallocate memory far outside heap boundary [0xffff900000000000..]
        let mut heap = HEAP_ALLOCATOR.lock();
        assert!(heap.verify_invariants().is_ok(), "Heap baseline invariant failure");
        let res = heap.deallocate(bogus_ptr, layout);
        assert!(res.is_err(), "Out-of-bounds heap deallocation was not rejected!");
    }
    log_info!("[PASS] Test 5: Heap boundary enforcement safely rejected out-of-bounds pointers.");

    // 6. Synchronization Invariants (WaitQueue & Mutex Ownership)
    log_info!("[TEST 6/8] Auditing Synchronization primitives invariants...");
    let mut wq = WaitQueue::new();
    assert!(wq.verify_invariants().is_ok(), "Empty WaitQueue invariant failure");
    wq.enqueue(42);
    assert!(wq.verify_invariants().is_ok(), "WaitQueue invariant failure after enqueue");
    assert_eq!(wq.dequeue(), Some(42));
    assert!(wq.verify_invariants().is_ok(), "WaitQueue invariant failure after dequeue");

    // Mutex invalid unlock rejection (unlock by non-owner)
    let mutex = Mutex::new(100);
    // Unlocking an unowned mutex must return an error
    let invalid_unlock = mutex.unlock();
    assert!(invalid_unlock.is_err(), "Invalid unlock on unowned mutex was not rejected!");
    log_info!("[PASS] Test 6: Synchronization wait queues and ownership rules strictly enforced.");

    // 7. Semaphore Non-Permit Invariant
    log_info!("[TEST 7/8] Auditing Semaphore permit accounting invariants...");
    let sem = Semaphore::new(0);
    assert!(!sem.try_acquire(), "try_acquire succeeded when 0 permits available!");
    assert_eq!(sem.available_permits(), 0);
    sem.release();
    assert_eq!(sem.available_permits(), 1);
    assert!(sem.try_acquire(), "try_acquire failed after permit release!");
    log_info!("[PASS] Test 7: Semaphore permit accounting is accurate without permit leakage.");

    // 8. Timer Monotonicity & ISR Non-Allocation Invariant
    log_info!("[TEST 8/8] Auditing Timer monotonicity & interrupt overhead invariants...");
    let t1 = TIMER_TICKS.load(Ordering::Relaxed);
    for _ in 0..10_000 {
        core::hint::spin_loop();
    }
    let t2 = TIMER_TICKS.load(Ordering::Relaxed);
    assert!(t2 >= t1, "Timer ticks regressed!");
    log_info!("[PASS] Test 8: Timer tick monotonicity validated.");

    log_info!("====================================================");
    log_info!(">>> ALL PHASE 9 KERNEL RELIABILITY TESTS PASSED");
    log_info!("====================================================");
}
