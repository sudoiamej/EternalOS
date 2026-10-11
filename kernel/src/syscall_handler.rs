use crate::process::UserTrapFrame;
use crate::syscall::*;
use crate::console::{console_read_byte, console_write_byte};
use crate::vfs::{with_vfs, OpenFlags, DirEntry};
use crate::mm::get_frame_allocator_stats;

#[unsafe(naked)]
#[unsafe(no_mangle)]
pub unsafe extern "C" fn syscall_entry_stub() {
    core::arch::naked_asm!(
        // Save general-purpose registers to form UserTrapFrame matching process::UserTrapFrame
        "push rax",
        "push rbx",
        "push rcx",
        "push rdx",
        "push rsi",
        "push rdi",
        "push rbp",
        "push r8",
        "push r9",
        "push r10",
        "push r11",
        "push r12",
        "push r13",
        "push r14",
        "push r15",

        // Pass pointer to UserTrapFrame in %rdi
        "mov rdi, rsp",
        "call handle_syscall",

        // Restore registers
        "pop r15",
        "pop r14",
        "pop r13",
        "pop r12",
        "pop r11",
        "pop r10",
        "pop r9",
        "pop r8",
        "pop rbp",
        "pop rdi",
        "pop rsi",
        "pop rdx",
        "pop rcx",
        "pop rbx",
        "pop rax",

        // Return from interrupt to user mode
        "iretq",
    );
}

#[unsafe(no_mangle)]
pub extern "C" fn handle_syscall(tf: &mut UserTrapFrame) {
    let syscall_num = tf.rax as usize;
    let arg1 = tf.rdi;
    let arg2 = tf.rsi;
    let arg3 = tf.rdx;
    let _arg4 = tf.r10;

    let res: isize = match syscall_num {
        SYS_EXIT => {
            let status = arg1 as i32;
            crate::log_info!("[SYSCALL] Process exit requested with code: {}", status);
            // Put return code into rax
            tf.rax = status as u64;
            // Return status code
            status as isize
        }

        SYS_WRITE => {
            let fd = arg1 as usize;
            let buf_addr = arg2;
            let len = arg3 as usize;

            if let Err(e) = validate_user_buffer(buf_addr, len) {
                e
            } else if fd == 1 || fd == 2 {
                // Console stdout / stderr
                let ptr = buf_addr as *const u8;
                for i in 0..len {
                    let b = unsafe { core::ptr::read_volatile(ptr.add(i)) };
                    console_write_byte(b);
                }
                len as isize
            } else {
                EINVAL
            }
        }

        SYS_READ => {
            let fd = arg1 as usize;
            let buf_addr = arg2;
            let len = arg3 as usize;

            if let Err(e) = validate_user_buffer(buf_addr, len) {
                e
            } else if fd == 0 {
                // Console stdin
                let ptr = buf_addr as *mut u8;
                let mut read_bytes = 0usize;
                while read_bytes < len {
                    if let Some(b) = console_read_byte() {
                        unsafe { core::ptr::write_volatile(ptr.add(read_bytes), b) };
                        read_bytes += 1;
                        if b == b'\n' || b == b'\r' {
                            break;
                        }
                    } else if read_bytes > 0 {
                        break;
                    } else {
                        // Cooperative yield / pause so we don't spin endlessly
                        core::hint::spin_loop();
                    }
                }
                read_bytes as isize
            } else {
                EINVAL
            }
        }

        SYS_YIELD => {
            0
        }

        SYS_SYSINFO => {
            let out_addr = arg1;
            if let Err(e) = validate_user_buffer(out_addr, 16) {
                e
            } else {
                let stats = get_frame_allocator_stats();
                let ptr = out_addr as *mut u64;
                unsafe {
                    core::ptr::write_volatile(ptr, stats.total_frames as u64);
                    core::ptr::write_volatile(ptr.add(1), stats.allocated_frames as u64);
                }
                0
            }
        }

        SYS_OPEN => {
            let path_addr = arg1;
            let flags = arg2 as u32;

            let mut path_buf = [0u8; 128];
            match copy_user_string(path_addr, 127, &mut path_buf) {
                Ok(len) => {
                    if let Ok(path_str) = core::str::from_utf8(&path_buf[..len]) {
                        let open_flags = if (flags & 1) != 0 {
                            OpenFlags::CREATE_OR_WRITE
                        } else {
                            OpenFlags::READ_ONLY
                        };

                        let res = with_vfs(|vfs| vfs.open(path_str, open_flags));
                        match res {
                            Ok(handle) => handle.fd as isize,
                            Err(_) => ENOENT,
                        }
                    } else {
                        EINVAL
                    }
                }
                Err(e) => e,
            }
        }

        SYS_CLOSE => {
            let fd = arg1 as usize;
            let res = with_vfs(|vfs| {
                let handle = crate::vfs::FileHandle { fd, generation: 1 };
                vfs.close(handle)
            });
            match res {
                Ok(_) => 0,
                Err(_) => EINVAL,
            }
        }

        SYS_READDIR => {
            let path_addr = arg1;
            let offset = arg2 as usize;
            let out_addr = arg3;

            if let Err(e) = validate_user_buffer(out_addr, 32) {
                e
            } else {
                let mut path_buf = [0u8; 128];
                match copy_user_string(path_addr, 127, &mut path_buf) {
                    Ok(len) => {
                        if let Ok(path_str) = core::str::from_utf8(&path_buf[..len]) {
                            let mut dir_buf = [DirEntry::empty(); 1];
                            let read_count = with_vfs(|vfs| vfs.readdir(path_str, offset, &mut dir_buf));
                            match read_count {
                                Ok(cnt) if cnt > 0 => {
                                    let name_str = dir_buf[0].name_as_str();
                                    let name_bytes = name_str.as_bytes();
                                    let name_len = name_bytes.len().min(27);
                                    let out_ptr = out_addr as *mut u8;
                                    unsafe {
                                        out_ptr.write_volatile(if dir_buf[0].node_type == crate::vfs::NodeType::Directory { 2 } else { 1 });
                                        out_ptr.add(1).write_volatile(name_len as u8);
                                        for k in 0..name_len {
                                            out_ptr.add(2 + k).write_volatile(name_bytes[k]);
                                        }
                                        out_ptr.add(2 + name_len).write_volatile(0);
                                    }
                                    1
                                }
                                Ok(_) => 0,
                                Err(_) => ENOENT,
                            }
                        } else {
                            EINVAL
                        }
                    }
                    Err(e) => e,
                }
            }
        }

        SYS_STAT => {
            let path_addr = arg1;
            let out_addr = arg2;

            if let Err(e) = validate_user_buffer(out_addr, 16) {
                e
            } else {
                let mut path_buf = [0u8; 128];
                match copy_user_string(path_addr, 127, &mut path_buf) {
                    Ok(len) => {
                        if let Ok(path_str) = core::str::from_utf8(&path_buf[..len]) {
                            match with_vfs(|vfs| vfs.stat(path_str)) {
                                Ok(meta) => {
                                    let out_ptr = out_addr as *mut u64;
                                    unsafe {
                                        out_ptr.write_volatile(meta.size);
                                        out_ptr.add(1).write_volatile(if meta.node_type == crate::vfs::NodeType::Directory { 2 } else { 1 });
                                    }
                                    0
                                }
                                Err(_) => ENOENT,
                            }
                        } else {
                            EINVAL
                        }
                    }
                    Err(e) => e,
                }
            }
        }

        _ => {
            crate::log_warn!("[SYSCALL] Unimplemented syscall number: {}", syscall_num);
            ENOSYS
        }
    };

    // Return value in %rax
    tf.rax = res as u64;
}
