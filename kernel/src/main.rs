#![no_std]
#![no_main]

use core::arch::asm;
use core::fmt::{self, Write};
use limine::BaseRevision;
use limine::request::{FramebufferRequest, RequestsEndMarker, RequestsStartMarker};

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

/// Minimal 16550 UART Serial Driver on COM1 (0x3F8)
struct SerialPort;

impl SerialPort {
    const COM1: u16 = 0x3F8;

    unsafe fn outb(port: u16, val: u8) {
        unsafe {
            asm!("out dx, al", in("dx") port, in("al") val, options(nomem, nostack, preserves_flags));
        }
    }

    unsafe fn inb(port: u16) -> u8 {
        let val: u8;
        unsafe {
            asm!("in al, dx", in("dx") port, out("al") val, options(nomem, nostack, preserves_flags));
        }
        val
    }

    pub fn init() {
        unsafe {
            Self::outb(Self::COM1 + 1, 0x00); // Disable all interrupts
            Self::outb(Self::COM1 + 3, 0x80); // Enable DLAB (set baud rate divisor)
            Self::outb(Self::COM1 + 0, 0x03); // Set divisor to 3 (38400 baud)
            Self::outb(Self::COM1 + 1, 0x00);
            Self::outb(Self::COM1 + 3, 0x03); // 8 bits, no parity, one stop bit
            Self::outb(Self::COM1 + 2, 0xC7); // Enable FIFO, clear them, with 14-byte threshold
            Self::outb(Self::COM1 + 4, 0x0B); // IRQs enabled, RTS/DSR set
        }
    }

    pub fn write_byte(byte: u8) {
        unsafe {
            while (Self::inb(Self::COM1 + 5) & 0x20) == 0 {
                // Wait for transmitter empty
            }
            Self::outb(Self::COM1, byte);
        }
    }
}

impl Write for SerialPort {
    fn write_str(&mut self, s: &str) -> fmt::Result {
        for b in s.bytes() {
            if b == b'\n' {
                SerialPort::write_byte(b'\r');
            }
            SerialPort::write_byte(b);
        }
        Ok(())
    }
}

#[unsafe(no_mangle)]
unsafe extern "C" fn kmain() -> ! {
    SerialPort::init();
    let mut serial = SerialPort;

    let _ = serial.write_str("\n");
    let _ = serial.write_str("====================================================\n");
    let _ = serial.write_str("       [Project EternalOS] - Phase 1 Kernel         \n");
    let _ = serial.write_str("====================================================\n");
    let _ = serial.write_str("[INFO] Architecture : x86-64 (AMD64 / Intel 64)\n");
    let _ = serial.write_str("[INFO] Boot Protocol: Limine (Revision: Base)\n");
    let _ = serial.write_str("[INFO] Kernel Mode  : Hybrid Architecture Foundation\n");

    if BASE_REVISION.is_supported() {
        let _ = serial.write_str("[OK]   Limine Base Revision handshake: Supported\n");
    } else {
        let _ = serial.write_str("[WARN] Limine Base Revision handshake: Not Supported\n");
    }

    if let Some(framebuffer_response) = FRAMEBUFFER_REQUEST.get_response() {
        let _ = serial.write_str("[OK]   Framebuffer detected! Initializing display block...\n");
        if let Some(fb) = framebuffer_response.framebuffers().next() {
            // Draw a distinctive blue status rectangle in the upper-left corner
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
            let _ = serial.write_str("[OK]   Display frame buffer initialized successfully.\n");
        }
    } else {
        let _ = serial.write_str("[WARN] No graphics framebuffer provided by bootloader.\n");
    }

    let _ = serial.write_str("[INIT] Initialization complete. Entering safe CPU halt state.\n");
    let _ = serial.write_str("====================================================\n");

    hcf();
}

#[panic_handler]
fn rust_panic(info: &core::panic::PanicInfo) -> ! {
    let mut serial = SerialPort;
    let _ = serial.write_str("[FATAL PANIC] ");
    if let Some(location) = info.location() {
        let _ = serial.write_str(location.file());
        let _ = serial.write_str(":");
    }
    let _ = serial.write_str("\n");
    hcf();
}

fn hcf() -> ! {
    loop {
        unsafe {
            asm!("hlt", options(nomem, nostack, preserves_flags));
        }
    }
}
