use crate::serial::SerialPort;
use crate::framebuffer::{with_framebuffer, Color};
use crate::sync::SpinLock;

/// Text console state tracking cursor row and column for framebuffer rendering.
pub struct ConsoleState {
    cursor_x: usize,
    cursor_y: usize,
    max_cols: usize,
    max_rows: usize,
}

impl ConsoleState {
    pub const fn new() -> Self {
        Self {
            cursor_x: 0,
            cursor_y: 0,
            max_cols: 160,
            max_rows: 50,
        }
    }
}

static CONSOLE: SpinLock<ConsoleState> = SpinLock::new(ConsoleState::new());

/// Write byte to console (echoes to COM1 and renders to Framebuffer if available)
pub fn console_write_byte(b: u8) {
    // 1. Output to serial
    if b == b'\n' {
        SerialPort::write_byte(b'\r');
    }
    SerialPort::write_byte(b);

    // 2. Output to framebuffer
    with_framebuffer(|fb| {
        let mut con = CONSOLE.lock();
        if con.cursor_y == 0 && con.cursor_x == 0 {
            // Initialize bottom console area if screen is wide enough
            con.max_cols = fb.width / 8;
            con.max_rows = fb.height / 16;
            // Let's place console starting at y = 260 if height >= 480
            if fb.height >= 480 {
                con.cursor_y = 260;
                con.cursor_x = 24;
            }
        }

        if b == b'\n' {
            con.cursor_x = 24;
            con.cursor_y += 16;
            if con.cursor_y + 16 > fb.height {
                con.cursor_y = if fb.height >= 480 { 260 } else { 0 };
            }
        } else if b == b'\r' {
            con.cursor_x = 24;
        } else if b == 0x08 {
            // Backspace
            if con.cursor_x >= 32 {
                con.cursor_x -= 8;
                fb.draw_char(con.cursor_x, con.cursor_y, ' ', Color::TEXT_PRIMARY, Some(Color::DARK_BG));
            }
        } else if b >= 0x20 && b <= 0x7E {
            fb.draw_char(con.cursor_x, con.cursor_y, b as char, Color::TEXT_PRIMARY, Some(Color::DARK_BG));
            con.cursor_x += 8;
            if con.cursor_x + 8 > fb.width - 24 {
                con.cursor_x = 24;
                con.cursor_y += 16;
                if con.cursor_y + 16 > fb.height {
                    con.cursor_y = if fb.height >= 480 { 260 } else { 0 };
                }
            }
        }
    });
}

/// Write a string slice to the system console.
pub fn console_write(s: &str) {
    for b in s.bytes() {
        console_write_byte(b);
    }
}

/// Read a single byte from console input (checks UART COM1, blocking or non-blocking)
pub fn console_read_byte() -> Option<u8> {
    SerialPort::read_byte()
}
