use crate::mm::{allocate_frame, deallocate_frame};
use crate::vm::VirtAddr;

pub fn run_phase3_memory_tests() {
    crate::log_info!("====================================================");
    crate::log_info!(">>> RUNNING PHASE 3 MEMORY MANAGEMENT TESTS");
    crate::log_info!("====================================================");

    // Test 1: Frame Alignment and Uniqueness
    crate::log_info!("[TEST 1/6] Allocating 3 frames to verify alignment and uniqueness...");
    let f1 = allocate_frame().expect("Test 1: Failed to allocate frame 1");
    let f2 = allocate_frame().expect("Test 1: Failed to allocate frame 2");
    let f3 = allocate_frame().expect("Test 1: Failed to allocate frame 3");

    assert!(f1.is_aligned(), "Test 1: Frame 1 must be 4KiB aligned");
    assert!(f2.is_aligned(), "Test 1: Frame 2 must be 4KiB aligned");
    assert!(f3.is_aligned(), "Test 1: Frame 3 must be 4KiB aligned");
    assert!(f1 != f2 && f2 != f3 && f1 != f3, "Test 1: Frames must be strictly unique");
    crate::log_info!("  -> Frame 1: {:#018x}", f1.as_u64());
    crate::log_info!("  -> Frame 2: {:#018x}", f2.as_u64());
    crate::log_info!("  -> Frame 3: {:#018x}", f3.as_u64());
    crate::log_info!("[PASS] Test 1: Frames are 4KiB aligned and unique.");

    // Test 2: Allocation and Deallocation Cycle
    crate::log_info!("[TEST 2/6] Deallocating Frame 2 and reallocating to verify reuse...");
    deallocate_frame(f2).expect("Test 2: Deallocating f2 failed");
    let f2_reallocated = allocate_frame().expect("Test 2: Reallocating failed");
    assert_eq!(f2, f2_reallocated, "Test 2: Reallocated frame should match freed frame");
    crate::log_info!("[PASS] Test 2: Allocation and deallocation cycle verified.");

    // Test 3: Double-Free Rejection
    crate::log_info!("[TEST 3/6] Testing double-free rejection...");
    deallocate_frame(f1).expect("Test 3: First deallocation of f1 failed");
    let double_free_res = deallocate_frame(f1);
    assert!(double_free_res.is_err(), "Test 3: Double-free should be rejected");
    crate::log_info!("  -> Correctly rejected double-free with: {:?}", double_free_res.unwrap_err());
    crate::log_info!("[PASS] Test 3: Double-free rejection verified.");

    // Test 4: Invalid and Reserved Free Rejection
    crate::log_info!("[TEST 4/6] Testing invalid & reserved address deallocation rejection...");
    let unaligned = crate::vm::PhysAddr::new(f3.as_u64() + 13);
    assert!(deallocate_frame(unaligned).is_err(), "Test 4: Unaligned address must be rejected");

    let out_of_bounds = crate::vm::PhysAddr::new(0xffff_ffff_ffff_f000);
    assert!(deallocate_frame(out_of_bounds).is_err(), "Test 4: Out-of-bounds address must be rejected");
    crate::log_info!("[PASS] Test 4: Unaligned and out-of-bounds deallocations rejected.");

    // Test 5: Virtual Address Canonical and Index Validation
    crate::log_info!("[TEST 5/6] Validating Virtual Address canonical parsing and indexing...");
    let kernel_virt = VirtAddr::new(0xffffffff80000000);
    assert!(kernel_virt.is_canonical(), "Kernel higher-half must be canonical");
    assert_eq!(kernel_virt.p4_index(), 511, "Kernel higher-half P4 index must be 511");

    let non_canonical = VirtAddr::new(0x0001_8000_0000_0000);
    assert!(!non_canonical.is_canonical(), "Address with mixed bits 47..63 must be non-canonical");
    crate::log_info!("[PASS] Test 5: Virtual address canonical forms verified.");

    // Test 6: Clean Deallocation of remaining test frames
    crate::log_info!("[TEST 6/6] Cleaning up remaining test frames...");
    deallocate_frame(f2_reallocated).expect("Test 6: Deallocation failed");
    deallocate_frame(f3).expect("Test 6: Deallocation failed");
    crate::log_info!("[PASS] Test 6: All allocated test frames returned cleanly.");

    crate::log_info!("====================================================");
    crate::log_info!(">>> ALL PHASE 3 MEMORY TESTS PASSED SUCCESSFULLY");
    crate::log_info!("====================================================");
}
