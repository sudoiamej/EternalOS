use core::arch::asm;
use core::sync::atomic::{AtomicBool, Ordering};

pub const KERNEL_CS: u16 = 0x28; // Limine 64-bit kernel code segment
pub const KERNEL_DS: u16 = 0x30; // Limine 64-bit kernel data segment

#[repr(C, packed)]
#[derive(Clone, Copy)]
pub struct GdtDescriptor {
    limit_low: u16,
    base_low: u16,
    base_middle: u8,
    access: u8,
    granularity: u8,
    base_high: u8,
}

impl GdtDescriptor {
    pub const fn null() -> Self {
        Self {
            limit_low: 0,
            base_low: 0,
            base_middle: 0,
            access: 0,
            granularity: 0,
            base_high: 0,
        }
    }

    pub const fn kernel_code() -> Self {
        Self {
            limit_low: 0,
            base_low: 0,
            base_middle: 0,
            access: 0x9A, // Present, Ring 0, Executable, Readable
            granularity: 0x20, // 64-bit Long mode (L bit set)
            base_high: 0,
        }
    }

    pub const fn kernel_data() -> Self {
        Self {
            limit_low: 0,
            base_low: 0,
            base_middle: 0,
            access: 0x92, // Present, Ring 0, Writable
            granularity: 0x00,
            base_high: 0,
        }
    }

    pub const fn user_data() -> Self {
        Self {
            limit_low: 0,
            base_low: 0,
            base_middle: 0,
            access: 0xF2, // Present, Ring 3 (DPL 3), Writable
            granularity: 0x00,
            base_high: 0,
        }
    }

    pub const fn user_code() -> Self {
        Self {
            limit_low: 0,
            base_low: 0,
            base_middle: 0,
            access: 0xFA, // Present, Ring 3 (DPL 3), Executable, Readable
            granularity: 0x20, // 64-bit Long mode (L bit set)
            base_high: 0,
        }
    }
}

/// 64-bit System Segment Descriptor (for TSS) - 16 bytes
#[repr(C, packed)]
#[derive(Clone, Copy)]
pub struct TssDescriptor {
    length: u16,
    base_low: u16,
    base_mid: u8,
    flags1: u8,
    flags2: u8,
    base_high: u8,
    base_upper32: u32,
    reserved: u32,
}

impl TssDescriptor {
    pub const fn new(base: u64, limit: u16) -> Self {
        Self {
            length: limit,
            base_low: base as u16,
            base_mid: (base >> 16) as u8,
            flags1: 0x89, // Present, 64-bit Available TSS (Type 0x9)
            flags2: 0x00,
            base_high: (base >> 24) as u8,
            base_upper32: (base >> 32) as u32,
            reserved: 0,
        }
    }
}

/// Standard x86-64 Task State Segment
#[repr(C, packed)]
pub struct TaskStateSegment {
    reserved0: u32,
    pub rsp0: u64,
    pub rsp1: u64,
    pub rsp2: u64,
    reserved1: u64,
    pub ist1: u64, // Double Fault stack
    pub ist2: u64, // NMI / Machine Check stack
    pub ist3: u64,
    pub ist4: u64,
    pub ist5: u64,
    pub ist6: u64,
    pub ist7: u64,
    reserved2: u64,
    reserved3: u16,
    pub iomap_base: u16,
}

impl TaskStateSegment {
    pub const fn new() -> Self {
        Self {
            reserved0: 0,
            rsp0: 0,
            rsp1: 0,
            rsp2: 0,
            reserved1: 0,
            ist1: 0,
            ist2: 0,
            ist3: 0,
            ist4: 0,
            ist5: 0,
            ist6: 0,
            ist7: 0,
            reserved2: 0,
            reserved3: 0,
            iomap_base: 104, // Size of TSS excluding I/O permission bitmap (no I/O map)
        }
    }
}

#[repr(C, packed)]
pub struct GdtPointer {
    pub limit: u16,
    pub base: u64,
}

// Dedicated IST stacks (16 KiB = 4 pages each, 16-byte aligned)
#[repr(align(16))]
struct ExceptionStack([u8; 16384]);

static mut DOUBLE_FAULT_STACK: ExceptionStack = ExceptionStack([0; 16384]);
static mut NMI_STACK: ExceptionStack = ExceptionStack([0; 16384]);

// Static TSS instance
pub static mut TSS: TaskStateSegment = TaskStateSegment::new();

// In x86-64, Limine configures:
// 0x00: Null
// 0x08: 16-bit code
// 0x10: 16-bit data
// 0x18: 32-bit code
// 0x20: 32-bit data
// 0x28: 64-bit kernel code (CS = 0x28)
// 0x30: 64-bit kernel data (DS/SS = 0x30)
pub const USER_DS: u16 = 0x38 | 3; // 64-bit user data segment (RPL 3) -> 0x3B
pub const USER_CS: u16 = 0x40 | 3; // 64-bit user code segment (RPL 3) -> 0x43
pub const TSS_SELECTOR: u16 = 0x48; // 16-byte TSS descriptor -> Selector 0x48

#[repr(C, align(16))]
pub struct Gdt {
    pub entries: [GdtDescriptor; 9],
    pub tss: TssDescriptor,
}

static mut GDT: Gdt = Gdt {
    entries: [
        GdtDescriptor::null(),        // 0x00
        GdtDescriptor::null(),        // 0x08
        GdtDescriptor::null(),        // 0x10
        GdtDescriptor::null(),        // 0x18
        GdtDescriptor::null(),        // 0x20
        GdtDescriptor::kernel_code(), // 0x28
        GdtDescriptor::kernel_data(), // 0x30
        GdtDescriptor::user_data(),   // 0x38 (with RPL 3 = 0x3B)
        GdtDescriptor::user_code(),   // 0x40 (with RPL 3 = 0x43)
    ],
    tss: TssDescriptor::new(0, 0),    // 0x48
};

static GDT_INITIALIZED: AtomicBool = AtomicBool::new(false);

pub fn is_initialized() -> bool {
    GDT_INITIALIZED.load(Ordering::Acquire)
}

pub fn get_cs() -> u16 {
    let cs: u16;
    unsafe {
        asm!("mov {0:x}, cs", out(reg) cs, options(nomem, nostack, preserves_flags));
    }
    cs
}

pub fn get_ss() -> u16 {
    let ss: u16;
    unsafe {
        asm!("mov {0:x}, ss", out(reg) ss, options(nomem, nostack, preserves_flags));
    }
    ss
}

pub fn get_tr() -> u16 {
    let tr: u16;
    unsafe {
        asm!("str {0:x}", out(reg) tr, options(nomem, nostack, preserves_flags));
    }
    tr
}

pub fn set_tss_rsp0(stack_top: u64) {
    unsafe {
        TSS.rsp0 = stack_top;
    }
}

pub fn init() {
    unsafe {
        // 1. Configure IST stack pointers in TSS (top of stack, 16-byte aligned)
        let df_stack_top = (&raw const DOUBLE_FAULT_STACK.0 as u64) + 16384;
        let nmi_stack_top = (&raw const NMI_STACK.0 as u64) + 16384;

        TSS.ist1 = df_stack_top;
        TSS.ist2 = nmi_stack_top;

        // 2. Setup 16-byte TSS descriptor in GDT pointing to our TSS
        let tss_base = &raw const TSS as u64;
        let tss_limit = (core::mem::size_of::<TaskStateSegment>() - 1) as u16;
        GDT.tss = TssDescriptor::new(tss_base, tss_limit);

        // 3. Load GDT using lgdt
        let gdt_ptr = GdtPointer {
            limit: (core::mem::size_of::<Gdt>() - 1) as u16,
            base: &raw const GDT as u64,
        };

        asm!("lgdt [{}]", in(reg) &gdt_ptr, options(readonly, nostack, preserves_flags));

        // 4. Reload segment registers
        asm!(
            "push 0x28",
            "lea rax, [rip + 2f]",
            "push rax",
            "retfq",
            "2:",
            "mov ax, 0x30",
            "mov ds, ax",
            "mov es, ax",
            "mov ss, ax",
            "mov fs, ax",
            "mov gs, ax",
            out("rax") _,
            options(nostack)
        );

        // 5. Load Task State Segment (LTR)
        asm!("ltr {0:x}", in(reg) TSS_SELECTOR, options(nomem, nostack, preserves_flags));
    }

    GDT_INITIALIZED.store(true, Ordering::Release);
    crate::log_info!("GDT & TSS loaded: CS={:#04x}, SS={:#04x}, TR={:#04x}, IST1(DF)={:#018x}", get_cs(), get_ss(), get_tr(), unsafe { TSS.ist1 });
}
