use crate::serial::SerialPort;
use crate::framebuffer::{with_framebuffer, Color, Rect};
use crate::sync::SpinLock;

/// Text console state tracking cursor row and column for framebuffer rendering.
pub struct ConsoleState {
    pub cursor_x: usize,
    pub cursor_y: usize,
    pub min_x: usize,
    pub min_y: usize,
    pub max_x: usize,
    pub max_y: usize,
    pub in_terminal_panel: bool,
}

impl ConsoleState {
    pub const fn new() -> Self {
        Self {
            cursor_x: 56,
            cursor_y: 292,
            min_x: 56,
            min_y: 292,
            max_x: 1200,
            max_y: 720,
            in_terminal_panel: false,
        }
    }
}

static CONSOLE: SpinLock<ConsoleState> = SpinLock::new(ConsoleState::new());

/// Initialize and render a dedicated terminal window panel on the framebuffer
pub fn init_terminal_viewport() {
    with_framebuffer(|fb| {
        let mut con = CONSOLE.lock();
        if fb.width < 640 || fb.height < 480 {
            con.min_x = 16;
            con.min_y = 60;
            con.max_x = fb.width.saturating_sub(16);
            con.max_y = fb.height.saturating_sub(40);
            con.cursor_x = con.min_x;
            con.cursor_y = con.min_y;
            con.in_terminal_panel = true;
            return;
        }

        let panel_x = 40;
        let panel_y = 254;
        let panel_w = fb.width - 80;
        let panel_h = fb.height - panel_y - 48;

        // Clear and draw terminal container window
        fb.fill_rect(Rect::new(panel_x, panel_y, panel_w, panel_h), Color::PANEL_BG);
        fb.draw_rect_border(Rect::new(panel_x, panel_y, panel_w, panel_h), Color::PANEL_BORDER);

        // Terminal Title Header
        fb.draw_text(panel_x + 20, panel_y + 14, "INTERACTIVE TERMINAL CONSOLE", Color::ACCENT_CYAN, None);
        fb.draw_text(panel_x + 320, panel_y + 14, "|   Shell Session Active   |   COM1 + Framebuffer", Color::TEXT_MUTED, None);
        fb.draw_hline(panel_x + 20, panel_y + 32, panel_w - 40, Color::PANEL_BORDER);

        // Update footer status bar
        let footer_y = fb.height - 32;
        fb.fill_rect(Rect::new(0, footer_y, fb.width, 32), Color::PANEL_BG);
        fb.draw_hline(0, footer_y, fb.width, Color::PANEL_BORDER);
        fb.draw_text(24, footer_y + 8, "Status: Interactive Shell Ready (Type 'help' for commands)", Color::SUCCESS_GREEN, None);
        let fb_info = "TTY: /dev/console (Serial & VESA)";
        let info_x = fb.width.saturating_sub(300);
        fb.draw_text(info_x, footer_y + 8, fb_info, Color::TEXT_MUTED, None);

        // Position text cursor cleanly inside terminal panel
        con.min_x = panel_x + 20;
        con.min_y = panel_y + 44;
        con.max_x = panel_x + panel_w - 20;
        con.max_y = panel_y + panel_h - 16;
        con.cursor_x = con.min_x;
        con.cursor_y = con.min_y;
        con.in_terminal_panel = true;
    });
}

/// Clear console display on framebuffer and serial terminal
pub fn console_clear() {
    // 1. ANSI clear sequence for serial terminal
    console_write("\x1B[2J\x1B[H");

    // 2. Clear framebuffer workspace area
    with_framebuffer(|fb| {
        let mut con = CONSOLE.lock();
        if con.in_terminal_panel && fb.width >= 640 && fb.height >= 480 {
            let panel_x = 40;
            let panel_y = 254;
            let panel_w = fb.width - 80;
            let content_y = panel_y + 36;
            let content_h = fb.height - content_y - 52;
            fb.fill_rect(Rect::new(panel_x + 2, content_y, panel_w - 4, content_h), Color::PANEL_BG);
            con.cursor_x = con.min_x;
            con.cursor_y = con.min_y;
        } else {
            let clear_rect = Rect::new(0, 50, fb.width, fb.height.saturating_sub(82));
            fb.fill_rect(clear_rect, Color::DARK_BG);
            con.cursor_x = con.min_x;
            con.cursor_y = con.min_y;
        }
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
            con.cursor_x = con.min_x;
        }

        let bg_color = if con.in_terminal_panel {
            Color::PANEL_BG
        } else {
            Color::DARK_BG
        };

        if b == b'\n' {
            con.cursor_x = con.min_x;
            con.cursor_y += 18;
            if con.cursor_y + 16 > con.max_y {
                // Scroll or clear viewport back to top of terminal panel
                if con.in_terminal_panel && fb.width >= 640 && fb.height >= 480 {
                    let panel_x = 40;
                    let panel_y = 254;
                    let panel_w = fb.width - 80;
                    let content_y = panel_y + 36;
                    let content_h = fb.height - content_y - 52;
                    fb.fill_rect(Rect::new(panel_x + 2, content_y, panel_w - 4, content_h), Color::PANEL_BG);
                } else {
                    let clear_rect = Rect::new(0, 50, fb.width, fb.height.saturating_sub(82));
                    fb.fill_rect(clear_rect, Color::DARK_BG);
                }
                con.cursor_y = con.min_y;
            }
        } else if b == b'\r' {
            con.cursor_x = con.min_x;
        } else if b == 0x08 {
            // Backspace
            if con.cursor_x >= con.min_x + 8 {
                con.cursor_x -= 8;
                fb.draw_char(con.cursor_x, con.cursor_y, ' ', Color::TEXT_PRIMARY, Some(bg_color));
            }
        } else if b >= 0x20 && b <= 0x7E {
            fb.draw_char(con.cursor_x, con.cursor_y, b as char, Color::TEXT_PRIMARY, Some(bg_color));
            con.cursor_x += 8;
            if con.cursor_x + 8 > con.max_x {
                con.cursor_x = con.min_x;
                con.cursor_y += 18;
                if con.cursor_y + 16 > con.max_y {
                    if con.in_terminal_panel && fb.width >= 640 && fb.height >= 480 {
                        let panel_x = 40;
                        let panel_y = 254;
                        let panel_w = fb.width - 80;
                        let content_y = panel_y + 36;
                        let content_h = fb.height - content_y - 52;
                        fb.fill_rect(Rect::new(panel_x + 2, content_y, panel_w - 4, content_h), Color::PANEL_BG);
                    } else {
                        let clear_rect = Rect::new(0, 50, fb.width, fb.height.saturating_sub(82));
                        fb.fill_rect(clear_rect, Color::DARK_BG);
                    }
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

