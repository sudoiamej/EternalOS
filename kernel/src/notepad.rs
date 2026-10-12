//! Graphical Notepad Application for EternalOS
//!
//! Features:
//! - Dedicated floating window surface on linear framebuffer
//! - Window decorations: purple border, header bar, close icon, status bar
//! - Text editing buffer with keyboard handling
//! - Full viewport restoration on exit

use crate::console::{console_read_input, redraw_terminal_viewport, ConsoleKey};
use crate::framebuffer::{with_framebuffer, Color, Rect};
use crate::vfs::{with_vfs, OpenFlags, VfsError};

const MAX_NOTEPAD_BYTES: usize = 4096;

pub struct NotepadApp {
    buffer: [u8; MAX_NOTEPAD_BYTES],
    buf_len: usize,
    cursor: usize,
    dirty: bool,
    path: [u8; 128],
    path_len: usize,
    status: [u8; 64],
    status_len: usize,
}

impl NotepadApp {
    pub fn new(initial_path: &str) -> Self {
        let mut path = [0u8; 128];
        let p_bytes = initial_path.as_bytes();
        let path_len = p_bytes.len().min(128);
        path[..path_len].copy_from_slice(&p_bytes[..path_len]);

        Self {
            buffer: [0u8; MAX_NOTEPAD_BYTES],
            buf_len: 0,
            cursor: 0,
            dirty: false,
            path,
            path_len,
            status: [0u8; 64],
            status_len: 0,
        }
    }

    pub fn set_status(&mut self, msg: &str) {
        let bytes = msg.as_bytes();
        self.status_len = bytes.len().min(64);
        self.status[..self.status_len].copy_from_slice(&bytes[..self.status_len]);
    }

    pub fn load_file(&mut self) {
        if self.path_len == 0 {
            return;
        }
        let path_str = core::str::from_utf8(&self.path[..self.path_len]).unwrap_or("");
        let res: Result<usize, VfsError> = with_vfs(|vfs| {
            let handle = vfs.open(path_str, OpenFlags::READ_ONLY)?;
            let mut read_buf = [0u8; 1024];
            let mut total_read = 0;
            loop {
                let n = vfs.read(handle, &mut read_buf)?;
                if n == 0 {
                    break;
                }
                let available = MAX_NOTEPAD_BYTES - total_read;
                let chunk = n.min(available);
                self.buffer[total_read..total_read + chunk].copy_from_slice(&read_buf[..chunk]);
                total_read += chunk;
                if total_read >= MAX_NOTEPAD_BYTES {
                    break;
                }
            }
            let _ = vfs.close(handle);
            Ok(total_read)
        });

        match res {
            Ok(len) => {
                self.buf_len = len;
                self.dirty = false;
                self.set_status("File loaded");
            }
            Err(_) => {
                self.buf_len = 0;
                self.dirty = false;
                self.set_status("New Document");
            }
        }
    }

    pub fn save_file(&mut self) -> bool {
        if self.path_len == 0 {
            self.set_status("Error: No file specified");
            return false;
        }
        let path_str = core::str::from_utf8(&self.path[..self.path_len]).unwrap_or("");
        let res: Result<(), VfsError> = with_vfs(|vfs| {
            let handle = vfs.open(path_str, OpenFlags::CREATE_OR_WRITE)?;
            let _ = vfs.write(handle, &self.buffer[..self.buf_len])?;
            vfs.close(handle)?;
            Ok(())
        });

        match res {
            Ok(()) => {
                self.dirty = false;
                self.set_status("File saved successfully");
                true
            }
            Err(e) => {
                if e == VfsError::PermissionDenied {
                    self.set_status("Error: Read-only storage");
                } else {
                    self.set_status("Error: Save failed");
                }
                false
            }
        }
    }

    fn insert_char(&mut self, ch: u8) {
        if self.buf_len >= MAX_NOTEPAD_BYTES - 1 {
            return;
        }
        for i in (self.cursor..self.buf_len).rev() {
            self.buffer[i + 1] = self.buffer[i];
        }
        self.buffer[self.cursor] = ch;
        self.buf_len += 1;
        self.cursor += 1;
        self.dirty = true;
    }

    fn delete_char_backspace(&mut self) {
        if self.cursor == 0 {
            return;
        }
        for i in self.cursor - 1..self.buf_len - 1 {
            self.buffer[i] = self.buffer[i + 1];
        }
        self.buf_len -= 1;
        self.cursor -= 1;
        self.dirty = true;
    }

    pub fn render(&self) {
        with_framebuffer(|fb| {
            if fb.width < 640 || fb.height < 480 {
                return;
            }

            // Window Dimensions (Center floating window)
            let win_w = 600.min(fb.width - 40);
            let win_h = 420.min(fb.height - 40);
            let win_x = (fb.width - win_w) / 2;
            let win_y = (fb.height - win_h) / 2;

            // Background & Border
            fb.fill_rect(Rect::new(win_x, win_y, win_w, win_h), Color::new(0x16, 0x16, 0x22));
            fb.draw_rect_border(Rect::new(win_x, win_y, win_w, win_h), Color::ACCENT_LAVENDER);

            // Title Bar
            let title_h = 28;
            fb.fill_rect(Rect::new(win_x + 2, win_y + 2, win_w - 4, title_h), Color::ACCENT_PURPLE);
            fb.draw_text(win_x + 12, win_y + 8, "EternalOS Notepad", Color::WHITE, None);

            // Path & Dirty Flag
            if self.path_len > 0 {
                let p_str = core::str::from_utf8(&self.path[..self.path_len]).unwrap_or("");
                fb.draw_text(win_x + 180, win_y + 8, p_str, Color::ACCENT_LAVENDER, None);
            }
            if self.dirty {
                fb.draw_text(win_x + win_w - 90, win_y + 8, "[* Unsaved]", Color::WARN_AMBER, None);
            }

            // Text Content Area
            let text_x = win_x + 16;
            let text_y = win_y + title_h + 12;
            let text_w = win_w - 32;
            let text_h = win_h - title_h - 48;

            fb.fill_rect(Rect::new(text_x, text_y, text_w, text_h), Color::new(0x0e, 0x0e, 0x18));
            fb.draw_rect_border(Rect::new(text_x, text_y, text_w, text_h), Color::new(0x28, 0x28, 0x42));

            // Render text lines
            let mut cur_y = text_y + 8;
            let mut cur_x = text_x + 8;
            for &b in &self.buffer[..self.buf_len] {
                if cur_y > text_y + text_h - 18 {
                    break;
                }
                if b == b'\n' {
                    cur_y += 16;
                    cur_x = text_x + 8;
                    continue;
                }
                if cur_x + 8 > text_x + text_w - 8 {
                    cur_y += 16;
                    cur_x = text_x + 8;
                }
                fb.draw_char(cur_x, cur_y, b as char, Color::WHITE, None);
                cur_x += 8;
            }

            // Blinking / solid cursor indication
            fb.fill_rect(Rect::new(cur_x, cur_y, 8, 14), Color::ACCENT_CYAN);

            // Bottom Status Bar
            let status_y = win_y + win_h - 24;
            fb.fill_rect(Rect::new(win_x + 2, status_y, win_w - 4, 22), Color::new(0x1a, 0x1a, 0x2c));
            fb.draw_text(win_x + 12, status_y + 4, "Ctrl+S Save | Esc / Ctrl+Q Exit", Color::TEXT_MUTED, None);

            if self.status_len > 0 {
                let s_str = core::str::from_utf8(&self.status[..self.status_len]).unwrap_or("");
                fb.draw_text(win_x + 320, status_y + 4, s_str, Color::SUCCESS_GREEN, None);
            }
        });
    }

    pub fn run(&mut self) {
        self.load_file();

        let mut needs_redraw = true;

        loop {
            if needs_redraw {
                self.render();
                needs_redraw = false;
            }

            let key = match console_read_input() {
                Some(k) => k,
                None => {
                    core::hint::spin_loop();
                    continue;
                }
            };

            needs_redraw = true;

            match key {
                ConsoleKey::Ctrl(b'q') | ConsoleKey::Escape => {
                    break;
                }
                ConsoleKey::Ctrl(b's') => {
                    self.save_file();
                }
                ConsoleKey::Left => {
                    self.cursor = self.cursor.saturating_sub(1);
                }
                ConsoleKey::Right => {
                    if self.cursor < self.buf_len {
                        self.cursor += 1;
                    }
                }
                ConsoleKey::Char(b'\r') | ConsoleKey::Char(b'\n') => {
                    self.insert_char(b'\n');
                }
                ConsoleKey::Char(0x08) | ConsoleKey::Char(0x7F) => {
                    self.delete_char_backspace();
                }
                ConsoleKey::Char(ch) => {
                    self.insert_char(ch);
                }
                _ => {
                    // Unknown key, avoid unnecessary redraw
                    needs_redraw = false;
                }
            }
        }

        // Clean restoration of terminal viewport upon closing application
        redraw_terminal_viewport();
    }
}
