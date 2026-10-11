use crate::mm::allocate_frame;
use crate::vm::{map_page_in_table, PageTableFlags, PhysAddr, VirtAddr, PAGE_SIZE};

pub const ELF_MAGIC: [u8; 4] = [0x7F, b'E', b'L', b'F'];
pub const ELF_CLASS_64: u8 = 2;
pub const ELF_DATA_2LSB: u8 = 1;
pub const ET_EXEC: u16 = 2;
pub const EM_X86_64: u16 = 0x3E;

pub const PT_LOAD: u32 = 1;
pub const PF_X: u32 = 1;
pub const PF_W: u32 = 2;
pub const PF_R: u32 = 4;

/// 64-bit ELF Header representation
#[repr(C, packed)]
#[derive(Debug, Clone, Copy)]
pub struct Elf64Header {
    pub ident: [u8; 16],
    pub elf_type: u16,
    pub machine: u16,
    pub version: u32,
    pub entry: u64,
    pub phoff: u64,
    pub shoff: u64,
    pub flags: u32,
    pub ehsize: u16,
    pub phentsize: u16,
    pub phnum: u16,
    pub shentsize: u16,
    pub shnum: u16,
    pub shstrndx: u16,
}

/// 64-bit ELF Program Header
#[repr(C, packed)]
#[derive(Debug, Clone, Copy)]
pub struct Elf64ProgramHeader {
    pub seg_type: u32,
    pub flags: u32,
    pub offset: u64,
    pub vaddr: u64,
    pub paddr: u64,
    pub filesz: u64,
    pub memsz: u64,
    pub align: u64,
}

#[derive(Debug, Clone, Copy)]
pub struct LoadedElf {
    pub entry_point: u64,
}

/// Parse and load an ELF64 executable binary into the target user PML4 address space.
pub fn load_elf64(elf_bytes: &[u8], user_pml4: PhysAddr) -> Result<LoadedElf, &'static str> {
    if elf_bytes.len() < core::mem::size_of::<Elf64Header>() {
        return Err("ELF buffer smaller than header");
    }

    let hdr = unsafe { &*(elf_bytes.as_ptr() as *const Elf64Header) };

    // 1. Magic check
    if hdr.ident[0..4] != ELF_MAGIC {
        return Err("Invalid ELF magic");
    }

    // 2. Class and Endianness
    if hdr.ident[4] != ELF_CLASS_64 {
        return Err("Unsupported ELF class (requires 64-bit)");
    }
    if hdr.ident[5] != ELF_DATA_2LSB {
        return Err("Unsupported endianness (requires 2's complement little-endian)");
    }

    // 3. Executable type & machine
    if hdr.elf_type != ET_EXEC {
        return Err("Unsupported ELF type (requires ET_EXEC statically linked executable)");
    }
    if hdr.machine != EM_X86_64 {
        return Err("Unsupported machine architecture (requires x86-64)");
    }

    // 4. Entry point bounds check
    const USER_SPACE_LIMIT: u64 = 0x0000_8000_0000_0000;
    if hdr.entry >= USER_SPACE_LIMIT || hdr.entry == 0 {
        return Err("Invalid or supervisor ELF entry point");
    }

    // 5. Program Header array bounds
    let phoff = hdr.phoff as usize;
    let phentsize = hdr.phentsize as usize;
    let phnum = hdr.phnum as usize;

    if phentsize < core::mem::size_of::<Elf64ProgramHeader>() {
        return Err("Invalid program header entry size");
    }

    let total_ph_size = phentsize.checked_mul(phnum).ok_or("Arithmetic overflow in ph size")?;
    let ph_end = phoff.checked_add(total_ph_size).ok_or("Arithmetic overflow in ph table")?;
    if ph_end > elf_bytes.len() {
        return Err("Program header table extends past ELF binary buffer");
    }

    // 6. Iterate through PT_LOAD segments and map into user PML4
    for i in 0..phnum {
        let ph_offset = phoff + (i * phentsize);
        let ph = unsafe { &*(elf_bytes[ph_offset..].as_ptr() as *const Elf64ProgramHeader) };

        if ph.seg_type != PT_LOAD {
            continue;
        }

        if ph.memsz == 0 {
            continue;
        }

        if ph.filesz > ph.memsz {
            return Err("ELF segment filesz > memsz");
        }

        let seg_start = ph.vaddr;
        let seg_end = seg_start.checked_add(ph.memsz).ok_or("Overflow in segment virtual address")?;
        if seg_end > USER_SPACE_LIMIT {
            return Err("ELF segment mapping exceeds user address space limit");
        }

        // Check file offset bounds
        let file_off = ph.offset as usize;
        let file_sz = ph.filesz as usize;
        let file_end = file_off.checked_add(file_sz).ok_or("Overflow in segment file offset")?;
        if file_end > elf_bytes.len() {
            return Err("Segment file data extends beyond ELF binary buffer");
        }

        // Determine page permissions
        let mut flags = PageTableFlags::PRESENT | PageTableFlags::USER_ACCESSIBLE;
        if (ph.flags & PF_W) != 0 {
            flags |= PageTableFlags::WRITABLE;
        }
        if (ph.flags & PF_X) == 0 {
            flags |= PageTableFlags::NO_EXECUTE;
        }

        // Map pages
        let page_start = seg_start & !(PAGE_SIZE as u64 - 1);
        let page_end = (seg_end + (PAGE_SIZE as u64 - 1)) & !(PAGE_SIZE as u64 - 1);

        let mut curr_vaddr = page_start;
        while curr_vaddr < page_end {
            let pframe = allocate_frame().ok_or("PMM out of memory loading ELF segment")?;
            let pframe_virt = crate::vm::phys_to_virt(pframe).ok_or("HHDM not available")?;
            let pframe_slice = unsafe { core::slice::from_raw_parts_mut(pframe_virt.as_mut_ptr::<u8>(), PAGE_SIZE) };
            pframe_slice.fill(0); // Clear to 0 for BSS / padding

            // Copy file data if this page intersects [seg_start, seg_start + filesz)
            let file_data_end = seg_start + (file_sz as u64);
            if curr_vaddr < file_data_end && (curr_vaddr + PAGE_SIZE as u64) > seg_start {
                let page_offset = if curr_vaddr < seg_start {
                    (seg_start - curr_vaddr) as usize
                } else {
                    0
                };

                let file_byte_offset = if curr_vaddr > seg_start {
                    (curr_vaddr - seg_start) as usize
                } else {
                    0
                };

                let copy_len = (PAGE_SIZE - page_offset).min(file_sz.saturating_sub(file_byte_offset));
                if copy_len > 0 {
                    let src_start = file_off + file_byte_offset;
                    pframe_slice[page_offset..page_offset + copy_len]
                        .copy_from_slice(&elf_bytes[src_start..src_start + copy_len]);
                }
            }

            unsafe {
                map_page_in_table(user_pml4, VirtAddr::new(curr_vaddr), pframe, flags)?;
            }

            curr_vaddr += PAGE_SIZE as u64;
        }
    }

    Ok(LoadedElf {
        entry_point: hdr.entry,
    })
}
