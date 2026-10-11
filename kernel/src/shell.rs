use crate::console::{console_read_byte, console_write, console_write_byte};
use crate::vfs::{with_vfs, DirEntry, NodeType, OpenFlags};

/// Built-in interactive command-line shell running in userspace execution model.
pub fn run_interactive_shell() {
    // Format dedicated terminal viewport on graphical framebuffer
    crate::console::init_terminal_viewport();

    console_write("\r\n");
    console_write("====================================================\r\n");
    console_write("        EternalOS Interactive Shell (Phase 15)      \r\n");
    console_write("====================================================\r\n");
    console_write("Type 'help' for available commands.\r\n\r\n");

    let mut line_buf = [0u8; 128];
    let mut current_dir = [0u8; 64];
    current_dir[0] = b'/';
    let mut dir_len = 1usize;

    loop {
        let cwd_str = core::str::from_utf8(&current_dir[..dir_len]).unwrap_or("/");

        // Print prompt
        console_write("EternalOS> ");

        let mut line_len = 0usize;

        // Read line
        crate::console::draw_text_cursor(true);
        loop {
            if let Some(b) = console_read_byte() {
                crate::console::draw_text_cursor(false);
                if b == b'\r' || b == b'\n' {
                    console_write("\r\n");
                    break;
                } else if b == 0x08 || b == 0x7F {
                    // Backspace
                    if line_len > 0 {
                        line_len -= 1;
                        console_write_byte(0x08);
                    }
                } else if b >= 0x20 && b <= 0x7E {
                    if line_len < 127 {
                        line_buf[line_len] = b;
                        line_len += 1;
                        console_write_byte(b);
                    }
                }
                crate::console::draw_text_cursor(true);
            } else {
                core::hint::spin_loop();
            }
        }
        crate::console::draw_text_cursor(false);

        let input_str = match core::str::from_utf8(&line_buf[..line_len]) {
            Ok(s) => s.trim(),
            Err(_) => continue,
        };

        if input_str.is_empty() {
            continue;
        }

        // Parse command and argument
        let mut parts = input_str.splitn(2, ' ');
        let cmd = parts.next().unwrap_or("");
        let arg = parts.next().unwrap_or("");

        match cmd {
            "help" => {
                console_write("EternalOS Shell Commands (15 available):\r\n");
                console_write("  help       - Display this list of commands\r\n");
                console_write("  clear      - Clear terminal screen\r\n");
                console_write("  echo <msg> - Print message to terminal\r\n");
                console_write("  pwd        - Print current working directory\r\n");
                console_write("  ls [dir]   - List files and directories\r\n");
                console_write("  cd <dir>   - Change current working directory\r\n");
                console_write("  cat <file> - Print contents of file\r\n");
                console_write("  stat <path>- Display file or directory metadata\r\n");
                console_write("  uname      - Print operating system & kernel info\r\n");
                console_write("  meminfo    - Display physical memory frame stats\r\n");
                console_write("  lspci      - List enumerated PCI/PCIe hardware devices\r\n");
                console_write("  uptime     - Display system uptime and PIT timer ticks\r\n");
                console_write("  mouse      - Display mouse pointer status and coordinates\r\n");
                console_write("  vfsinfo    - Display root filesystem & mount status\r\n");
                console_write("  exit       - Exit interactive shell\r\n");
            }

            "clear" => {
                crate::console::console_clear();
            }

            "echo" => {
                console_write(arg);
                console_write("\r\n");
            }

            "pwd" => {
                console_write(cwd_str);
                console_write("\r\n");
            }

            "uname" => {
                console_write("EternalOS 0.1.0 x86_64 Limine hybrid kernel\r\n");
            }

            "meminfo" => {
                let stats = crate::mm::get_frame_allocator_stats();
                console_write("Physical Memory Stats:\r\n");
                let mut num_buf = [0u8; 32];
                console_write("  Total Frames    : ");
                console_write(format_u64(stats.total_frames as u64, &mut num_buf));
                console_write("\r\n  Allocated Frames: ");
                console_write(format_u64(stats.allocated_frames as u64, &mut num_buf));
                console_write("\r\n  Free Frames     : ");
                console_write(format_u64(stats.free_frames as u64, &mut num_buf));
                console_write("\r\n");
            }

            "cd" => {
                if arg.is_empty() || arg == "/" {
                    current_dir[0] = b'/';
                    dir_len = 1;
                } else if arg == ".." {
                    // Go up one level
                    if dir_len > 1 {
                        if let Some(last_slash) = cwd_str.rfind('/') {
                            if last_slash == 0 {
                                current_dir[0] = b'/';
                                dir_len = 1;
                            } else {
                                dir_len = last_slash;
                            }
                        }
                    }
                } else {
                    let mut combined = [0u8; 128];
                    let c_len;

                    if arg.starts_with('/') {
                        let arg_b = arg.as_bytes();
                        let len = arg_b.len().min(127);
                        combined[..len].copy_from_slice(&arg_b[..len]);
                        c_len = len;
                    } else {
                        let mut len = dir_len;
                        combined[..dir_len].copy_from_slice(&current_dir[..dir_len]);
                        if !cwd_str.ends_with('/') {
                            combined[len] = b'/';
                            len += 1;
                        }
                        let arg_b = arg.as_bytes();
                        let to_copy = arg_b.len().min(128 - len);
                        combined[len..len + to_copy].copy_from_slice(&arg_b[..to_copy]);
                        len += to_copy;
                        c_len = len;
                    }

                    let target_path = core::str::from_utf8(&combined[..c_len]).unwrap_or(arg);

                    match with_vfs(|vfs| vfs.stat(target_path)) {
                        Ok(meta) if meta.node_type == NodeType::Directory => {
                            let b = target_path.as_bytes();
                            let len = b.len().min(63);
                            current_dir[..len].copy_from_slice(&b[..len]);
                            dir_len = len;
                        }
                        Ok(_) => {
                            console_write("cd: not a directory: ");
                            console_write(arg);
                            console_write("\r\n");
                        }
                        Err(_) => {
                            console_write("cd: directory not found: ");
                            console_write(arg);
                            console_write("\r\n");
                        }
                    }
                }
            }

            "ls" => {
                let target_dir = if arg.is_empty() { cwd_str } else { arg };
                let mut dir_entries = [DirEntry::empty(); 16];
                let count = with_vfs(|vfs| vfs.readdir(target_dir, 0, &mut dir_entries));
                match count {
                    Ok(cnt) => {
                        for ent in dir_entries.iter().take(cnt) {
                            let name = ent.name_as_str();
                            if ent.node_type == NodeType::Directory {
                                console_write("  [DIR]  ");
                            } else {
                                console_write("  [FILE] ");
                            }
                            console_write(name);
                            console_write("\r\n");
                        }
                    }
                    Err(e) => {
                        console_write("ls: error reading directory: ");
                        console_write(target_dir);
                        console_write(" (");
                        let mut e_buf = [0u8; 32];
                        console_write(format_u64(e as u64, &mut e_buf));
                        console_write(")\r\n");
                    }
                }
            }

            "cat" => {
                if arg.is_empty() {
                    console_write("cat: missing file argument\r\n");
                } else {
                    let full_path = if arg.starts_with('/') {
                        arg
                    } else {
                        arg // VFS router handles path resolution
                    };

                    let res: Result<([u8; 1024], usize), crate::vfs::VfsError> = with_vfs(|vfs| {
                        let handle = vfs.open(full_path, OpenFlags::READ_ONLY)?;
                        let mut buf = [0u8; 1024];
                        let read_bytes = vfs.read(handle, &mut buf)?;
                        let _ = vfs.close(handle);
                        Ok((buf, read_bytes))
                    });

                    match res {
                        Ok((buf, bytes)) => {
                            if let Ok(text) = core::str::from_utf8(&buf[..bytes]) {
                                console_write(text);
                                if !text.ends_with('\n') {
                                    console_write("\r\n");
                                }
                            } else {
                                console_write("<binary data>\r\n");
                            }
                        }
                        Err(_) => {
                            console_write("cat: failed to open/read file: ");
                            console_write(arg);
                            console_write("\r\n");
                        }
                    }
                }
            }

            "stat" => {
                if arg.is_empty() {
                    console_write("stat: missing path argument\r\n");
                } else {
                    match with_vfs(|vfs| vfs.stat(arg)) {
                        Ok(meta) => {
                            console_write("Path: ");
                            console_write(arg);
                            console_write("\r\n  Inode: ");
                            let mut n_buf = [0u8; 32];
                            console_write(format_u64(meta.inode, &mut n_buf));
                            console_write("\r\n  Type : ");
                            console_write(if meta.node_type == NodeType::Directory { "Directory" } else { "RegularFile" });
                            console_write("\r\n  Size : ");
                            console_write(format_u64(meta.size, &mut n_buf));
                            console_write(" bytes\r\n");
                        }
                        Err(_) => {
                            console_write("stat: cannot stat '");
                            console_write(arg);
                            console_write("': No such file or directory\r\n");
                        }
                    }
                }
            }

            "lspci" => {
                let count = crate::pci::get_device_count();
                console_write("Discovered PCI Devices (");
                let mut c_buf = [0u8; 32];
                console_write(format_u64(count as u64, &mut c_buf));
                console_write(" total):\r\n");

                crate::pci::with_pci_registry(|reg| {
                    for dev in reg.devices().iter().take(16) {
                        console_write("  [");
                        let mut b_buf = [0u8; 32];
                        console_write(format_u64(dev.bus as u64, &mut b_buf));
                        console_write(":");
                        console_write(format_u64(dev.device as u64, &mut b_buf));
                        console_write(".");
                        console_write(format_u64(dev.function as u64, &mut b_buf));
                        console_write("] ");
                        console_write(dev.class_name());
                        console_write(" (Vendor: 0x");
                        console_write(format_hex_u16(dev.vendor_id, &mut b_buf));
                        console_write(", Device: 0x");
                        console_write(format_hex_u16(dev.device_id, &mut b_buf));
                        console_write(")\r\n");
                    }
                });
            }

            "uptime" => {
                let ticks = crate::timer::get_ticks();
                let seconds = ticks / crate::timer::PIT_HZ;
                let ms = (ticks % crate::timer::PIT_HZ) * 10;
                let mut u_buf = [0u8; 32];
                console_write("System Uptime: ");
                console_write(format_u64(seconds, &mut u_buf));
                console_write("s ");
                console_write(format_u64(ms, &mut u_buf));
                console_write("ms (Ticks: ");
                console_write(format_u64(ticks, &mut u_buf));
                console_write(" @ 100 Hz)\r\n");
            }

            "mouse" => {
                let (x, y, lb, rb, mb) = {
                    let ptr = crate::input::POINTER.lock();
                    (ptr.x, ptr.y, ptr.left_button, ptr.right_button, ptr.middle_button)
                };
                let mut m_buf = [0u8; 32];
                console_write("PS/2 Mouse Subsystem Status:\r\n");
                console_write("  Coordinates: X=");
                console_write(format_u64(x as u64, &mut m_buf));
                console_write(", Y=");
                console_write(format_u64(y as u64, &mut m_buf));
                console_write("\r\n  Buttons    : Left=");
                console_write(if lb { "DOWN" } else { "UP" });
                console_write(", Right=");
                console_write(if rb { "DOWN" } else { "UP" });
                console_write(", Middle=");
                console_write(if mb { "DOWN" } else { "UP" });
                console_write("\r\n  Hardware   : IRQ12 / Vector 44 (Streaming Mode)\r\n");
            }

            "vfsinfo" => {
                console_write("Virtual Filesystem (VFS) Topology:\r\n");
                console_write("  Root Mount   : / (RamFs in-memory backend)\r\n");
                console_write("  EternalFS    : Supported (/disk)\r\n");
                console_write("  Active CWD   : ");
                console_write(cwd_str);
                console_write("\r\n");
            }

            "exit" => {
                console_write("Shell exited. Halting session.\r\n");
                break;
            }

            unknown => {
                console_write("shell: command not found: ");
                console_write(unknown);
                console_write("\r\n");
            }
        }
    }
}

fn format_u64(mut n: u64, buf: &mut [u8; 32]) -> &str {
    if n == 0 {
        return "0";
    }
    let mut i = 31;
    while n > 0 && i > 0 {
        buf[i] = b'0' + (n % 10) as u8;
        n /= 10;
        i -= 1;
    }
    core::str::from_utf8(&buf[i + 1..32]).unwrap_or("0")
}

fn format_hex_u16(n: u16, buf: &mut [u8; 32]) -> &str {
    const HEX: &[u8; 16] = b"0123456789ABCDEF";
    buf[0] = HEX[((n >> 12) & 0xF) as usize];
    buf[1] = HEX[((n >> 8) & 0xF) as usize];
    buf[2] = HEX[((n >> 4) & 0xF) as usize];
    buf[3] = HEX[(n & 0xF) as usize];
    core::str::from_utf8(&buf[..4]).unwrap_or("0000")
}
