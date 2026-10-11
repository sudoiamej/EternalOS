use core::arch::asm;
use core::fmt;

#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct InterruptStackFrame {
    pub rip: u64,
    pub cs: u64,
    pub rflags: u64,
    pub rsp: u64,
    pub ss: u64,
}

impl fmt::Display for InterruptStackFrame {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "RIP={:#018x}, CS={:#06x}, RFLAGS={:#018x}, RSP={:#018x}, SS={:#06x}",
            self.rip, self.cs, self.rflags, self.rsp, self.ss
        )
    }
}

#[derive(Clone, Copy)]
#[repr(C, packed)]
pub struct IdtEntry {
    pointer_low: u16,
    gdt_selector: u16,
    options: u16,
    pointer_middle: u16,
    pointer_high: u32,
    reserved: u32,
}

impl IdtEntry {
    pub const fn missing() -> Self {
        Self {
            pointer_low: 0,
            gdt_selector: 0,
            options: 0,
            pointer_middle: 0,
            pointer_high: 0,
            reserved: 0,
        }
    }

    pub fn set_handler(&mut self, handler: usize) {
        self.pointer_low = handler as u16;
        self.gdt_selector = 0x08; // Kernel code segment selector
        self.options = 0x8E00;    // Present (bit 15), DPL 0, 64-bit Interrupt Gate (0xE)
        self.pointer_middle = (handler >> 16) as u16;
        self.pointer_high = (handler >> 32) as u32;
        self.reserved = 0;
    }
}

#[repr(C, align(16))]
pub struct Idt {
    pub entries: [IdtEntry; 256],
}

impl Idt {
    pub const fn new() -> Self {
        Self {
            entries: [IdtEntry::missing(); 256],
        }
    }
}

#[repr(C, packed)]
pub struct IdtPointer {
    pub limit: u16,
    pub base: u64,
}

static mut IDT: Idt = Idt::new();

pub extern "x86-interrupt" fn divide_by_zero_handler(stack_frame: InterruptStackFrame) {
    crate::log_error!("[CPU EXCEPTION #DE] Divide-by-Zero Exception occurred!");
    crate::log_error!("Stack Frame: {}", stack_frame);
    crate::log_error!("Halting CPU to prevent undefined behavior.");
    crate::hcf();
}

pub extern "x86-interrupt" fn invalid_opcode_handler(stack_frame: InterruptStackFrame) {
    crate::log_error!("[CPU EXCEPTION #UD] Invalid Opcode Exception occurred!");
    crate::log_error!("Stack Frame: {}", stack_frame);
    crate::log_error!("Halting CPU to prevent undefined behavior.");
    crate::hcf();
}

pub extern "x86-interrupt" fn double_fault_handler(
    stack_frame: InterruptStackFrame,
    error_code: u64,
) -> ! {
    crate::log_error!("[CPU EXCEPTION #DF] Double Fault occurred! Error Code: {:#x}", error_code);
    crate::log_error!("Stack Frame: {}", stack_frame);
    crate::log_error!("Halting CPU.");
    crate::hcf();
}

pub extern "x86-interrupt" fn general_protection_handler(
    stack_frame: InterruptStackFrame,
    error_code: u64,
) {
    crate::log_error!("[CPU EXCEPTION #GP] General Protection Fault! Error Code: {:#x}", error_code);
    crate::log_error!("Stack Frame: {}", stack_frame);
    crate::log_error!("Halting CPU.");
    crate::hcf();
}

pub fn init() {
    unsafe {
        IDT.entries[0].set_handler(divide_by_zero_handler as *const () as usize);
        IDT.entries[6].set_handler(invalid_opcode_handler as *const () as usize);
        IDT.entries[8].set_handler(double_fault_handler as *const () as usize);
        IDT.entries[13].set_handler(general_protection_handler as *const () as usize);

        let idt_ptr = IdtPointer {
            limit: (core::mem::size_of::<Idt>() - 1) as u16,
            base: (&raw const IDT) as u64,
        };

        asm!("lidt [{}]", in(reg) &idt_ptr, options(readonly, nostack, preserves_flags));
    }

    crate::log_info!("Interrupt Descriptor Table (IDT) loaded with core exception handlers (#DE, #UD, #DF, #GP).");
}
