use core::sync::atomic::{AtomicU64, Ordering};
use crate::mm::{allocate_frame, deallocate_frame};
use crate::vm::{
    create_user_address_space, destroy_user_address_space, map_page_in_table, Cr3, PageTableFlags,
    PhysAddr, VirtAddr, PAGE_SIZE,
};
use crate::gdt::{set_tss_rsp0, USER_CS, USER_DS};
use crate::sync::SpinLock;
use crate::elf::load_elf64;

pub const MAX_PROCESSES: usize = 16;
pub const USER_STACK_TOP: u64 = 0x0000_7FFF_FFFF_0000;
pub const USER_STACK_PAGES: usize = 4; // 16 KiB user stack
pub const KERNEL_STACK_PAGES: usize = 4; // 16 KiB kernel stack per process

static NEXT_PID: AtomicU64 = AtomicU64::new(1);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProcessState {
    Unused,
    Ready,
    Running,
    Blocked,
    Terminated(i32),
}

/// Saved general-purpose user registers upon entry to kernel mode via `int 0x80` or exception
#[repr(C)]
#[derive(Debug, Clone, Copy, Default)]
pub struct UserTrapFrame {
    // Callee/Caller saved registers preserved on stack
    pub r15: u64,
    pub r14: u64,
    pub r13: u64,
    pub r12: u64,
    pub r11: u64,
    pub r10: u64,
    pub r9: u64,
    pub r8: u64,
    pub rbp: u64,
    pub rdi: u64,
    pub rsi: u64,
    pub rdx: u64,
    pub rcx: u64,
    pub rbx: u64,
    pub rax: u64,

    // Hardware pushed interrupt stack frame by CPU on transition
    pub rip: u64,
    pub cs: u64,
    pub rflags: u64,
    pub rsp: u64,
    pub ss: u64,
}

pub struct Process {
    pub pid: u64,
    pub name: [u8; 32],
    pub state: ProcessState,
    pub pml4_phys: PhysAddr,
    pub kernel_stack_bottom: VirtAddr,
    pub kernel_stack_top: VirtAddr,
    pub user_stack_top: VirtAddr,
    pub trap_frame: UserTrapFrame,
    pub exit_code: i32,
    pub cwd: [u8; 128],
    pub cwd_len: usize,
}

impl Process {
    pub const fn empty() -> Self {
        Self {
            pid: 0,
            name: [0; 32],
            state: ProcessState::Unused,
            pml4_phys: PhysAddr(0),
            kernel_stack_bottom: VirtAddr(0),
            kernel_stack_top: VirtAddr(0),
            user_stack_top: VirtAddr(0),
            trap_frame: UserTrapFrame {
                r15: 0, r14: 0, r13: 0, r12: 0, r11: 0, r10: 0, r9: 0, r8: 0,
                rbp: 0, rdi: 0, rsi: 0, rdx: 0, rcx: 0, rbx: 0, rax: 0,
                rip: 0, cs: 0, rflags: 0, rsp: 0, ss: 0,
            },
            exit_code: 0,
            cwd: [0; 128],
            cwd_len: 0,
        }
    }
}

pub struct ProcessManager {
    pub processes: [Process; MAX_PROCESSES],
    pub current: Option<usize>,
}

impl ProcessManager {
    pub const fn new() -> Self {
        Self {
            processes: [const { Process::empty() }; MAX_PROCESSES],
            current: None,
        }
    }

    /// Allocate a new process slot
    fn allocate_slot(&mut self) -> Option<usize> {
        for (i, p) in self.processes.iter_mut().enumerate() {
            if p.state == ProcessState::Unused {
                return Some(i);
            }
        }
        None
    }

    /// Spawn a new user process from raw ELF64 executable bytes
    pub fn spawn_elf(&mut self, name: &str, elf_bytes: &[u8]) -> Result<u64, &'static str> {
        let slot = self.allocate_slot().ok_or("Process table overflow")?;
        let pid = NEXT_PID.fetch_add(1, Ordering::Relaxed);

        // 1. Create independent user PML4 address space
        let user_pml4 = create_user_address_space()?;

        // 2. Load ELF into the user PML4
        let loaded = match load_elf64(elf_bytes, user_pml4) {
            Ok(l) => l,
            Err(e) => {
                unsafe { let _ = destroy_user_address_space(user_pml4); };
                return Err(e);
            }
        };

        // 3. Allocate and map user stack in the user PML4
        let stack_bottom = USER_STACK_TOP - (USER_STACK_PAGES * PAGE_SIZE) as u64;
        for i in 0..USER_STACK_PAGES {
            let pframe = allocate_frame().ok_or("PMM out of memory for user stack")?;
            let flags = PageTableFlags::PRESENT | PageTableFlags::WRITABLE | PageTableFlags::USER_ACCESSIBLE;
            let vaddr = VirtAddr::new(stack_bottom + (i * PAGE_SIZE) as u64);
            unsafe {
                map_page_in_table(user_pml4, vaddr, pframe, flags)?;
            }
        }

        // 4. Allocate kernel stack for user->kernel privilege transitions
        // 4 contiguous frames mapped in higher-half direct map (HHDM)
        let kstack_frame = allocate_frame().ok_or("PMM out of memory for kernel stack")?;
        let kstack_virt = crate::vm::phys_to_virt(kstack_frame).ok_or("HHDM not available")?;
        let kstack_top = VirtAddr::new(kstack_virt.as_u64() + PAGE_SIZE as u64);

        // 5. Initialize TrapFrame for user mode entry
        let mut tf = UserTrapFrame::default();
        tf.rip = loaded.entry_point;
        tf.cs = USER_CS as u64; // DPL 3
        tf.rflags = 0x202; // IF=1 (interrupts enabled), reserved bit 1 = 1
        tf.rsp = USER_STACK_TOP - 16; // 16-byte aligned user stack pointer
        tf.ss = USER_DS as u64; // RPL 3

        let proc = &mut self.processes[slot];
        proc.pid = pid;
        let mut name_buf = [0u8; 32];
        let bytes = name.as_bytes();
        let len = bytes.len().min(31);
        name_buf[..len].copy_from_slice(&bytes[..len]);
        proc.name = name_buf;
        proc.state = ProcessState::Ready;
        proc.pml4_phys = user_pml4;
        proc.kernel_stack_bottom = kstack_virt;
        proc.kernel_stack_top = kstack_top;
        proc.user_stack_top = VirtAddr::new(USER_STACK_TOP);
        proc.trap_frame = tf;
        proc.exit_code = 0;
        proc.cwd[0] = b'/';
        proc.cwd_len = 1;

        Ok(pid)
    }

    pub fn current_process(&self) -> Option<&Process> {
        self.current.and_then(|idx| self.processes.get(idx))
    }

    pub fn current_process_mut(&mut self) -> Option<&mut Process> {
        self.current.and_then(|idx| self.processes.get_mut(idx))
    }

    /// Terminate process by PID and reclaim its address space
    pub fn terminate(&mut self, pid: u64, exit_code: i32) {
        for p in self.processes.iter_mut() {
            if p.pid == pid && p.state != ProcessState::Unused {
                p.state = ProcessState::Terminated(exit_code);
                p.exit_code = exit_code;

                // Reclaim user address space
                unsafe {
                    let _ = destroy_user_address_space(p.pml4_phys);
                }

                // Deallocate kernel stack frame
                if let Some(phys) = crate::vm::translate_addr(p.kernel_stack_bottom).ok() {
                    let _ = deallocate_frame(phys);
                }

                p.state = ProcessState::Unused;
                break;
            }
        }
    }
}

pub static PROCESS_MANAGER: SpinLock<ProcessManager> = SpinLock::new(ProcessManager::new());

/// Global helper to spawn user process from ELF64 binary
pub fn spawn_user_process(name: &str, elf_bytes: &[u8]) -> Result<u64, &'static str> {
    PROCESS_MANAGER.lock().spawn_elf(name, elf_bytes)
}

/// Execute user process at index directly, entering CPL3 via `iretq`
pub unsafe fn enter_user_mode(tf: &UserTrapFrame, user_pml4: PhysAddr, kernel_stack_top: u64) -> ! {
    // Set TSS.rsp0 so interrupts/syscalls originating from user mode land on this process's kernel stack
    set_tss_rsp0(kernel_stack_top);

    unsafe {
        // Switch to user address space (PML4)
        Cr3::write(Cr3 {
            pml4_phys: user_pml4,
            flags: 0,
        });

        // Jump to user mode using iretq
        core::arch::asm!(
            "mov ax, {user_ds:x}",
            "mov ds, ax",
            "mov es, ax",
            "mov fs, ax",
            "mov gs, ax",

            "push {ss}",
            "push {rsp}",
            "push {rflags}",
            "push {cs}",
            "push {rip}",

            "iretq",

            user_ds = in(reg) USER_DS,
            ss = in(reg) tf.ss,
            rsp = in(reg) tf.rsp,
            rflags = in(reg) tf.rflags,
            cs = in(reg) tf.cs,
            rip = in(reg) tf.rip,
            options(noreturn)
        );
    }
}
