use crate::serial::SerialPort;
use crate::framebuffer::{with_framebuffer, Color, Rect};
use crate::sync::SpinLock;
use crate::input::{poll_event, pop_char, InputEvent, KeyCode};

pub const TERM_ROWS: usize = 24;
pub const TERM_COLS: usize = 120;
pub const SCROLLBACK_CAPACITY: usize = 128;

/// Unified console key event
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConsoleKey {
    Char(u8),
    Up,
    Down,
    PageUp,
    PageDown,
    Left,
    Right,
    Home,
    End,
}

/// A single row of characters in the terminal scrollback buffer
#[derive(Clone, Copy)]
pub struct TerminalLine {
    pub chars: [u8; TERM_COLS],
    pub len: usize,
}

impl TerminalLine {
    pub const fn new() -> Self {
        Self {
            chars: [0; TERM_COLS],
            len: 0,
        }
    }
}

/// Bounded scrollback buffer storing lines of text
pub struct ScrollbackBuffer {
    pub lines: [TerminalLine; SCROLLBACK_CAPACITY],
    pub head: usize,        // Index of oldest line
    pub count: usize,       // Number of valid lines retained
    pub current_col: usize, // Active cursor column in current line
    pub scroll_offset: usize, // 0 = live view at bottom, >0 = scrolled up by N lines
}

impl ScrollbackBuffer {
    pub const fn new() -> Self {
        Self {
            lines: [const { TerminalLine::new() }; SCROLLBACK_CAPACITY],
            head: 0,
            count: 0,
            current_col: 0,
            scroll_offset: 0,
        }
    }

    /// Add a character to the active line, wrapping if necessary
    pub fn push_char(&mut self, b: u8) {
        if b == b'\r' {
            self.current_col = 0;
            return;
        }

        if b == b'\n' {
            self.newline();
            return;
        }

        if b == 0x08 {
            // Backspace
            if self.current_col > 0 {
                self.current_col -= 1;
                let active_idx = self.active_line_index();
                if self.current_col < self.lines[active_idx].len {
                    self.lines[active_idx].chars[self.current_col] = b' ';
                    if self.current_col + 1 == self.lines[active_idx].len {
                        self.lines[active_idx].len = self.current_col;
                    }
                }
            }
            return;
        }

        if b < 0x20 || b > 0x7E {
            return;
        }

        if self.count == 0 {
            self.count = 1;
        }

        if self.current_col >= TERM_COLS {
            self.newline();
        }

        let active_idx = self.active_line_index();
        self.lines[active_idx].chars[self.current_col] = b;
        self.current_col += 1;
        if self.current_col > self.lines[active_idx].len {
            self.lines[active_idx].len = self.current_col;
        }
    }

    /// Advance to next line in ring buffer
    pub fn newline(&mut self) {
        if self.count < SCROLLBACK_CAPACITY {
            self.count += 1;
        } else {
            // Drop oldest line
            self.head = (self.head + 1) % SCROLLBACK_CAPACITY;
        }
        let active_idx = self.active_line_index();
        self.lines[active_idx] = TerminalLine::new();
        self.current_col = 0;
    }

    /// Physical index of the current active bottom line
    pub fn active_line_index(&self) -> usize {
        if self.count == 0 {
            0
        } else {
            (self.head + self.count - 1) % SCROLLBACK_CAPACITY
        }
    }

    /// Get line relative to bottom (0 = active bottom line, 1 = 1 line above bottom, etc.)
    pub fn get_line_from_bottom(&self, from_bottom: usize) -> Option<&TerminalLine> {
        if from_bottom >= self.count {
            None
        } else {
            let offset_from_head = self.count - 1 - from_bottom;
            let idx = (self.head + offset_from_head) % SCROLLBACK_CAPACITY;
            Some(&self.lines[idx])
        }
    }

    /// Scroll up by count lines
    pub fn scroll_up(&mut self, amount: usize) {
        let max_scroll = self.count.saturating_sub(TERM_ROWS);
        self.scroll_offset = (self.scroll_offset + amount).min(max_scroll);
    }

    /// Scroll down by count lines
    pub fn scroll_down(&mut self, amount: usize) {
        self.scroll_offset = self.scroll_offset.saturating_sub(amount);
    }

    /// Reset to bottom live view
    pub fn reset_scroll(&mut self) {
        self.scroll_offset = 0;
    }

    pub fn is_scrolled(&self) -> bool {
        self.scroll_offset > 0
    }
}

/// Text console state tracking cursor row and column for framebuffer rendering.
pub struct ConsoleState {
    pub cursor_x: usize,
    pub cursor_y: usize,
    pub min_x: usize,
    pub min_y: usize,
    pub max_x: usize,
    pub max_y: usize,
    pub in_terminal_panel: bool,
    pub scrollback: ScrollbackBuffer,
}

impl ConsoleState {
    pub const fn new() -> Self {
        Self {
            cursor_x: 60,
            cursor_y: 298,
            min_x: 60,
            min_y: 298,
            max_x: 1200,
            max_y: 730,
            in_terminal_panel: false,
            scrollback: ScrollbackBuffer::new(),
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
        fb.draw_text(panel_x + 20, panel_y + 14, "INTERACTIVE TERMINAL CONSOLE", Color::ACCENT_LAVENDER, None);
        fb.draw_text(panel_x + 320, panel_y + 14, "|   Shell Session Active   |   COM1 + Framebuffer", Color::TEXT_MUTED, None);
        fb.draw_hline(panel_x + 20, panel_y + 32, panel_w - 40, Color::PANEL_BORDER);

        // Update footer status bar
        render_footer_bar(fb, 0);

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

fn render_footer_bar(fb: &crate::framebuffer::Framebuffer, scroll_offset: usize) {
    let footer_y = fb.height - 32;
    fb.fill_rect(Rect::new(0, footer_y, fb.width, 32), Color::PANEL_BG);
    fb.draw_hline(0, footer_y, fb.width, Color::PANEL_BORDER);

    if scroll_offset > 0 {
        fb.draw_text(24, footer_y + 8, "[SCROLLBACK ACTIVE] PgUp/PgDn to navigate | PgDn to live view", Color::WARN_AMBER, None);
    } else {
        fb.draw_text(24, footer_y + 8, "Status: Ready (Up/Down: History | PgUp/PgDn: Scrollback)", Color::SUCCESS_GREEN, None);
    }

    let fb_info = "TTY: /dev/console (Serial & VESA)";
    let info_x = fb.width.saturating_sub(300);
    fb.draw_text(info_x, footer_y + 8, fb_info, Color::TEXT_MUTED, None);
}

/// Clear console display on framebuffer and serial terminal
pub fn console_clear() {
    // 1. ANSI clear sequence for serial terminal
    console_write("\x1B[2J\x1B[H");

    // 2. Clear framebuffer workspace area
    with_framebuffer(|fb| {
        let mut con = CONSOLE.lock();
        con.scrollback = ScrollbackBuffer::new();

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

/// Redraw the visible terminal panel from the scrollback buffer
pub fn redraw_terminal_viewport() {
    with_framebuffer(|fb| {
        let con = CONSOLE.lock();
        redraw_viewport_locked(fb, &con);
    });
}

fn redraw_viewport_locked(fb: &crate::framebuffer::Framebuffer, con: &ConsoleState) {
    if !con.in_terminal_panel || fb.width < 640 || fb.height < 480 {
        return;
    }

    let panel_x = 40;
    let panel_y = 254;
    let panel_w = fb.width - 80;
    let content_y = panel_y + 36;
    let content_h = fb.height - content_y - 52;

    // Clear terminal content area
    fb.fill_rect(Rect::new(panel_x + 2, content_y, panel_w - 4, content_h), Color::PANEL_BG);

    // Update footer bar with scroll state
    render_footer_bar(fb, con.scrollback.scroll_offset);

    let offset = con.scrollback.scroll_offset;
    let rows_to_show = TERM_ROWS.min(con.scrollback.count);

    for row in 0..rows_to_show {
        let from_bottom = (rows_to_show - 1 - row) + offset;
        if let Some(line) = con.scrollback.get_line_from_bottom(from_bottom) {
            let draw_y = con.min_y + (row * 18);
            let mut draw_x = con.min_x;

            for &ch in &line.chars[..line.len] {
                if ch >= 0x20 && ch <= 0x7E {
                    fb.draw_char(draw_x, draw_y, ch as char, Color::TEXT_PRIMARY, Some(Color::PANEL_BG));
                    draw_x += 8;
                }
            }
        }
    }
}

/// Scroll terminal back (older lines)
pub fn terminal_page_up() {
    let mut con = CONSOLE.lock();
    con.scrollback.scroll_up(TERM_ROWS / 2);
    drop(con);
    redraw_terminal_viewport();
}

/// Scroll terminal forward (newer lines)
pub fn terminal_page_down() {
    let mut con = CONSOLE.lock();
    con.scrollback.scroll_down(TERM_ROWS / 2);
    drop(con);
    redraw_terminal_viewport();
}

/// Return terminal to live view
pub fn terminal_return_to_live() {
    let mut con = CONSOLE.lock();
    if con.scrollback.is_scrolled() {
        con.scrollback.reset_scroll();
        drop(con);
        redraw_terminal_viewport();
    }
}

/// Write byte to console (echoes to COM1 and renders to Framebuffer if available)
pub fn console_write_byte(b: u8) {
    // 1. Output to serial
    if b == b'\n' {
        SerialPort::write_byte(b'\r');
        SerialPort::write_byte(b);
    } else if b == 0x08 {
        SerialPort::write_byte(0x08);
        SerialPort::write_byte(b' ');
        SerialPort::write_byte(0x08);
    } else {
        SerialPort::write_byte(b);
    }

    // 2. Output to scrollback & framebuffer
    with_framebuffer(|fb| {
        let mut con = CONSOLE.lock();
        con.scrollback.push_char(b);

        // If user is currently scrolled up, don't clobber historical display with live characters
        if con.scrollback.is_scrolled() {
            return;
        }

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
                redraw_viewport_locked(fb, &con);
                con.cursor_y = con.min_y + ((TERM_ROWS - 1) * 18);
                con.cursor_x = con.min_x;
            }
        } else if b == b'\r' {
            con.cursor_x = con.min_x;
        } else if b == 0x08 {
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
                    redraw_viewport_locked(fb, &con);
                    con.cursor_y = con.min_y + ((TERM_ROWS - 1) * 18);
                    con.cursor_x = con.min_x;
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

/// Render or clear text cursor box at active cursor position
pub fn draw_text_cursor(visible: bool) {
    with_framebuffer(|fb| {
        let con = CONSOLE.lock();
        if con.scrollback.is_scrolled() {
            return;
        }
        let bg_color = if con.in_terminal_panel {
            Color::PANEL_BG
        } else {
            Color::DARK_BG
        };
        let color = if visible { Color::ACCENT_LAVENDER } else { bg_color };
        fb.fill_rect(Rect::new(con.cursor_x, con.cursor_y + 13, 8, 2), color);
    });
}

/// Read a single byte from console input (backward compatibility helper)
pub fn console_read_byte() -> Option<u8> {
    if let Some(key) = console_read_input() {
        match key {
            ConsoleKey::Char(c) => Some(c),
            _ => None,
        }
    } else {
        None
    }
}

/// Unified console input reading: checks keyboard events first, then UART COM1
pub fn console_read_input() -> Option<ConsoleKey> {
    // 1. Check structured input event queue from PS/2 keyboard IRQ1
    while let Some(ev) = poll_event() {
        if let InputEvent::KeyDown { key, ascii, .. } = ev {
            // If this event has an ASCII character, discard the corresponding mirrored character in CHAR_QUEUE
            if ascii.is_some() {
                let _ = pop_char();
            }

            match key {
                KeyCode::UpArrow => return Some(ConsoleKey::Up),
                KeyCode::DownArrow => return Some(ConsoleKey::Down),
                KeyCode::PageUp => return Some(ConsoleKey::PageUp),
                KeyCode::PageDown => return Some(ConsoleKey::PageDown),
                KeyCode::LeftArrow => return Some(ConsoleKey::Left),
                KeyCode::RightArrow => return Some(ConsoleKey::Right),
                KeyCode::Home => return Some(ConsoleKey::Home),
                KeyCode::End => return Some(ConsoleKey::End),
                _ => {
                    if let Some(ch) = ascii {
                        return Some(ConsoleKey::Char(ch));
                    }
                }
            }
        }
    }

    // 2. Check buffered characters (for any legacy pushes not via InputEvent)
    if let Some(b) = pop_char() {
        return Some(ConsoleKey::Char(b));
    }

    // 3. Check Serial Port COM1 (supports ANSI escape sequences)
    if let Some(b) = SerialPort::read_byte() {
        if b == 0x1B {
            // Check for ANSI escape sequences: \x1b[A, \x1b[B, \x1b[5~, etc.
            if let Some(b2) = SerialPort::read_byte() {
                if b2 == b'[' {
                    if let Some(b3) = SerialPort::read_byte() {
                        match b3 {
                            b'A' => return Some(ConsoleKey::Up),
                            b'B' => return Some(ConsoleKey::Down),
                            b'C' => return Some(ConsoleKey::Right),
                            b'D' => return Some(ConsoleKey::Left),
                            b'5' => {
                                let _ = SerialPort::read_byte(); // consume '~'
                                return Some(ConsoleKey::PageUp);
                            }
                            b'6' => {
                                let _ = SerialPort::read_byte(); // consume '~'
                                return Some(ConsoleKey::PageDown);
                            }
                            _ => return Some(ConsoleKey::Char(b3)),
                        }
                    }
                }
                return Some(ConsoleKey::Char(b2));
            }
            return Some(ConsoleKey::Char(0x1B));
        }
        return Some(ConsoleKey::Char(b));
    }

    None
}


