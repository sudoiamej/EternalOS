//! Native Terminal Text Editor for EternalOS
//!
//! Features:
//! - Hybrid model: Direct editing + Nano shortcut bar (`Ctrl+S` save, `Ctrl+Q` quit)
//!   and Vim mode toggle (`Esc` for Normal mode: `h`/`j`/`k`/`l`, `:w`, `:q`, `:wq`, `:q!`, `i` to insert)
//! - 8 KiB text buffer with safe row/col cursor bounds
//! - Terminal viewport preservation and restoration on exit

use crate::console::{console_read_input, console_write, redraw_terminal_viewport, ConsoleKey};
use crate::shell::format_u64;
use crate::vfs::{with_vfs, OpenFlags, VfsError};

const MAX_EDITOR_SIZE: usize = 8192;
const MAX_LINES: usize = 256;

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum EditorMode {
    Nano,       // Direct typing, shortcuts Ctrl+S, Ctrl+Q
    VimNormal,  // Vim normal mode: navigation, command line (:w, :q)
    VimInsert,  // Vim insert mode: typing into buffer
}

pub struct TextEditor {
    buffer: [u8; MAX_EDITOR_SIZE],
    buf_len: usize,
    cursor_pos: usize, // byte offset in buffer
    mode: EditorMode,
    dirty: bool,
    path: [u8; 128],
    path_len: usize,
    command_buf: [u8; 32],
    command_len: usize,
    status_msg: [u8; 64],
    status_len: usize,
    line_offsets: [usize; MAX_LINES],
    line_count: usize,
}

impl TextEditor {
    pub fn new(initial_path: &str, initial_vim: bool) -> Self {
        let mut path = [0u8; 128];
        let p_bytes = initial_path.as_bytes();
        let path_len = p_bytes.len().min(128);
        path[..path_len].copy_from_slice(&p_bytes[..path_len]);

        Self {
            buffer: [0u8; MAX_EDITOR_SIZE],
            buf_len: 0,
            cursor_pos: 0,
            mode: if initial_vim { EditorMode::VimNormal } else { EditorMode::Nano },
            dirty: false,
            path,
            path_len,
            command_buf: [0u8; 32],
            command_len: 0,
            status_msg: [0u8; 64],
            status_len: 0,
            line_offsets: [0; MAX_LINES],
            line_count: 1,
        }
    }

    pub fn set_status(&mut self, msg: &str) {
        let bytes = msg.as_bytes();
        self.status_len = bytes.len().min(64);
        self.status_msg[..self.status_len].copy_from_slice(&bytes[..self.status_len]);
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
                let available = MAX_EDITOR_SIZE - total_read;
                let chunk = n.min(available);
                self.buffer[total_read..total_read + chunk].copy_from_slice(&read_buf[..chunk]);
                total_read += chunk;
                if total_read >= MAX_EDITOR_SIZE {
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
                self.recompute_lines();
                self.set_status("Loaded file successfully");
            }
            Err(_) => {
                self.buf_len = 0;
                self.dirty = false;
                self.recompute_lines();
                self.set_status("New File");
            }
        }
    }

    pub fn save_file(&mut self) -> bool {
        if self.path_len == 0 {
            self.set_status("Error: No file path specified");
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
                self.set_status("Wrote to file successfully");
                true
            }
            Err(e) => {
                if e == crate::vfs::VfsError::PermissionDenied {
                    self.set_status("Error: Read-only storage");
                } else {
                    self.set_status("Error: Failed to write");
                }
                false
            }
        }
    }

    fn recompute_lines(&mut self) {
        self.line_count = 1;
        self.line_offsets[0] = 0;

        for i in 0..self.buf_len {
            if self.buffer[i] == b'\n' {
                if self.line_count < MAX_LINES {
                    self.line_offsets[self.line_count] = i + 1;
                    self.line_count += 1;
                }
            }
        }
    }

    fn current_line_and_col(&self) -> (usize, usize) {
        let mut line = 0;
        for i in 0..self.line_count {
            if self.line_offsets[i] <= self.cursor_pos {
                line = i;
            } else {
                break;
            }
        }
        let col = self.cursor_pos - self.line_offsets[line];
        (line, col)
    }

    fn insert_char(&mut self, ch: u8) {
        if self.buf_len >= MAX_EDITOR_SIZE - 1 {
            self.set_status("Buffer full!");
            return;
        }

        for i in (self.cursor_pos..self.buf_len).rev() {
            self.buffer[i + 1] = self.buffer[i];
        }
        self.buffer[self.cursor_pos] = ch;
        self.buf_len += 1;
        self.cursor_pos += 1;
        self.dirty = true;
        self.recompute_lines();
    }

    fn delete_char_backspace(&mut self) {
        if self.cursor_pos == 0 {
            return;
        }

        for i in self.cursor_pos - 1..self.buf_len - 1 {
            self.buffer[i] = self.buffer[i + 1];
        }
        self.buf_len -= 1;
        self.cursor_pos -= 1;
        self.dirty = true;
        self.recompute_lines();
    }

    pub fn render(&self) {
        // ANSI clear screen & home cursor
        console_write("\x1B[2J\x1B[H");

        // Top Header
        console_write("=== EternalOS Editor: ");
        if self.path_len > 0 {
            console_write(core::str::from_utf8(&self.path[..self.path_len]).unwrap_or(""));
        } else {
            console_write("[Untitled]");
        }
        if self.dirty {
            console_write(" [*]");
        }
        console_write(" ===\r\n");

        // Display rows (first 18 lines)
        let display_lines = self.line_count.min(18);
        for l in 0..display_lines {
            let start = self.line_offsets[l];
            let end = if l + 1 < self.line_count {
                self.line_offsets[l + 1].saturating_sub(1)
            } else {
                self.buf_len
            };

            let line_slice = &self.buffer[start..end];
            if let Ok(line_str) = core::str::from_utf8(line_slice) {
                console_write(line_str);
            }
            console_write("\r\n");
        }

        // Bottom Status & Mode bar
        console_write("--------------------------------------------------------------------------------\r\n");
        let (row, col) = self.current_line_and_col();
        match self.mode {
            EditorMode::Nano => {
                console_write("[Nano Mode] ^S Save | ^Q Exit | Esc Vim Mode | Ln ");
            }
            EditorMode::VimNormal => {
                console_write("[Vim Normal] i Insert | :w Save | :q Quit | Ln ");
            }
            EditorMode::VimInsert => {
                console_write("[Vim INSERT] Esc Normal Mode | Ln ");
            }
        }
        let mut n_b = [0u8; 32];
        console_write(format_u64((row + 1) as u64, &mut n_b));
        console_write(", Col ");
        console_write(format_u64((col + 1) as u64, &mut n_b));

        if self.command_len > 0 {
            console_write(" | Cmd: :");
            console_write(core::str::from_utf8(&self.command_buf[..self.command_len]).unwrap_or(""));
        } else if self.status_len > 0 {
            console_write(" | ");
            console_write(core::str::from_utf8(&self.status_msg[..self.status_len]).unwrap_or(""));
        }
        console_write("\r\n");
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
                ConsoleKey::Ctrl(b'q') => {
                    if self.dirty {
                        self.set_status("Unsaved changes! Use :q! or Ctrl+S first");
                    } else {
                        break;
                    }
                }
                ConsoleKey::Ctrl(b's') => {
                    self.save_file();
                }
                ConsoleKey::Escape => {
                    match self.mode {
                        EditorMode::Nano => self.mode = EditorMode::VimNormal,
                        EditorMode::VimInsert => self.mode = EditorMode::VimNormal,
                        EditorMode::VimNormal => {
                            self.command_len = 0;
                            self.mode = EditorMode::Nano;
                        }
                    }
                }
                ConsoleKey::Left => {
                    self.cursor_pos = self.cursor_pos.saturating_sub(1);
                }
                ConsoleKey::Right => {
                    if self.cursor_pos < self.buf_len {
                        self.cursor_pos += 1;
                    }
                }
                ConsoleKey::Up => {
                    let (cur_line, cur_col) = self.current_line_and_col();
                    if cur_line > 0 {
                        let prev_line_start = self.line_offsets[cur_line - 1];
                        let prev_line_len = self.line_offsets[cur_line] - prev_line_start - 1;
                        let target_col = cur_col.min(prev_line_len);
                        self.cursor_pos = prev_line_start + target_col;
                    }
                }
                ConsoleKey::Down => {
                    let (cur_line, cur_col) = self.current_line_and_col();
                    if cur_line + 1 < self.line_count {
                        let next_line_start = self.line_offsets[cur_line + 1];
                        let next_line_end = if cur_line + 2 < self.line_count {
                            self.line_offsets[cur_line + 2] - 1
                        } else {
                            self.buf_len
                        };
                        let next_line_len = next_line_end - next_line_start;
                        let target_col = cur_col.min(next_line_len);
                        self.cursor_pos = next_line_start + target_col;
                    }
                }
                ConsoleKey::Char(b'\r') | ConsoleKey::Char(b'\n') => {
                    if self.mode == EditorMode::VimNormal {
                        if self.command_len > 0 {
                            let mut tmp = [0u8; 32];
                            let c_len = self.command_len.min(32);
                            tmp[..c_len].copy_from_slice(&self.command_buf[..c_len]);
                            self.command_len = 0;
                            let cmd_str = core::str::from_utf8(&tmp[..c_len]).unwrap_or("");
                            let should_break = self.handle_vim_command(cmd_str);
                            if should_break {
                                break;
                            }
                        }
                    } else {
                        self.insert_char(b'\n');
                    }
                }
                ConsoleKey::Char(0x08) | ConsoleKey::Char(0x7F) => {
                    // Backspace
                    if self.mode == EditorMode::VimNormal {
                        if self.command_len > 0 {
                            self.command_len -= 1;
                        }
                    } else {
                        self.delete_char_backspace();
                    }
                }
                ConsoleKey::Char(ch) => {
                    if self.mode == EditorMode::VimNormal {
                        if self.command_len > 0 {
                            if self.command_len < 31 {
                                self.command_buf[self.command_len] = ch;
                                self.command_len += 1;
                            }
                        } else {
                            match ch {
                                b':' => {
                                    self.command_buf[0] = b':';
                                    self.command_len = 1;
                                }
                                b'i' => {
                                    self.mode = EditorMode::VimInsert;
                                }
                                b'h' => {
                                    self.cursor_pos = self.cursor_pos.saturating_sub(1);
                                }
                                b'l' => {
                                    if self.cursor_pos < self.buf_len {
                                        self.cursor_pos += 1;
                                    }
                                }
                                b'k' => {
                                    let (r, c) = self.current_line_and_col();
                                    if r > 0 {
                                        let prev_start = self.line_offsets[r - 1];
                                        let prev_len = self.line_offsets[r] - prev_start - 1;
                                        self.cursor_pos = prev_start + c.min(prev_len);
                                    }
                                }
                                b'j' => {
                                    let (r, c) = self.current_line_and_col();
                                    if r + 1 < self.line_count {
                                        let next_start = self.line_offsets[r + 1];
                                        let next_end = if r + 2 < self.line_count { self.line_offsets[r + 2] - 1 } else { self.buf_len };
                                        self.cursor_pos = next_start + c.min(next_end - next_start);
                                    }
                                }
                                b'x' => {
                                    if self.cursor_pos < self.buf_len {
                                        self.cursor_pos += 1;
                                        self.delete_char_backspace();
                                    }
                                }
                                _ => {}
                            }
                        }
                    } else {
                        self.insert_char(ch);
                    }
                }
                _ => {}
            }
        }

        // On exit, restore terminal viewport and scrollback cleanly
        redraw_terminal_viewport();
    }

    fn handle_vim_command(&mut self, cmd: &str) -> bool {
        match cmd {
            ":w" => {
                self.save_file();
                false
            }
            ":q" => {
                if self.dirty {
                    self.set_status("No write since last change (use :q! to override)");
                    false
                } else {
                    true
                }
            }
            ":wq" | ":x" => {
                if self.save_file() {
                    true
                } else {
                    false
                }
            }
            ":q!" => {
                true
            }
            _ => {
                self.set_status("Unknown editor command");
                false
            }
        }
    }
}
