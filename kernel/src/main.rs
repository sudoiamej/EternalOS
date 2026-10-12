#![no_std]
#![no_main]
#![feature(abi_x86_interrupt)]

use core::arch::asm;
use limine::BaseRevision;
use limine::request::{RequestsEndMarker, RequestsStartMarker};

pub mod cpu;
pub mod gdt;
pub mod heap;
#[cfg(feature = "test-heap")]
pub mod heap_tests;
pub mod idt;
pub mod mm;
#[cfg(feature = "test-memory")]
pub mod mm_tests;
#[cfg(feature = "test-preempt")]
pub mod preempt_tests;
pub mod serial;
pub mod sync;
#[cfg(feature = "test-sync")]
pub mod sync_tests;
pub mod task;
#[cfg(feature = "test-sleep")]
pub mod sleep_tests;
#[cfg(feature = "test-task")]
pub mod task_tests;
#[cfg(feature = "test-reliability")]
pub mod reliability_tests;
#[cfg(feature = "test-vmem")]
pub mod vmem_tests;
#[cfg(feature = "test-arch")]
pub mod arch_tests;
pub mod font;
pub mod framebuffer;
#[cfg(feature = "test-graphics")]
pub mod graphics_tests;
pub mod logo_asset;
pub mod pci;
#[cfg(feature = "test-pci")]
pub mod pci_tests;
pub mod block;
pub mod virtio_blk;
#[cfg(feature = "test-storage")]
pub mod storage_tests;
pub mod partition;
#[cfg(feature = "test-partitions")]
pub mod partition_tests;
pub mod vfs;
#[cfg(feature = "test-vfs")]
pub mod vfs_tests;
#[cfg(feature = "test-eternalfs")]
pub mod eternalfs_tests;
pub mod timer;
pub mod vm;
pub mod console;
pub mod syscall;
pub mod syscall_handler;
pub mod elf;
pub mod process;
pub mod shell_parser;
pub mod shell;
#[cfg(feature = "test-shell")]
pub mod shell_tests;
pub mod user_test_elf;
#[cfg(feature = "test-userspace")]
pub mod userspace_tests;
pub mod input;
pub mod keyboard;
pub mod mouse;
#[cfg(feature = "test-input")]
pub mod input_tests;
pub mod net;
#[cfg(feature = "test-net")]
pub mod net_tests;
pub mod usb;
#[cfg(feature = "test-usb")]
pub mod usb_tests;

#[used]
#[unsafe(link_section = ".requests")]
static BASE_REVISION: BaseRevision = BaseRevision::new();

#[used]
#[unsafe(link_section = ".requests_start_marker")]
static _START_MARKER: RequestsStartMarker = RequestsStartMarker::new();

#[used]
#[unsafe(link_section = ".requests_end_marker")]
static _END_MARKER: RequestsEndMarker = RequestsEndMarker::new();

#[unsafe(no_mangle)]
unsafe extern "C" fn kmain() -> ! {
    serial::SerialPort::init();

    println!();
    println!("====================================================");
    println!("       [Project EternalOS] - Phase 6 Kernel         ");
    println!("====================================================");
    log_info!("Architecture : x86-64 (AMD64 / Intel 64)");
    log_info!("Boot Protocol: Limine (Revision: Base)");
    log_info!("Kernel Subsys: IDT, PMM, Heap, Tasking & Preemption");

    if BASE_REVISION.is_supported() {
        log_info!("Limine Base Revision handshake: Supported");
    } else {
        log_warn!("Limine Base Revision handshake: Not Supported");
    }

    // 1. Initialize GDT, TSS, and IST Exception Stacks
    gdt::init();

    // 2. Discover and Log CPU Hardware Capabilities (CPUID)
    cpu::log_cpu_capabilities();

    // 3. Initialize CPU Exceptions and Timer IRQ in IDT (binds IST1 to #DF)
    idt::init();

    // 3. Inspect Physical Memory Map
    mm::parse_memory_map();

    // 4. Initialize Bitmap Physical Page-Frame Allocator (PMM)
    mm::init_frame_allocator();

    // 5. Inspect Active Virtual Memory / Paging Mode
    vm::inspect_paging_environment();

    // 6. Initialize Dynamic Kernel Heap Allocator
    if let Err(e) = heap::init_heap() {
        log_error!("Failed to initialize Kernel Heap: {}", e);
    }

    // 7. Initialize Framebuffer & Render Graphical Boot Screen
    framebuffer::init();

    // 8. Initialize Kernel Tasking Subsystem
    task::init_tasking();

    // 9. Discover PCI/PCIe Hardware & Enumerate Buses (Mechanism #1)
    pci::init();

    // 10. Initialize Block Storage (VirtIO Block Driver)
    virtio_blk::init();

    // 11. Initialize Virtual Filesystem (VFS) with root RamFs
    vfs::init();

    // 12. Initialize Hardware Timer (8254 PIT at 100 Hz = 10ms quantum)
    timer::init_pit(100);

    // 13. Initialize PS/2 Keyboard & Mouse Input Subsystems
    keyboard::init();
    mouse::init();

    // 14. Initialize Network Subsystem (Intel E1000 Gigabit Driver & Protocol Stack)
    net::init();

    // 15. Initialize USB Subsystem (UHCI Host Controller Discovery & Device Enumeration)
    usb::init();

    // 16. Enable CPU Interrupts
    unsafe {
        asm!("sti", options(nomem, nostack, preserves_flags));
    }
    log_info!("Hardware interrupts enabled on Bootstrap Processor (STI).");

    // 16. Test Hooks
    #[cfg(feature = "test-usb")]
    {
        usb_tests::run_usb_tests();
    }
    #[cfg(feature = "test-net")]
    {
        net_tests::run_net_tests();
    }
    #[cfg(feature = "test-shell")]
    {
        shell_tests::run_phase_shell_tests();
    }
    #[cfg(feature = "test-input")]
    {
        input_tests::run_input_tests();
    }
    #[cfg(feature = "test-userspace")]
    {
        userspace_tests::run_phase15_userspace_tests();
    }
    #[cfg(feature = "test-eternalfs")]
    {
        eternalfs_tests::run_phase14c_eternalfs_tests();
    }
    #[cfg(feature = "test-vfs")]
    {
        vfs_tests::run_phase14b_vfs_tests();
    }
    #[cfg(feature = "test-partitions")]
    {
        partition_tests::run_phase14a_partition_tests();
    }
    #[cfg(feature = "test-storage")]
    {
        storage_tests::run_phase13b_storage_tests();
    }
    #[cfg(feature = "test-graphics")]
    {
        graphics_tests::run_phase13a_graphics_tests();
    }
    #[cfg(feature = "test-pci")]
    {
        pci_tests::run_phase12_pci_tests();
    }
    #[cfg(feature = "test-sleep")]
    {
        sleep_tests::run_phase8_sleep_tests();
    }

    #[cfg(feature = "test-sync")]
    {
        sync_tests::run_phase7_sync_tests();
    }

    #[cfg(feature = "test-preempt")]
    {
        preempt_tests::run_phase6_preemption_tests();
    }

    #[cfg(feature = "test-task")]
    {
        task_tests::run_phase5_scheduler_tests();
    }

    #[cfg(feature = "test-heap")]
    {
        heap_tests::run_phase4_heap_tests();
    }

    #[cfg(feature = "test-memory")]
    {
        mm_tests::run_phase3_memory_tests();
    }

    #[cfg(feature = "test-reliability")]
    {
        reliability_tests::run_phase9_reliability_tests();
    }

    #[cfg(feature = "test-vmem")]
    {
        vmem_tests::run_phase10_vmem_tests();
    }

    #[cfg(feature = "test-arch")]
    {
        arch_tests::run_phase11_arch_tests();
    }

    #[cfg(feature = "test-vmem-fault")]
    {
        log_warn!(">>> TRIGGERING TEST: Write to Read-Only Page Fault (#PF)...");
        // Allocate and map a read-only page
        let ro_virt = vm::VirtAddr::new(0xffff_b000_0000_0000);
        let ro_frame = mm::allocate_frame().expect("PMM alloc failed");
        unsafe {
            // Map strictly as PRESENT without WRITABLE
            vm::map_page(ro_virt, ro_frame, vm::PageTableFlags::NO_EXECUTE).expect("Map RO failed");
            let ro_ptr = ro_virt.as_mut_ptr::<u32>();
            // Attempt write to trigger hardware #PF with Write=true
            core::ptr::write_volatile(ro_ptr, 0x1337);
        }
    }

    #[cfg(feature = "test-div-zero")]
    {
        log_warn!(">>> TRIGGERING TEST: Divide-By-Zero Exception (#DE)...");
        unsafe {
            let divisor = 0u64;
            asm!(
                "mov rdx, 0",
                "mov rax, 42",
                "div {0}",
                in(reg) divisor,
                out("rax") _,
                out("rdx") _,
                options(nomem, nostack)
            );
        }
    }

    #[cfg(feature = "test-invalid-opcode")]
    {
        log_warn!(">>> TRIGGERING TEST: Invalid Opcode Exception (#UD)...");
        unsafe {
            asm!("ud2", options(nomem, nostack));
        }
    }

    #[cfg(feature = "test-gpf")]
    {
        log_warn!(">>> TRIGGERING TEST: General Protection Fault (#GP)...");
        unsafe {
            // Loading an invalid segment selector (e.g. 0x0043 or out-of-limit selector) into DS triggers #GP with error code
            let bad_selector: u16 = 0x00ff;
            asm!("mov ds, {0:x}", in(reg) bad_selector, options(nomem, nostack));
        }
    }

    #[cfg(feature = "test-pagefault")]
    {
        log_warn!(">>> TRIGGERING TEST: Page Fault Exception (#PF)...");
        unsafe {
            // Access an unmapped virtual address in higher-half non-canonical or unmapped space
            let bad_ptr = 0xffff_dead_beef_0000 as *const u64;
            let _val = core::ptr::read_volatile(bad_ptr);
        }
    }

    #[cfg(feature = "test-panic")]
    {
        log_warn!(">>> TRIGGERING TEST: Deliberate Kernel Panic...");
        panic!("Intentional test panic triggered via test-panic flag");
    }

    log_info!("Kernel startup sequence complete.");
    println!("====================================================");

    // 15. Transition from boot diagnostics to interactive shell
    #[cfg(not(any(
        feature = "test-shell",
        feature = "test-userspace",
        feature = "test-eternalfs",
        feature = "test-vfs",
        feature = "test-partitions",
        feature = "test-storage",
        feature = "test-graphics",
        feature = "test-pci",
        feature = "test-sleep",
        feature = "test-sync",
        feature = "test-preempt",
        feature = "test-task",
        feature = "test-heap",
        feature = "test-memory",
        feature = "test-reliability",
        feature = "test-vmem",
        feature = "test-arch",
        feature = "test-vmem-fault",
        feature = "test-div-zero",
        feature = "test-invalid-opcode",
        feature = "test-gpf",
        feature = "test-pagefault",
        feature = "test-panic",
        feature = "test-input",
        feature = "test-net",
        feature = "test-usb"
    )))]
    {
        log_info!("Starting EternalOS Interactive Shell...");
        shell::run_interactive_shell();
    }

    log_info!("Session finished. Entering safe CPU halt state.");
    hcf();
}

static PANIC_RECURSION: core::sync::atomic::AtomicUsize = core::sync::atomic::AtomicUsize::new(0);

#[panic_handler]
fn rust_panic(info: &core::panic::PanicInfo) -> ! {
    // 1. Immediately disable all interrupts to freeze context and prevent timer preemption
    unsafe {
        asm!("cli", options(nomem, nostack, preserves_flags));
    }

    let recursion = PANIC_RECURSION.fetch_add(1, core::sync::atomic::Ordering::SeqCst);
    if recursion == 0 {
        // First-level panic: Attempt structured output
        log_fatal!("================== KERNEL PANIC ==================");
        if let Some(location) = info.location() {
            log_fatal!("Location : {}:{}:{}", location.file(), location.line(), location.column());
        }
        log_fatal!("Message  : {}", info.message());
        log_fatal!("Subsystem: Bootstrap Processor / Core Kernel");
        log_fatal!("Action   : Halting CPU safely to prevent memory corruption.");
        log_fatal!("==================================================");
    } else if recursion == 1 {
        // Recursive panic detected: Bypass formatting/locks and dump emergency alert
        serial::emergency_write_str("\r\n[FATAL] DOUBLE PANIC DETECTED! Aborting formatted diagnostics.\r\n");
    }

    hcf();
}

pub fn hcf() -> ! {
    loop {
        unsafe {
            asm!("hlt", options(nomem, nostack, preserves_flags));
        }
    }
}
