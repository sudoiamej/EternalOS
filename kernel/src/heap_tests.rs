use core::alloc::Layout;
use crate::heap::{HEAP_ALLOCATOR, HEAP_START, HEAP_INITIAL_SIZE};

pub fn run_phase4_heap_tests() {
    crate::log_info!("====================================================");
    crate::log_info!(">>> RUNNING PHASE 4 KERNEL HEAP TESTS");
    crate::log_info!("====================================================");

    // Test 1: Small Object Allocation & Writable Verification
    crate::log_info!("[TEST 1/8] Allocating small 64-byte buffer and verifying memory write...");
    let layout_64 = Layout::from_size_align(64, 8).unwrap();
    let ptr1 = HEAP_ALLOCATOR.lock().allocate(layout_64).expect("Test 1: Failed to allocate 64 bytes");
    assert!(!ptr1.is_null(), "Allocated pointer must not be null");
    assert!((ptr1 as usize) >= HEAP_START && (ptr1 as usize) < HEAP_START + HEAP_INITIAL_SIZE);

    // Write to memory to prove it is backed by physical RAM and writable
    unsafe {
        core::ptr::write_bytes(ptr1, 0xAA, 64);
        assert_eq!(*ptr1, 0xAA);
        assert_eq!(*ptr1.add(63), 0xAA);
    }
    crate::log_info!("  -> Allocated at {:#018x}, written and verified.", ptr1 as usize);
    crate::log_info!("[PASS] Test 1: Small object allocation & memory write successful.");

    // Test 2: Multi-Alignment Requirements (8, 16, 64, 128 bytes)
    crate::log_info!("[TEST 2/8] Testing multiple power-of-two alignment guarantees (16, 64, 128)...");
    let layout_align16 = Layout::from_size_align(32, 16).unwrap();
    let ptr_align16 = HEAP_ALLOCATOR.lock().allocate(layout_align16).unwrap();
    assert_eq!((ptr_align16 as usize) % 16, 0, "Address must be 16-byte aligned");

    let layout_align64 = Layout::from_size_align(128, 64).unwrap();
    let ptr_align64 = HEAP_ALLOCATOR.lock().allocate(layout_align64).unwrap();
    assert_eq!((ptr_align64 as usize) % 64, 0, "Address must be 64-byte aligned");

    let layout_align128 = Layout::from_size_align(256, 128).unwrap();
    let ptr_align128 = HEAP_ALLOCATOR.lock().allocate(layout_align128).unwrap();
    assert_eq!((ptr_align128 as usize) % 128, 0, "Address must be 128-byte aligned");
    crate::log_info!("[PASS] Test 2: Custom alignments verified (16, 64, 128).");

    // Test 3: Multiple Live Allocations Non-Overlapping
    crate::log_info!("[TEST 3/8] Verifying multiple allocations do not overlap...");
    let p_a = HEAP_ALLOCATOR.lock().allocate(Layout::from_size_align(128, 8).unwrap()).unwrap();
    let p_b = HEAP_ALLOCATOR.lock().allocate(Layout::from_size_align(128, 8).unwrap()).unwrap();
    let addr_a = p_a as usize;
    let addr_b = p_b as usize;
    assert!(addr_a + 128 <= addr_b || addr_b + 128 <= addr_a, "Buffers must never overlap");
    crate::log_info!("  -> Buffer A: {:#018x}, Buffer B: {:#018x}", addr_a, addr_b);
    crate::log_info!("[PASS] Test 3: Non-overlapping ranges verified.");

    // Test 4: Deallocation and Memory Reuse
    crate::log_info!("[TEST 4/8] Testing deallocation and memory reuse...");
    let test_layout = Layout::from_size_align(512, 16).unwrap();
    let ptr_reuse1 = HEAP_ALLOCATOR.lock().allocate(test_layout).unwrap();
    HEAP_ALLOCATOR.lock().deallocate(ptr_reuse1, test_layout).expect("Deallocation failed");
    let ptr_reuse2 = HEAP_ALLOCATOR.lock().allocate(test_layout).unwrap();
    assert_eq!(ptr_reuse1, ptr_reuse2, "Allocating after free should reuse available block");
    crate::log_info!("[PASS] Test 4: Deallocation and immediate memory reuse confirmed.");

    // Test 5: Out-of-Bounds Free Rejection
    crate::log_info!("[TEST 5/8] Testing invalid pointer deallocation rejection...");
    let invalid_ptr = 0x0000_1234_5678_0000 as *mut u8;
    let dealloc_res = HEAP_ALLOCATOR.lock().deallocate(invalid_ptr, Layout::from_size_align(16, 8).unwrap());
    assert!(dealloc_res.is_err(), "Out-of-bounds pointer must be rejected");
    crate::log_info!("  -> Correctly rejected invalid free with: {:?}", dealloc_res.unwrap_err());
    crate::log_info!("[PASS] Test 5: Invalid deallocation correctly rejected.");

    // Test 6: Heap Exhaustion Behavior
    crate::log_info!("[TEST 6/8] Testing heap exhaustion behavior...");
    let huge_layout = Layout::from_size_align(HEAP_INITIAL_SIZE * 2, 8).unwrap();
    let exhaust_res = HEAP_ALLOCATOR.lock().allocate(huge_layout);
    assert!(exhaust_res.is_err(), "Allocating more than heap capacity must fail gracefully");
    crate::log_info!("  -> Correctly handled exhaustion with: {:?}", exhaust_res.unwrap_err());
    crate::log_info!("[PASS] Test 6: Deterministic failure on heap exhaustion verified.");

    // Test 7: Zero-Sized Allocation
    crate::log_info!("[TEST 7/8] Testing zero-sized allocation request handling...");
    let zero_layout = Layout::from_size_align(0, 1).unwrap();
    let zero_ptr = HEAP_ALLOCATOR.lock().allocate(zero_layout).unwrap();
    assert!(!zero_ptr.is_null());
    HEAP_ALLOCATOR.lock().deallocate(zero_ptr, zero_layout).unwrap();
    crate::log_info!("[PASS] Test 7: Zero-sized allocation safely handled.");

    // Test 8: Cleanup of Live Test Allocations
    crate::log_info!("[TEST 8/8] Cleaning up allocated test blocks...");
    HEAP_ALLOCATOR.lock().deallocate(ptr1, layout_64).unwrap();
    HEAP_ALLOCATOR.lock().deallocate(ptr_align16, layout_align16).unwrap();
    HEAP_ALLOCATOR.lock().deallocate(ptr_align64, layout_align64).unwrap();
    HEAP_ALLOCATOR.lock().deallocate(ptr_align128, layout_align128).unwrap();
    HEAP_ALLOCATOR.lock().deallocate(p_a, Layout::from_size_align(128, 8).unwrap()).unwrap();
    HEAP_ALLOCATOR.lock().deallocate(p_b, Layout::from_size_align(128, 8).unwrap()).unwrap();
    HEAP_ALLOCATOR.lock().deallocate(ptr_reuse2, test_layout).unwrap();

    let (cap, alloc_bytes, free_bytes) = HEAP_ALLOCATOR.lock().stats();
    crate::log_info!("  -> Heap Stats: Cap: {} KiB | Total Allocated: {} B | Total Freed: {} B", cap / 1024, alloc_bytes, free_bytes);
    crate::log_info!("[PASS] Test 8: All test allocations cleaned up cleanly.");

    crate::log_info!("====================================================");
    crate::log_info!(">>> ALL PHASE 4 HEAP ALLOCATOR TESTS PASSED");
    crate::log_info!("====================================================");
}
