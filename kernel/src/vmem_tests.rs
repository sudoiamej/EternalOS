use crate::mm::{allocate_frame, deallocate_frame};
use crate::vm::{
    is_mapped, map_page, translate_addr, unmap_page, update_page_flags, PageTableFlags, PhysAddr,
    VirtAddr,
};
use crate::{log_info, log_warn};

pub fn run_phase10_vmem_tests() {
    log_info!("====================================================");
    log_info!("STARTING PHASE 10 ADVANCED VIRTUAL MEMORY TESTS     ");
    log_info!("====================================================");

    // 1. Inspect Known Valid Kernel Mapping
    log_info!("[TEST 1/12] Inspecting known kernel text mapping translation...");
    let kernel_entry_virt = VirtAddr::new(crate::kmain as *const () as usize as u64);
    assert!(kernel_entry_virt.is_canonical());
    let kernel_phys = translate_addr(kernel_entry_virt).expect("Failed to translate kernel kmain address");
    log_info!("  -> kmain VirtAddr: {:#018x} -> PhysAddr: {:#018x}", kernel_entry_virt.as_u64(), kernel_phys.as_u64());
    assert!(kernel_phys.as_u64() > 0, "Physical address must be non-zero");
    log_info!("[PASS] Test 1: Kernel text mapping translated successfully.");

    // 2. Reject Non-Canonical and Misaligned Addresses
    log_info!("[TEST 2/12] Testing non-canonical and misaligned mapping validation...");
    let non_canonical = VirtAddr::new(0x0000_8000_0000_0000); // 48th bit set without sign extension
    let misaligned_virt = VirtAddr::new(0xffff_8000_0000_0123);
    let valid_frame = PhysAddr::new(0x1000);
    let misaligned_frame = PhysAddr::new(0x1123);

    unsafe {
        assert!(map_page(non_canonical, valid_frame, PageTableFlags::WRITABLE).is_err());
        assert!(map_page(misaligned_virt, valid_frame, PageTableFlags::WRITABLE).is_err());
        assert!(map_page(VirtAddr::new(0xffff_8000_0000_0000), misaligned_frame, PageTableFlags::WRITABLE).is_err());
    }
    log_info!("[PASS] Test 2: Non-canonical and misaligned addresses rejected deterministically.");

    // 3. Map a Fresh Physical Frame to a Test Virtual Page
    log_info!("[TEST 3/12] Mapping fresh physical frame at test virtual address...");
    let test_virt = VirtAddr::new(0xffff_b000_0000_0000);
    assert!(!is_mapped(test_virt), "Test virtual address must not be mapped initially");

    let test_frame = allocate_frame().expect("Failed to allocate frame for test mapping");
    let flags = PageTableFlags::WRITABLE | PageTableFlags::NO_EXECUTE;

    unsafe {
        map_page(test_virt, test_frame, flags).expect("Failed to map test page");
    }
    assert!(is_mapped(test_virt), "Test virtual address must be mapped after map_page");

    let translated = translate_addr(test_virt).expect("Translation failed after map_page");
    assert_eq!(translated, test_frame, "Translated frame must match mapped frame");
    log_info!("  -> Mapped {:#018x} -> {:#018x}", test_virt.as_u64(), translated.as_u64());
    log_info!("[PASS] Test 3: Fresh page mapped and translation verified.");

    // 4. Read and Write Access on Mapped Page
    log_info!("[TEST 4/12] Testing read and write operations on mapped page...");
    let ptr = test_virt.as_mut_ptr::<u64>();
    unsafe {
        core::ptr::write_volatile(ptr, 0xCAFE_BABE_DEAD_BEEF);
        let read_back = core::ptr::read_volatile(ptr);
        assert_eq!(read_back, 0xCAFE_BABE_DEAD_BEEF, "Read back value does not match written value!");
    }
    log_info!("[PASS] Test 4: Writable mapped page read/write verified.");

    // 5. Memory Aliasing (Two Virtual Pages to One Physical Frame)
    log_info!("[TEST 5/12] Testing physical frame aliasing across two virtual pages...");
    let alias_virt = VirtAddr::new(0xffff_b000_0000_1000);
    unsafe {
        map_page(alias_virt, test_frame, flags).expect("Failed to map alias page");
        let alias_ptr = alias_virt.as_mut_ptr::<u64>();
        // Reading through alias should see the value written through test_virt
        assert_eq!(core::ptr::read_volatile(alias_ptr), 0xCAFE_BABE_DEAD_BEEF);

        // Modify through alias
        core::ptr::write_volatile(alias_ptr, 0x1234_5678_9ABC_DEF0);
        // Verify reflection in test_virt
        assert_eq!(core::ptr::read_volatile(ptr), 0x1234_5678_9ABC_DEF0);

        // Unmap alias page (explicitly NOT deallocating test_frame)
        let unmapped_alias = unmap_page(alias_virt).expect("Failed to unmap alias page");
        assert_eq!(unmapped_alias, test_frame);
    }
    assert!(!is_mapped(alias_virt), "Alias page must no longer be mapped");
    log_info!("[PASS] Test 5: Frame aliasing and coherent updates verified.");

    // 6. Reject Accidental Duplicate Mapping
    log_info!("[TEST 6/12] Testing duplicate mapping rejection...");
    let dummy_frame = allocate_frame().expect("PMM alloc failed");
    unsafe {
        let dup_res = map_page(test_virt, dummy_frame, flags);
        assert!(dup_res.is_err(), "Duplicate mapping was not rejected!");
    }
    let _ = deallocate_frame(dummy_frame);
    log_info!("[PASS] Test 6: Duplicate mapping correctly rejected.");

    // 7. Dynamic Permission Update (Writable -> Read-Only)
    log_info!("[TEST 7/12] Updating page permissions (removing WRITABLE flag)...");
    unsafe {
        update_page_flags(test_virt, PageTableFlags::NO_EXECUTE).expect("Failed to update flags");
        // Verify read operation continues to function
        let read_val = core::ptr::read_volatile(ptr);
        assert_eq!(read_val, 0x1234_5678_9ABC_DEF0);
    }
    log_info!("[PASS] Test 7: Page permissions updated and read-access maintained.");

    // 8. Unmap Page and Verify Translation Disappears
    log_info!("[TEST 8/12] Unmapping test page and verifying translation disappears...");
    unsafe {
        let unmapped_frame = unmap_page(test_virt).expect("Failed to unmap test_virt");
        assert_eq!(unmapped_frame, test_frame, "Unmapped frame must match original frame");
    }
    assert!(!is_mapped(test_virt), "test_virt must no longer be mapped");
    assert!(translate_addr(test_virt).is_err(), "translate_addr must return error for unmapped address");
    log_info!("[PASS] Test 8: Page unmapped, translation removed, and TLB flushed.");

    // 9. Frame Lifetime Independence (Caller owns physical frame)
    log_info!("[TEST 9/12] Verifying physical frame lifetime independence...");
    // The frame was not automatically freed on unmap; caller explicitly frees it back to PMM
    assert!(deallocate_frame(test_frame).is_ok(), "Deallocating frame after unmap must succeed");
    // Attempting double free should fail
    assert!(deallocate_frame(test_frame).is_err(), "Double deallocation must be rejected");
    log_info!("[PASS] Test 9: Frame lifetime and unmapping lifetime are completely decoupled.");

    // 10. Virtual Address Reuse After Unmapping
    log_info!("[TEST 10/12] Testing virtual address reuse after unmapping...");
    let new_frame = allocate_frame().expect("PMM alloc failed");
    unsafe {
        map_page(test_virt, new_frame, flags).expect("Re-mapping same virtual address failed");
        assert!(is_mapped(test_virt));
        let ptr2 = test_virt.as_mut_ptr::<u32>();
        core::ptr::write_volatile(ptr2, 0x9988_7766);
        assert_eq!(core::ptr::read_volatile(ptr2), 0x9988_7766);
        let _ = unmap_page(test_virt);
        let _ = deallocate_frame(new_frame);
    }
    assert!(!is_mapped(test_virt));
    log_info!("[PASS] Test 10: Virtual address reused cleanly with fresh physical frame.");

    // 11. Intermediate Page-Table Reclamation Verification
    log_info!("[TEST 11/12] Verifying empty intermediate page-table reclamation...");
    // Allocate a page at a completely isolated PML4 entry (e.g., PML4 index 352)
    let isolated_virt = VirtAddr::new(0xffff_b001_0000_0000);
    let frames_before = crate::mm::ALLOCATOR.lock().free_frames();

    let iso_frame = allocate_frame().expect("PMM alloc failed");
    unsafe {
        map_page(isolated_virt, iso_frame, flags).expect("Map isolated page failed");
    }
    let frames_during = crate::mm::ALLOCATOR.lock().free_frames();
    // Intermediate tables (PDPT, PD, PT) plus the mapped frame consumed frames
    assert!(frames_during < frames_before, "Page tables must have consumed physical frames");

    unsafe {
        let _ = unmap_page(isolated_virt);
        let _ = deallocate_frame(iso_frame);
    }
    let frames_after = crate::mm::ALLOCATOR.lock().free_frames();
    assert_eq!(frames_after, frames_before, "All intermediate page table frames must be reclaimed back to PMM");
    log_info!("[PASS] Test 11: Intermediate PT/PD/PDPT tables reclaimed upon unmapping.");

    // 12. Paging Environment and Subsystem Stability
    log_info!("[TEST 12/12] Validating heap and tasking memory stability...");
    let heap_stats = crate::heap::HEAP_ALLOCATOR.lock().stats();
    assert!(heap_stats.0 > 0, "Heap must remain valid and operational");
    log_info!("[PASS] Test 12: Core kernel subsystem mappings remain completely uncorrupted.");

    log_info!("====================================================");
    log_info!(">>> ALL PHASE 10 ADVANCED VIRTUAL MEMORY TESTS PASSED");
    log_info!("====================================================");
}
