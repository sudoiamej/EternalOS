#![no_std]
#![no_main]
#![feature(abi_x86_interrupt)]

use core::arch::asm;
use limine::BaseRevision;
use limine::request::{FramebufferRequest, RequestsEndMarker, RequestsStartMarker};

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
pub mod task;
#[cfg(feature = "test-task")]
pub mod task_tests;
pub mod timer;
pub mod vm;

/// Set Limine base revision
#[used]
#[unsafe(link_section = ".requests")]
static BASE_REVISION: BaseRevision = BaseRevision::new();

#[used]
#[unsafe(link_section = ".requests")]
static FRAMEBUFFER_REQUEST: FramebufferRequest = FramebufferRequest::new();

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

    // 1. Initialize CPU Exceptions and Timer IRQ in IDT
    idt::init();

    // 2. Initialize Framebuffer Output if available
    if let Some(framebuffer_response) = FRAMEBUFFER_REQUEST.get_response() {
        log_info!("Framebuffer detected. Initializing visual status block...");
        if let Some(fb) = framebuffer_response.framebuffers().next() {
            let width = fb.width().min(320);
            let height = fb.height().min(80);
            for y in 0..height {
                for x in 0..width {
                    let pixel_offset = (y * fb.pitch()) + (x * 4);
                    unsafe {
                        fb.addr()
                            .add(pixel_offset as usize)
                            .cast::<u32>()
                            .write(0x001B4D3E); // Deep teal/blue
                    }
                }
            }
            log_info!("Display frame buffer initialized successfully.");
        }
    } else {
        log_warn!("No graphics framebuffer provided by bootloader.");
    }

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

    // 7. Initialize Kernel Tasking Subsystem
    task::init_tasking();

    // 8. Initialize Hardware Timer (8254 PIT at 100 Hz = 10ms quantum)
    timer::init_pit(100);

    // 9. Enable CPU Interrupts
    unsafe {
        asm!("sti", options(nomem, nostack, preserves_flags));
    }
    log_info!("Hardware interrupts enabled on Bootstrap Processor (STI).");

    // 10. Test Hooks
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

    #[cfg(feature = "test-div-zero")]
    {
        log_warn!(">>> TRIGGERING TEST: Divide-By-Zero Exception (#DE)...");
        let a = 42;
        let b = 0;
        let _c = a / b;
    }

    #[cfg(feature = "test-invalid-opcode")]
    {
        log_warn!(">>> TRIGGERING TEST: Invalid Opcode Exception (#UD)...");
        unsafe {
            asm!("ud2", options(nomem, nostack));
        }
    }

    #[cfg(feature = "test-panic")]
    {
        log_warn!(">>> TRIGGERING TEST: Deliberate Kernel Panic...");
        panic!("Intentional test panic triggered via test-panic flag");
    }

    log_info!("Kernel startup sequence complete. Entering safe CPU halt state.");
    println!("====================================================");

    hcf();
}

#[panic_handler]
fn rust_panic(info: &core::panic::PanicInfo) -> ! {
    log_error!("================== KERNEL PANIC ==================");
    if let Some(location) = info.location() {
        log_error!("Location: {}:{}:{}", location.file(), location.line(), location.column());
    }
    log_error!("Message : {}", info.message());
    log_error!("System halted safely to prevent corrupted state.");
    log_error!("==================================================");
    hcf();
}

pub fn hcf() -> ! {
    loop {
        unsafe {
            asm!("hlt", options(nomem, nostack, preserves_flags));
        }
    }
}
