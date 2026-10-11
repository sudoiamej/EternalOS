use crate::serial::SerialPort;
use crate::framebuffer::{with_framebuffer, Color, Rect};
use crate::sync::SpinLock;

/// Text console state tracking cursor row and column for framebuffer rendering.
pub struct ConsoleState {
    pub cursor_x: usize,
    pub cursor_y: usize,
    pub min_y: usize,
    pub max_cols: usize,
    pub max_rows: usize,
}

impl ConsoleState {
    pub const fn new() -> Self {
        Self {
            cursor_x: 24,
            cursor_y: 60,
            min_y: 60,
            max_cols: 160,
            max_rows: 50,
        }
    }
}

static CONSOLE: SpinLock<ConsoleState> = SpinLock::new(ConsoleState::new());

/// Clear console display on framebuffer and serial terminal
pub fn console_clear() {
    // 1. ANSI clear sequence for serial terminal
    console_write("\x1B[2J\x1B[H");

    // 2. Clear framebuffer workspace area below top header bar (y >= 50)
    with_framebuffer(|fb| {
        let mut con = CONSOLE.lock();
        let clear_rect = Rect::new(0, 50, fb.width, fb.height.saturating_sub(82));
        fb.fill_rect(clear_rect, Color::DARK_BG);
        con.cursor_x = 24;
        con.cursor_y = 60;
        con.min_y = 60;
    });
}

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
        if con.cursor_y < con.min_y {
            con.cursor_y = con.min_y;
            con.cursor_x = 24;
        }

        if b == b'\n' {
            con.cursor_x = 24;
            con.cursor_y += 16;
            if con.cursor_y + 20 > fb.height.saturating_sub(32) {
                // Scroll or wrap back to top of workspace area
                let clear_rect = Rect::new(0, 50, fb.width, fb.height.saturating_sub(82));
                fb.fill_rect(clear_rect, Color::DARK_BG);
                con.cursor_y = con.min_y;
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
                if con.cursor_y + 20 > fb.height.saturating_sub(32) {
                    let clear_rect = Rect::new(0, 50, fb.width, fb.height.saturating_sub(82));
                    fb.fill_rect(clear_rect, Color::DARK_BG);
                    con.cursor_y = con.min_y;
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

