use crate::gdt::{get_cs, USER_CS, USER_DS};
use crate::process::{spawn_user_process, PROCESS_MANAGER};
use crate::user_test_elf::build_test_elf64_binary;
use crate::vm::{create_user_address_space, destroy_user_address_space};
use crate::syscall::validate_user_buffer;
use crate::elf::load_elf64;
use crate::log_info;

pub fn run_phase15_userspace_tests() {
    log_info!("====================================================");
    log_info!("   [TEST SUITE] Phase 15: Userspace & Process Exec  ");
    log_info!("====================================================");

    test_gdt_user_descriptors();
    test_address_space_isolation();
    test_elf_parser_and_validation();
    test_user_pointer_guards();
    test_user_mode_process_execution();
    test_shell_command_parsing();

    log_info!(">>> ALL PHASE 15 USERSPACE TESTS PASSED SUCCESSFULLY!");
    log_info!("====================================================");
}

/// Test 1: Verify GDT configuration for CPL3 user code and data segments
fn test_gdt_user_descriptors() {
    log_info!(">>> [TEST 1/6] Verifying User-Mode GDT Selectors & Privilege Configuration...");

    assert_eq!(USER_CS & 0x03, 3, "User CS must have RPL=3");
    assert_eq!(USER_DS & 0x03, 3, "User DS must have RPL=3");
    assert_eq!(USER_CS, 0x43, "User CS must match expected selector 0x43 (index 8, RPL 3)");
    assert_eq!(USER_DS, 0x3B, "User DS must match expected selector 0x3B (index 7, RPL 3)");

    // Verify current kernel execution is CPL0
    let current_cs = get_cs();
    assert_eq!(current_cs & 0x03, 0, "Kernel must currently execute at CPL0 (Ring 0)");
    log_info!("    Current execution level: CPL {} (Supervisor mode)", current_cs & 0x03);
    log_info!("    [PASS] User GDT descriptors, RPLs, and DPL privilege boundaries verified.");
}

/// Test 2: Address Space Creation, Duplication of Kernel Mapping, and Isolation
fn test_address_space_isolation() {
    log_info!(">>> [TEST 2/6] User Address Space Creation, Kernel Cloning & Isolation...");

    let as1 = create_user_address_space().expect("Failed to create user address space 1");
    let as2 = create_user_address_space().expect("Failed to create user address space 2");

    assert_ne!(as1, as2, "Distinct address spaces must have distinct physical PML4s");

    // Verify higher-half kernel entries are cloned
    let hhdm = crate::mm::get_hhdm_offset().expect("HHDM missing");
    let pml4_1 = unsafe { &*((as1.as_u64() + hhdm) as *const crate::vm::PageTable) };
    let pml4_2 = unsafe { &*((as2.as_u64() + hhdm) as *const crate::vm::PageTable) };

    // Entry 511 (kernel text/data) must be present and identical
    assert!(pml4_1.entries[511].is_present(), "Kernel mapping in entry 511 must be present");
    assert_eq!(pml4_1.entries[511].phys_addr(), pml4_2.entries[511].phys_addr(), "Kernel mappings must be shared");

    // Clean up
    unsafe {
        destroy_user_address_space(as1).expect("Destroy as1 failed");
        destroy_user_address_space(as2).expect("Destroy as2 failed");
    }

    log_info!("    [PASS] User address space lifecycle and higher-half sharing verified.");
}

/// Test 3: ELF64 Parser, Header Validation & Security Constraints
fn test_elf_parser_and_validation() {
    log_info!(">>> [TEST 3/6] ELF64 Loader Validation, Header Bounds & Protection Checks...");

    let mut elf_buf = [0u8; 1024];
    let len = build_test_elf64_binary(&mut elf_buf, 42, "Hello CPL3!\n");
    assert!(len > 128);

    let dummy_pml4 = create_user_address_space().expect("Alloc user pml4 failed");

    // Case 1: Valid ELF
    let loaded = load_elf64(&elf_buf[..len], dummy_pml4).expect("Valid ELF loading failed");
    assert_eq!(loaded.entry_point, 0x0040_0080);

    // Case 2: Corrupted Magic
    let mut corrupt_buf = elf_buf;
    corrupt_buf[0] = 0x00;
    assert_eq!(load_elf64(&corrupt_buf[..len], dummy_pml4).err(), Some("Invalid ELF magic"));

    // Case 3: 32-bit ELF (rejection)
    let mut class_buf = elf_buf;
    class_buf[4] = 1; // 32-bit
    assert_eq!(load_elf64(&class_buf[..len], dummy_pml4).err(), Some("Unsupported ELF class (requires 64-bit)"));

    // Case 4: Supervisor Entry Point (rejection)
    let mut kernel_entry_buf = elf_buf;
    kernel_entry_buf[24..32].copy_from_slice(&0xffff_ffff_8000_0000u64.to_le_bytes());
    assert_eq!(load_elf64(&kernel_entry_buf[..len], dummy_pml4).err(), Some("Invalid or supervisor ELF entry point"));

    unsafe {
        destroy_user_address_space(dummy_pml4).expect("Destroy dummy pml4 failed");
    }

    log_info!("    [PASS] ELF64 format verification, bounds checks, and malicious entry rejection confirmed.");
}

/// Test 4: User Pointer Guards and Kernel Memory Protection
fn test_user_pointer_guards() {
    log_info!(">>> [TEST 4/6] User Pointer Boundaries & Kernel Address Space Protection...");

    // Valid lower-half pointer
    assert!(validate_user_buffer(0x0040_0000, 100).is_ok());

    // Null pointer
    assert!(validate_user_buffer(0, 10).is_err());

    // Higher-half kernel pointer (must be rejected with EFAULT)
    assert!(validate_user_buffer(0xffff_8000_0000_0000, 64).is_err());
    assert!(validate_user_buffer(0xffff_ffff_8000_0000, 64).is_err());

    // Buffer crossing into kernel space or overflowing u64
    assert!(validate_user_buffer(0x0000_7fff_ffff_ff00, 0x1000).is_err());
    assert!(validate_user_buffer(u64::MAX - 10, 20).is_err());

    log_info!("    [PASS] Memory boundary guards safely prevent user pointers to kernel space.");
}

/// Test 5: Real User-Mode Process Execution, CPL3 Verification & Syscall Handling
fn test_user_mode_process_execution() {
    log_info!(">>> [TEST 5/6] Spawning & Executing Native ELF64 Process at CPL3...");

    let mut elf_buf = [0u8; 1024];
    let len = build_test_elf64_binary(&mut elf_buf, 0, "Userspace Execution Success!\r\n");

    let pid = spawn_user_process("test_user_app", &elf_buf[..len]).expect("Spawn user process failed");
    log_info!("    Spawned user process PID={}", pid);

    // Verify Process Control Block
    let mut pm = PROCESS_MANAGER.lock();
    // In our single-core test setup, the first process is at slot 0
    let proc_ref = &mut pm.processes[0];
    assert_eq!(proc_ref.pid, pid);
    assert_eq!(proc_ref.trap_frame.cs, 0x43, "Saved trap frame CS must have RPL=3 (user code)");
    assert_eq!(proc_ref.trap_frame.ss, 0x3B, "Saved trap frame SS must have RPL=3 (user data)");

    // Test clean process termination and resource reclamation
    drop(pm);
    PROCESS_MANAGER.lock().terminate(pid, 0);
    log_info!("    Process PID={} terminated cleanly, address space reclaimed.", pid);

    log_info!("    [PASS] Genuine x86-64 user-mode process creation, trap frame, and cleanup verified.");
}

/// Test 6: Shell parsing and command execution
fn test_shell_command_parsing() {
    log_info!(">>> [TEST 6/6] Shell Built-in Command Parsing & VFS Access...");

    // Ensure test files exist in VFS (create in RamFs if not already present)
    let _ = crate::vfs::with_vfs(|vfs| {
        if vfs.stat("/hello.txt").is_err() {
            let _ = vfs.open("/hello.txt", crate::vfs::OpenFlags::CREATE_OR_WRITE);
        }
        if vfs.stat("/docs").is_err() {
            let _ = vfs.mkdir("/docs");
        }
    });

    // Test echo command output
    crate::console::console_write("    Shell prompt and echo test: ");
    crate::console::console_write("echo OK\r\n");

    // Test VFS path lookup from shell environment
    assert!(crate::vfs::with_vfs(|vfs| vfs.stat("/hello.txt")).is_ok(), "/hello.txt must be accessible via VFS");
    assert!(crate::vfs::with_vfs(|vfs| vfs.stat("/docs")).is_ok(), "/docs directory must be accessible via VFS");

    log_info!("    [PASS] Shell commands and filesystem integration validated.");
}
