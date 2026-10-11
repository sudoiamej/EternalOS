use core::arch::asm;
use core::sync::atomic::{AtomicU64, Ordering};

pub const IRQ0_VECTOR: u8 = 32;

// 8259 PIC I/O Ports
const PIC1_COMMAND: u16 = 0x20;
const PIC1_DATA: u16 = 0x21;
const PIC2_COMMAND: u16 = 0xA0;
const PIC2_DATA: u16 = 0xA1;

const ICW1_INIT: u8 = 0x11;
const ICW4_8086: u8 = 0x01;
const PIC_EOI: u8 = 0x20;

// 8254 PIT I/O Ports & Constants
const PIT_CHANNEL0_DATA: u16 = 0x40;
const PIT_COMMAND: u16 = 0x43;
const PIT_BASE_FREQUENCY: u32 = 1193182; // 1.193182 MHz

pub static TIMER_TICKS: AtomicU64 = AtomicU64::new(0);

#[inline]
unsafe fn outb(port: u16, val: u8) {
    unsafe {
        asm!("out dx, al", in("dx") port, in("al") val, options(nomem, nostack, preserves_flags));
    }
}

#[inline]
unsafe fn inb(port: u16) -> u8 {
    let val: u8;
    unsafe {
        asm!("in al, dx", in("dx") port, out("al") val, options(nomem, nostack, preserves_flags));
    }
    val
}

#[inline]
unsafe fn io_wait() {
    unsafe {
        outb(0x80, 0);
    }
}

/// Remap the 8259 PIC master and slave vectors to 32..47 so they don't collide with CPU exceptions 0..31
pub fn remap_pic() {
    unsafe {
        let _mask1 = inb(PIC1_DATA);
        let _mask2 = inb(PIC2_DATA);

        outb(PIC1_COMMAND, ICW1_INIT);
        io_wait();
        outb(PIC2_COMMAND, ICW1_INIT);
        io_wait();

        outb(PIC1_DATA, 32); // Master PIC vector offset = 32
        io_wait();
        outb(PIC2_DATA, 40); // Slave PIC vector offset = 40
        io_wait();

        outb(PIC1_DATA, 4); // Tell Master PIC that there is a slave PIC at IRQ2
        io_wait();
        outb(PIC2_DATA, 2); // Tell Slave PIC its cascade identity
        io_wait();

        outb(PIC1_DATA, ICW4_8086);
        io_wait();
        outb(PIC2_DATA, ICW4_8086);
        io_wait();

        // Unmask IRQ0 (timer) on Master PIC (0xFE), mask all other IRQs (slave PIC = 0xFF)
        outb(PIC1_DATA, 0xFE);
        outb(PIC2_DATA, 0xFF);
    }
}

pub fn send_eoi() {
    unsafe {
        outb(PIC1_COMMAND, PIC_EOI);
    }
}

/// Configure the 8254 Programmable Interval Timer (Channel 0, Square Wave mode)
pub fn init_pit(hz: u32) {
    remap_pic();

    let divisor = (PIT_BASE_FREQUENCY / hz) as u16;

    unsafe {
        // Channel 0, Access Mode lobyte/hibyte, Mode 2 (rate generator), 16-bit binary
        outb(PIT_COMMAND, 0x34);
        io_wait();
        outb(PIT_CHANNEL0_DATA, (divisor & 0xFF) as u8);
        io_wait();
        outb(PIT_CHANNEL0_DATA, ((divisor >> 8) & 0xFF) as u8);
        io_wait();
    }

    crate::log_info!("[TIMER] 8254 PIT initialized at {} Hz (divisor={}). Remapped IRQ0 to vector {}.", hz, divisor, IRQ0_VECTOR);
}

pub fn get_ticks() -> u64 {
    TIMER_TICKS.load(Ordering::Relaxed)
}
