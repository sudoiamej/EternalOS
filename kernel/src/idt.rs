use core::arch::asm;
use core::fmt;
use core::sync::atomic::Ordering;
use crate::task::{PREEMPTION_COUNT, SCHEDULER, SCHEDULER_LOCK};
use crate::timer::{send_eoi, TIMER_TICKS};

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
        let cs: u16;
        unsafe {
            core::arch::asm!("mov {0:x}, cs", out(reg) cs, options(nomem, nostack, preserves_flags));
        }
        self.pointer_low = handler as u16;
        self.gdt_selector = cs; // Use active Limine kernel code segment selector (0x28)
        self.options = 0x8E00;   // Present (bit 15), DPL 0, 64-bit Interrupt Gate (0xE)
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
    crate::log_fatal!("==================================================");
    crate::log_fatal!("[CPU EXCEPTION #DE] Divide-by-Zero Exception (Vector 0)");
    crate::log_fatal!("Stack Frame: {}", stack_frame);
    crate::log_fatal!("Halting CPU safely to prevent undefined behavior.");
    crate::log_fatal!("==================================================");
    crate::hcf();
}

pub extern "x86-interrupt" fn invalid_opcode_handler(stack_frame: InterruptStackFrame) {
    crate::log_fatal!("==================================================");
    crate::log_fatal!("[CPU EXCEPTION #UD] Invalid Opcode Exception (Vector 6)");
    crate::log_fatal!("Stack Frame: {}", stack_frame);
    crate::log_fatal!("Halting CPU safely to prevent undefined behavior.");
    crate::log_fatal!("==================================================");
    crate::hcf();
}

pub extern "x86-interrupt" fn double_fault_handler(
    stack_frame: InterruptStackFrame,
    error_code: u64,
) -> ! {
    crate::log_fatal!("==================================================");
    crate::log_fatal!("[CPU EXCEPTION #DF] Double Fault (Vector 8)!");
    crate::log_fatal!("Error Code : {:#x}", error_code);
    crate::log_fatal!("Stack Frame: {}", stack_frame);
    crate::log_fatal!("Halting CPU.");
    crate::log_fatal!("==================================================");
    crate::hcf();
}

pub extern "x86-interrupt" fn invalid_tss_handler(
    stack_frame: InterruptStackFrame,
    error_code: u64,
) {
    crate::log_fatal!("==================================================");
    crate::log_fatal!("[CPU EXCEPTION #TS] Invalid TSS Exception (Vector 10)!");
    crate::log_fatal!("Error Code : {:#x}", error_code);
    crate::log_fatal!("Stack Frame: {}", stack_frame);
    crate::log_fatal!("Halting CPU.");
    crate::log_fatal!("==================================================");
    crate::hcf();
}

pub extern "x86-interrupt" fn stack_segment_fault_handler(
    stack_frame: InterruptStackFrame,
    error_code: u64,
) {
    crate::log_fatal!("==================================================");
    crate::log_fatal!("[CPU EXCEPTION #SS] Stack-Segment Fault (Vector 12)!");
    crate::log_fatal!("Error Code : {:#x}", error_code);
    crate::log_fatal!("Stack Frame: {}", stack_frame);
    crate::log_fatal!("Halting CPU.");
    crate::log_fatal!("==================================================");
    crate::hcf();
}

pub extern "x86-interrupt" fn general_protection_handler(
    stack_frame: InterruptStackFrame,
    error_code: u64,
) {
    crate::log_fatal!("==================================================");
    crate::log_fatal!("[CPU EXCEPTION #GP] General Protection Fault (Vector 13)!");
    crate::log_fatal!("Error Code : {:#x}", error_code);
    crate::log_fatal!("Stack Frame: {}", stack_frame);
    crate::log_fatal!("Halting CPU.");
    crate::log_fatal!("==================================================");
    crate::hcf();
}

pub extern "x86-interrupt" fn page_fault_handler(
    stack_frame: InterruptStackFrame,
    error_code: u64,
) {
    let cr2: u64;
    unsafe {
        asm!("mov {}, cr2", out(reg) cr2, options(nomem, nostack, preserves_flags));
    }

    let p = (error_code & (1 << 0)) != 0;
    let wr = (error_code & (1 << 1)) != 0;
    let us = (error_code & (1 << 2)) != 0;
    let rsvd = (error_code & (1 << 3)) != 0;
    let id = (error_code & (1 << 4)) != 0;

    crate::log_fatal!("==================================================");
    crate::log_fatal!("[CPU EXCEPTION #PF] Page Fault (Vector 14)!");
    crate::log_fatal!("Faulting Virtual Address (CR2): {:#018x}", cr2);
    crate::log_fatal!(
        "Error Code: {:#x} [Present={}, Write={}, User={}, ReservedBit={}, InstructionFetch={}]",
        error_code, p, wr, us, rsvd, id
    );
    crate::log_fatal!("Stack Frame: {}", stack_frame);
    crate::log_fatal!("Halting CPU.");
    crate::log_fatal!("==================================================");
    crate::hcf();
}

/// Hardware Timer Interrupt Handler (IRQ0 -> Vector 32)
pub extern "x86-interrupt" fn timer_interrupt_handler(_stack_frame: InterruptStackFrame) {
    let current_ticks = TIMER_TICKS.fetch_add(1, Ordering::Relaxed) + 1;
    send_eoi();

    // If preemption is possible and scheduler is unlocked, record preemption event and wake sleeping tasks
    if SCHEDULER_LOCK.compare_exchange_weak(false, true, Ordering::Acquire, Ordering::Relaxed).is_ok() {
        PREEMPTION_COUNT.fetch_add(1, Ordering::Relaxed);

        unsafe {
            #[allow(static_mut_refs)]
            {
                // 1. Wake any sleeping tasks whose deadlines have arrived
                SCHEDULER.check_sleeping_tasks(current_ticks);

                // 2. Schedule next runnable task if runnable workers exist
                if SCHEDULER.has_runnable_workers() {
                    SCHEDULER.schedule_next();
                }
            }
        }

        SCHEDULER_LOCK.store(false, Ordering::Release);
    }
}

pub fn init() {
    unsafe {
        IDT.entries[0].set_handler(divide_by_zero_handler as *const () as usize);
        IDT.entries[6].set_handler(invalid_opcode_handler as *const () as usize);
        IDT.entries[8].set_handler(double_fault_handler as *const () as usize);
        IDT.entries[10].set_handler(invalid_tss_handler as *const () as usize);
        IDT.entries[12].set_handler(stack_segment_fault_handler as *const () as usize);
        IDT.entries[13].set_handler(general_protection_handler as *const () as usize);
        IDT.entries[14].set_handler(page_fault_handler as *const () as usize);
        IDT.entries[32].set_handler(timer_interrupt_handler as *const () as usize);

        let idt_ptr = IdtPointer {
            limit: (core::mem::size_of::<Idt>() - 1) as u16,
            base: (&raw const IDT) as u64,
        };

        asm!("lidt [{}]", in(reg) &idt_ptr, options(readonly, nostack, preserves_flags));
    }

    crate::log_info!("Interrupt Descriptor Table (IDT) loaded with enhanced CPU exceptions and Timer IRQ0.");
}
