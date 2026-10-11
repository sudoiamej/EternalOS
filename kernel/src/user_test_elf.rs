/// Construct a valid static ELF64 executable in memory that prints a message via int 0x80 (SYS_WRITE)
/// and exits with exit code `exit_status` via int 0x80 (SYS_EXIT).
pub fn build_test_elf64_binary(out: &mut [u8; 1024], exit_status: u32, message: &str) -> usize {
    out.fill(0);

    let msg_bytes = message.as_bytes();
    let msg_len = msg_bytes.len().min(128);

    // Virtual entry address: 0x0000_0000_0040_0000
    // Header size: 64 bytes (Elf64Header) + 56 bytes (Elf64ProgramHeader) = 120 bytes
    // We will place the code right after the headers at file offset 128 (0x80), vaddr 0x00400080.
    let entry_vaddr = 0x0040_0080u64;

    // --- ELF64 Header (64 bytes) ---
    out[0..4].copy_from_slice(&[0x7F, b'E', b'L', b'F']); // e_ident[EI_MAG0..3]
    out[4] = 2; // EI_CLASS = 64-bit
    out[5] = 1; // EI_DATA = 2's complement, little-endian
    out[6] = 1; // EI_VERSION = 1
    out[7] = 0; // EI_OSABI = System V
    out[8] = 0; // EI_ABIVERSION = 0

    out[16..18].copy_from_slice(&2u16.to_le_bytes()); // e_type = ET_EXEC
    out[18..20].copy_from_slice(&0x3Eu16.to_le_bytes()); // e_machine = EM_X86_64
    out[20..24].copy_from_slice(&1u32.to_le_bytes()); // e_version = 1
    out[24..32].copy_from_slice(&entry_vaddr.to_le_bytes()); // e_entry
    out[32..40].copy_from_slice(&64u64.to_le_bytes()); // e_phoff = 64
    out[40..48].copy_from_slice(&0u64.to_le_bytes()); // e_shoff = 0
    out[48..52].copy_from_slice(&0u32.to_le_bytes()); // e_flags = 0
    out[52..54].copy_from_slice(&64u16.to_le_bytes()); // e_ehsize = 64
    out[54..56].copy_from_slice(&56u16.to_le_bytes()); // e_phentsize = 56
    out[56..58].copy_from_slice(&1u16.to_le_bytes()); // e_phnum = 1
    out[58..60].copy_from_slice(&0u16.to_le_bytes()); // e_shentsize = 0
    out[60..62].copy_from_slice(&0u16.to_le_bytes()); // e_shnum = 0
    out[62..64].copy_from_slice(&0u16.to_le_bytes()); // e_shstrndx = 0

    // --- Program Header (56 bytes, offset 64..120) ---
    // PT_LOAD segment covering file offset 0 to 512, mapped at vaddr 0x0040_0000
    out[64..68].copy_from_slice(&1u32.to_le_bytes()); // p_type = PT_LOAD
    out[68..72].copy_from_slice(&(1u32 | 2u32 | 4u32).to_le_bytes()); // p_flags = PF_X | PF_W | PF_R
    out[72..80].copy_from_slice(&0u64.to_le_bytes()); // p_offset = 0
    out[80..88].copy_from_slice(&0x0040_0000u64.to_le_bytes()); // p_vaddr
    out[88..96].copy_from_slice(&0x0040_0000u64.to_le_bytes()); // p_paddr
    out[96..104].copy_from_slice(&512u64.to_le_bytes()); // p_filesz = 512
    out[104..112].copy_from_slice(&512u64.to_le_bytes()); // p_memsz = 512
    out[112..120].copy_from_slice(&0x1000u64.to_le_bytes()); // p_align = 4096

    // --- Code & Data at offset 128 (0x80) ---
    // Instructions:
    // 1. mov rax, 1 (SYS_WRITE)
    // 2. mov rdi, 1 (fd = stdout)
    // 3. lea rsi, [rip + msg_offset]
    // 4. mov rdx, msg_len
    // 5. int 0x80
    // 6. mov rax, 0 (SYS_EXIT)
    // 7. mov rdi, exit_status
    // 8. int 0x80
    // 9. hlt (or jmp $)

    let code_start = 128;
    let mut c = code_start;

    // mov rax, 1 -> 48 c7 c0 01 00 00 00
    out[c..c+7].copy_from_slice(&[0x48, 0xC7, 0xC0, 0x01, 0x00, 0x00, 0x00]);
    c += 7;

    // mov rdi, 1 -> 48 c7 c7 01 00 00 00
    out[c..c+7].copy_from_slice(&[0x48, 0xC7, 0xC7, 0x01, 0x00, 0x00, 0x00]);
    c += 7;

    // lea rsi, [rip + 35] -> 48 8d 35 23 00 00 00
    // We'll compute the RIP-relative offset to msg
    let lea_rip_offset_loc = c + 3;
    out[c..c+7].copy_from_slice(&[0x48, 0x8D, 0x35, 0x00, 0x00, 0x00, 0x00]);
    c += 7;

    // mov rdx, msg_len -> 48 c7 c2 [len] 00 00 00
    out[c..c+7].copy_from_slice(&[0x48, 0xC7, 0xC2, msg_len as u8, 0x00, 0x00, 0x00]);
    c += 7;

    // int 0x80 -> cd 80
    out[c..c+2].copy_from_slice(&[0xCD, 0x80]);
    c += 2;

    // mov rax, 0 (SYS_EXIT) -> 48 c7 c0 00 00 00 00
    out[c..c+7].copy_from_slice(&[0x48, 0xC7, 0xC0, 0x00, 0x00, 0x00, 0x00]);
    c += 7;

    // mov rdi, exit_status -> 48 c7 c7 [status] 00 00 00
    out[c..c+7].copy_from_slice(&[0x48, 0xC7, 0xC7, exit_status as u8, 0x00, 0x00, 0x00]);
    c += 7;

    // int 0x80 -> cd 80
    out[c..c+2].copy_from_slice(&[0xCD, 0x80]);
    c += 2;

    // jmp $ -> eb fe (in case exit returns)
    out[c..c+2].copy_from_slice(&[0xEB, 0xFE]);
    c += 2;

    // Now place message bytes
    let msg_start = c;
    let rel_offset = (msg_start as i32) - ((lea_rip_offset_loc + 4) as i32);
    out[lea_rip_offset_loc..lea_rip_offset_loc+4].copy_from_slice(&(rel_offset as i32).to_le_bytes());

    out[msg_start..msg_start + msg_len].copy_from_slice(msg_bytes);
    c += msg_len;

    let filesz = c as u64;
    out[96..104].copy_from_slice(&filesz.to_le_bytes()); // p_filesz
    out[104..112].copy_from_slice(&filesz.to_le_bytes()); // p_memsz

    c
}
