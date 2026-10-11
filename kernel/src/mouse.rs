//! PS/2 Mouse Driver (i8042 Auxiliary Device, IRQ12 -> Vector 44)
//!
//! Handles 3-byte standard PS/2 mouse packets, signed relative movement, button state (left/right/middle),
//! screen boundary coordinate clamping, and mouse pointer cursor rendering on linear framebuffer.

use core::arch::asm;
use crate::input::{post_event, POINTER, InputEvent, MouseButton};
use crate::sync::SpinLock;
use crate::timer::{send_eoi_irq, unmask_irq};
use crate::framebuffer::{with_framebuffer, Color};

const PS2_DATA_PORT: u16 = 0x60;
const PS2_STATUS_PORT: u16 = 0x64;
const PS2_COMMAND_PORT: u16 = 0x64;

#[inline]
unsafe fn inb(port: u16) -> u8 {
    let val: u8;
    unsafe {
        asm!("in al, dx", in("dx") port, out("al") val, options(nomem, nostack, preserves_flags));
    }
    val
}

#[inline]
unsafe fn outb(port: u16, val: u8) {
    unsafe {
        asm!("out dx, al", in("dx") port, in("al") val, options(nomem, nostack, preserves_flags));
    }
}

#[inline]
unsafe fn io_wait() {
    unsafe {
        outb(0x80, 0);
    }
}

fn wait_write() -> bool {
    for _ in 0..10_000 {
        if unsafe { inb(PS2_STATUS_PORT) & 0x02 } == 0 {
            return true;
        }
        core::hint::spin_loop();
    }
    false
}

fn wait_read() -> bool {
    for _ in 0..10_000 {
        if unsafe { inb(PS2_STATUS_PORT) & 0x01 } != 0 {
            return true;
        }
        core::hint::spin_loop();
    }
    false
}

fn mouse_write(byte: u8) -> bool {
    // Send 0xD4 command to controller to tell it next byte is for second PS/2 port
    if !wait_write() {
        return false;
    }
    unsafe {
        outb(PS2_COMMAND_PORT, 0xD4);
        io_wait();
    }
    if !wait_write() {
        return false;
    }
    unsafe {
        outb(PS2_DATA_PORT, byte);
        io_wait();
    }
    true
}

fn mouse_read() -> Option<u8> {
    if wait_read() {
        Some(unsafe { inb(PS2_DATA_PORT) })
    } else {
        None
    }
}

pub struct MouseDriver {
    pub cycle: u8,
    pub packet: [u8; 3],
    pub left_down: bool,
    pub right_down: bool,
    pub middle_down: bool,
    pub last_drawn_x: usize,
    pub last_drawn_y: usize,
    pub saved_pixels: [Color; 64], // 8x8 cursor background restoration buffer
    pub cursor_visible: bool,
}

impl MouseDriver {
    pub const fn new() -> Self {
        Self {
            cycle: 0,
            packet: [0; 3],
            left_down: false,
            right_down: false,
            middle_down: false,
            last_drawn_x: 640,
            last_drawn_y: 400,
            saved_pixels: [Color::DARK_BG; 64],
            cursor_visible: false,
        }
    }
}

pub static MOUSE_DRIVER: SpinLock<MouseDriver> = SpinLock::new(MouseDriver::new());

/// 8x8 Arrow Cursor Bitmap
/// '.' = Transparent, 'W' = White outline, 'B' = Cyan/Blue interior
const CURSOR_WIDTH: usize = 8;
const CURSOR_HEIGHT: usize = 8;
const CURSOR_BITMAP: [&str; 8] = [
    "W.......",
    "WW......",
    "WBW.....",
    "WBBW....",
    "WBBBW...",
    "WBBWW...",
    "WBW.....",
    "WW......",
];

/// Initialize PS/2 Mouse on IRQ12 (Vector 44)
pub fn init() {
    crate::log_info!("[MOUSE] Initializing PS/2 Mouse via i8042 auxiliary port...");

    // 1. Enable auxiliary device (command 0xA8)
    if wait_write() {
        unsafe {
            outb(PS2_COMMAND_PORT, 0xA8);
            io_wait();
        }
    }

    // 2. Read config byte and enable IRQ12 (bit 1) and disable clock inhibit (bit 5)
    if wait_write() {
        unsafe {
            outb(PS2_COMMAND_PORT, 0x20);
            io_wait();
        }
        if let Some(mut config) = mouse_read() {
            config |= 0x02; // Enable auxiliary interrupt IRQ12
            config &= !0x20; // Disable auxiliary clock disable bit

            if wait_write() {
                unsafe {
                    outb(PS2_COMMAND_PORT, 0x60);
                    io_wait();
                    outb(PS2_DATA_PORT, config);
                    io_wait();
                }
            }
        }
    }

    // 3. Set default settings (command 0xF6)
    mouse_write(0xF6);
    let _ = mouse_read(); // ACK (0xFA)

    // 4. Enable data reporting (command 0xF4)
    mouse_write(0xF4);
    let _ = mouse_read(); // ACK (0xFA)

    // 5. Unmask IRQ12 on Slave PIC (and cascade IRQ2)
    unmask_irq(12);

    // 6. Initialize pointer boundaries from Framebuffer dimensions
    with_framebuffer(|fb| {
        let mut ptr = POINTER.lock();
        ptr.set_bounds(fb.width, fb.height);
        ptr.x = fb.width / 2;
        ptr.y = fb.height / 2;
    });

    crate::log_info!("[MOUSE] PS/2 Mouse initialized, IRQ12 unmasked.");
}

/// Handle Mouse Interrupt (IRQ12 -> Vector 44)
pub fn handle_interrupt() {
    let status = unsafe { inb(PS2_STATUS_PORT) };
    if (status & 0x01) == 0 {
        send_eoi_irq(12);
        return;
    }

    // Verify byte came from auxiliary port (bit 5)
    let is_mouse_byte = (status & 0x20) != 0;
    let byte = unsafe { inb(PS2_DATA_PORT) };
    send_eoi_irq(12);

    if !is_mouse_byte {
        return;
    }

    let mut driver = MOUSE_DRIVER.lock();

    // Packet synchronization: byte 0 must have bit 3 set to 1
    if driver.cycle == 0 && (byte & 0x08) == 0 {
        // Out of sync discard
        return;
    }

    let cycle_idx = driver.cycle as usize;
    driver.packet[cycle_idx] = byte;
    driver.cycle += 1;

    if driver.cycle == 3 {
        driver.cycle = 0;
        let b0 = driver.packet[0];
        let b1 = driver.packet[1];
        let b2 = driver.packet[2];

        // Discard packet if overflow flags (bits 6 or 7) are set
        if (b0 & 0xC0) != 0 {
            return;
        }

        // Decode relative signed X movement (9-bit signed integer using sign bit 4)
        let mut rel_x = b1 as i32;
        if (b0 & 0x10) != 0 {
            rel_x |= !0xFF; // sign extend negative
        }

        // Decode relative signed Y movement (9-bit signed integer using sign bit 5)
        // PS/2 Y is inverted (positive is UP, screen coordinates positive is DOWN)
        let mut rel_y = b2 as i32;
        if (b0 & 0x20) != 0 {
            rel_y |= !0xFF;
        }
        rel_y = -rel_y;

        // Button states
        let left = (b0 & 0x01) != 0;
        let right = (b0 & 0x02) != 0;
        let middle = (b0 & 0x04) != 0;

        // Update global pointer coordinates
        let (abs_x, abs_y) = {
            let mut ptr = POINTER.lock();
            ptr.apply_relative(rel_x, rel_y);
            ptr.left_button = left;
            ptr.right_button = right;
            ptr.middle_button = middle;
            (ptr.x, ptr.y)
        };

        // Post pointer move event
        if rel_x != 0 || rel_y != 0 {
            post_event(InputEvent::PointerMove {
                rel_x,
                rel_y,
                abs_x,
                abs_y,
            });
        }

        // Post button events
        if left != driver.left_down {
            driver.left_down = left;
            let ev = if left {
                InputEvent::PointerButtonDown { button: MouseButton::Left, abs_x, abs_y }
            } else {
                InputEvent::PointerButtonUp { button: MouseButton::Left, abs_x, abs_y }
            };
            post_event(ev);
        }

        if right != driver.right_down {
            driver.right_down = right;
            let ev = if right {
                InputEvent::PointerButtonDown { button: MouseButton::Right, abs_x, abs_y }
            } else {
                InputEvent::PointerButtonUp { button: MouseButton::Right, abs_x, abs_y }
            };
            post_event(ev);
        }

        if middle != driver.middle_down {
            driver.middle_down = middle;
            let ev = if middle {
                InputEvent::PointerButtonDown { button: MouseButton::Middle, abs_x, abs_y }
            } else {
                InputEvent::PointerButtonUp { button: MouseButton::Middle, abs_x, abs_y }
            };
            post_event(ev);
        }

        // Redraw mouse cursor on framebuffer
        draw_cursor(&mut driver, abs_x, abs_y);
    }
}

/// Render 8x8 software cursor with background preservation
fn draw_cursor(driver: &mut MouseDriver, new_x: usize, new_y: usize) {
    with_framebuffer(|fb| {
        // 1. Restore background under previous cursor position if previously drawn
        if driver.cursor_visible {
            let old_x = driver.last_drawn_x;
            let old_y = driver.last_drawn_y;
            for r in 0..CURSOR_HEIGHT {
                for c in 0..CURSOR_WIDTH {
                    let pixel_color = driver.saved_pixels[r * CURSOR_WIDTH + c];
                    fb.put_pixel(old_x + c, old_y + r, pixel_color);
                }
            }
        }

        // 2. Save pixels under new cursor position
        for r in 0..CURSOR_HEIGHT {
            for c in 0..CURSOR_WIDTH {
                // If get_pixel is available or sample from framebuffer memory directly:
                let px = new_x + c;
                let py = new_y + r;
                let bg_color = fb.get_pixel(px, py).unwrap_or(Color::DARK_BG);
                driver.saved_pixels[r * CURSOR_WIDTH + c] = bg_color;
            }
        }

        // 3. Draw cursor bitmap at new location
        for r in 0..CURSOR_HEIGHT {
            let row_str = CURSOR_BITMAP[r].as_bytes();
            for c in 0..CURSOR_WIDTH {
                let sym = row_str[c];
                let px = new_x + c;
                let py = new_y + r;
                if sym == b'W' {
                    fb.put_pixel(px, py, Color::new(255, 255, 255));
                } else if sym == b'B' {
                    fb.put_pixel(px, py, Color::ACCENT_CYAN);
                }
            }
        }

        driver.last_drawn_x = new_x;
        driver.last_drawn_y = new_y;
        driver.cursor_visible = true;
    });
}
