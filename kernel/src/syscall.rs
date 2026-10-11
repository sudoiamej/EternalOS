use crate::vm::VirtAddr;

/// Syscall numbers supported by EternalOS ABI v1.0
pub const SYS_EXIT: usize = 0;
pub const SYS_WRITE: usize = 1;
pub const SYS_READ: usize = 2;
pub const SYS_YIELD: usize = 3;
pub const SYS_SYSINFO: usize = 4;
pub const SYS_OPEN: usize = 5;
pub const SYS_CLOSE: usize = 6;
pub const SYS_READDIR: usize = 7;
pub const SYS_STAT: usize = 8;
pub const SYS_SPAWN: usize = 9;
pub const SYS_WAITPID: usize = 10;

/// Syscall error codes
pub const ENOSYS: isize = -1;
pub const EFAULT: isize = -2;
pub const EINVAL: isize = -3;
pub const ENOENT: isize = -4;
pub const EPERM: isize = -5;
pub const EMFILE: isize = -6;
pub const EIO: isize = -7;

/// Validates that a user-supplied buffer range [addr, addr + len) is canonical,
/// completely resides in user address space (< 0x0000_8000_0000_0000), and does not overflow.
pub fn validate_user_buffer(addr: u64, len: usize) -> Result<(), isize> {
    if len == 0 {
        return Ok(());
    }

    if addr == 0 {
        return Err(EFAULT);
    }

    let end = addr.checked_add(len as u64).ok_or(EFAULT)?;

    // Must be in canonical user lower-half space
    const USER_LIMIT: u64 = 0x0000_8000_0000_0000;
    if addr >= USER_LIMIT || end > USER_LIMIT {
        return Err(EFAULT);
    }

    let vaddr = VirtAddr::new(addr);
    if !vaddr.is_canonical() {
        return Err(EFAULT);
    }

    Ok(())
}

/// Safely copy a null-terminated string from userspace up to max_len.
pub fn copy_user_string(addr: u64, max_len: usize, out: &mut [u8]) -> Result<usize, isize> {
    validate_user_buffer(addr, 1)?;

    let user_ptr = addr as *const u8;
    let mut copied = 0usize;

    for i in 0..max_len {
        let curr_addr = addr.checked_add(i as u64).ok_or(EFAULT)?;
        if curr_addr >= 0x0000_8000_0000_0000 {
            return Err(EFAULT);
        }

        let b = unsafe { core::ptr::read_volatile(user_ptr.add(i)) };
        if b == 0 {
            return Ok(copied);
        }

        if copied < out.len() {
            out[copied] = b;
            copied += 1;
        } else {
            return Err(EINVAL);
        }
    }

    Ok(copied)
}
