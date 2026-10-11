use core::arch::asm;
use core::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use crate::mm::allocate_frame;
use crate::vm::{map_page, PageTableFlags, VirtAddr, PAGE_SIZE};

pub const STACK_PAGES: usize = 4; // 16 KiB kernel stack per task
pub const STACK_SIZE: usize = STACK_PAGES * PAGE_SIZE;
const TASK_STACKS_BASE: u64 = 0xffff_a000_0000_0000;
pub const MAX_TASKS: usize = 16;

static NEXT_TASK_ID: AtomicU64 = AtomicU64::new(1);
static SCHEDULER_LOCK: AtomicBool = AtomicBool::new(false);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TaskState {
    Ready,
    Running,
    Terminated,
}

#[derive(Debug, Clone, Copy)]
#[repr(C)]
pub struct Context {
    pub r15: u64,
    pub r14: u64,
    pub r13: u64,
    pub r12: u64,
    pub rbx: u64,
    pub rbp: u64,
    pub rsp: u64,
    pub rip: u64,
}

impl Context {
    pub const fn empty() -> Self {
        Self {
            r15: 0,
            r14: 0,
            r13: 0,
            r12: 0,
            rbx: 0,
            rbp: 0,
            rsp: 0,
            rip: 0,
        }
    }
}

pub struct Task {
    pub id: u64,
    pub name: &'static str,
    pub state: TaskState,
    pub context: Context,
    pub stack_bottom: VirtAddr,
    pub stack_top: VirtAddr,
}

impl Task {
    pub fn new(id: u64, name: &'static str, stack_bottom: VirtAddr, stack_top: VirtAddr, entry: extern "C" fn()) -> Self {
        let mut context = Context::empty();
        // Setup stack pointer ensuring 16-byte alignment
        let rsp = (stack_top.as_u64() - 16) & !15;
        context.rsp = rsp;
        context.rip = task_entry_trampoline as *const () as usize as u64;
        context.r12 = entry as *const () as usize as u64; // Pass entry function pointer in callee-saved r12

        Self {
            id,
            name,
            state: TaskState::Ready,
            context,
            stack_bottom,
            stack_top,
        }
    }
}

pub struct Scheduler {
    tasks: [Option<Task>; MAX_TASKS],
    current: usize,
}

impl Scheduler {
    pub const fn new() -> Self {
        const INIT_TASK: Option<Task> = None;
        Self {
            tasks: [INIT_TASK; MAX_TASKS],
            current: 0,
        }
    }

    pub fn add_task(&mut self, task: Task) -> Result<u64, &'static str> {
        for slot in self.tasks.iter_mut() {
            if slot.is_none() {
                let id = task.id;
                *slot = Some(task);
                return Ok(id);
            }
        }
        Err("Scheduler task table full")
    }

    pub fn current_task_id(&self) -> u64 {
        if let Some(ref t) = self.tasks[self.current] {
            t.id
        } else {
            0
        }
    }

    pub fn terminate_current(&mut self) {
        if let Some(ref mut t) = self.tasks[self.current] {
            crate::log_info!("[SCHED] Task {} ('{}') has terminated.", t.id, t.name);
            t.state = TaskState::Terminated;
        }
        self.schedule_next();
    }

    pub fn schedule_next(&mut self) {
        let prev_idx = self.current;
        let mut next_idx = (self.current + 1) % MAX_TASKS;
        let mut found = false;

        for _ in 0..MAX_TASKS {
            if let Some(ref t) = self.tasks[next_idx] {
                if t.state == TaskState::Ready || t.state == TaskState::Running {
                    found = true;
                    break;
                }
            }
            next_idx = (next_idx + 1) % MAX_TASKS;
        }

        if !found {
            // Check if current is still running/ready
            if let Some(ref t) = self.tasks[prev_idx] {
                if t.state == TaskState::Ready || t.state == TaskState::Running {
                    return; // Keep running current task
                }
            }
            return; // No runnable tasks left
        }

        if prev_idx == next_idx {
            return; // Only 1 runnable task, continue executing
        }

        self.current = next_idx;

        if let Some(ref mut prev_task) = self.tasks[prev_idx] {
            if prev_task.state == TaskState::Running {
                prev_task.state = TaskState::Ready;
            }
        }

        if let Some(ref mut next_task) = self.tasks[next_idx] {
            next_task.state = TaskState::Running;
        }

        // Get pointers to previous and next contexts for context switch
        let prev_ctx_ptr = match self.tasks[prev_idx] {
            Some(ref mut t) => &raw mut t.context,
            None => return,
        };

        let next_ctx_ptr = match self.tasks[next_idx] {
            Some(ref t) => &raw const t.context,
            None => return,
        };

        // Release scheduler spinlock before switching execution to the next task
        SCHEDULER_LOCK.store(false, Ordering::Release);

        unsafe {
            x86_64_switch_context(prev_ctx_ptr, next_ctx_ptr);
        }

        // Re-acquire scheduler spinlock upon resumption
        while SCHEDULER_LOCK.compare_exchange_weak(false, true, Ordering::Acquire, Ordering::Relaxed).is_err() {
            core::hint::spin_loop();
        }
    }
}

pub static mut SCHEDULER: Scheduler = Scheduler::new();

#[unsafe(no_mangle)]
pub extern "C" fn task_entry_trampoline() {
    let entry_fn: extern "C" fn();
    unsafe {
        asm!("mov {}, r12", out(reg) entry_fn, options(nomem, nostack, preserves_flags));
    }

    // Call the actual task entry function
    entry_fn();

    // When the task function returns, terminate cleanly through the scheduler
    terminate_task();
}

#[unsafe(naked)]
#[unsafe(no_mangle)]
pub unsafe extern "C" fn x86_64_switch_context(prev: *mut Context, next: *const Context) {
    core::arch::naked_asm!(
            // Save callee-saved registers to prev context
            "mov [rdi + 0], r15",
            "mov [rdi + 8], r14",
            "mov [rdi + 16], r13",
            "mov [rdi + 24], r12",
            "mov [rdi + 32], rbx",
            "mov [rdi + 40], rbp",
            "mov [rdi + 48], rsp",
            "lea rax, [rip + 1f]",
            "mov [rdi + 56], rax",

            // Restore callee-saved registers from next context
            "mov r15, [rsi + 0]",
            "mov r14, [rsi + 8]",
            "mov r13, [rsi + 16]",
            "mov r12, [rsi + 24]",
            "mov rbx, [rsi + 32]",
            "mov rbp, [rsi + 40]",
            "mov rsp, [rsi + 48]",
            "mov rax, [rsi + 56]",
            "jmp rax",
            "1:",
            "ret"
        );
}

pub fn yield_now() {
    while SCHEDULER_LOCK.compare_exchange_weak(false, true, Ordering::Acquire, Ordering::Relaxed).is_err() {
        core::hint::spin_loop();
    }

    unsafe {
        #[allow(static_mut_refs)]
        SCHEDULER.schedule_next();
    }

    SCHEDULER_LOCK.store(false, Ordering::Release);
}

pub fn terminate_task() -> ! {
    while SCHEDULER_LOCK.compare_exchange_weak(false, true, Ordering::Acquire, Ordering::Relaxed).is_err() {
        core::hint::spin_loop();
    }

    unsafe {
        #[allow(static_mut_refs)]
        SCHEDULER.terminate_current();
    }

    SCHEDULER_LOCK.store(false, Ordering::Release);

    // If no more tasks, idle in halt loop
    crate::hcf();
}

pub fn spawn_task(name: &'static str, entry: extern "C" fn()) -> Result<u64, &'static str> {
    let id = NEXT_TASK_ID.fetch_add(1, Ordering::Relaxed);
    let stack_base = TASK_STACKS_BASE + (id * (STACK_SIZE as u64 + PAGE_SIZE as u64));

    // Allocate and map 4 physical pages for the task's stack
    for page in 0..STACK_PAGES {
        let vaddr = VirtAddr::new(stack_base + (page * PAGE_SIZE) as u64);
        let pframe = allocate_frame().ok_or("PMM failed to allocate frame for task stack")?;
        let flags = PageTableFlags::PRESENT | PageTableFlags::WRITABLE | PageTableFlags::NO_EXECUTE;
        unsafe {
            map_page(vaddr, pframe, flags)?;
        }
    }

    let stack_bottom = VirtAddr::new(stack_base);
    let stack_top = VirtAddr::new(stack_base + STACK_SIZE as u64);

    let task = Task::new(id, name, stack_bottom, stack_top, entry);

    while SCHEDULER_LOCK.compare_exchange_weak(false, true, Ordering::Acquire, Ordering::Relaxed).is_err() {
        core::hint::spin_loop();
    }

    let res = unsafe {
        #[allow(static_mut_refs)]
        SCHEDULER.add_task(task)
    };

    SCHEDULER_LOCK.store(false, Ordering::Release);

    crate::log_info!("[TASK] Spawned Task {} ('{}') with stack {:#018x}..{:#018x}", id, name, stack_bottom.as_u64(), stack_top.as_u64());
    res
}

pub fn init_tasking() {
    crate::log_info!("Initializing Kernel Tasking & Cooperative Scheduler...");
    crate::log_info!("  Scheduler Policy : Cooperative Round-Robin");
    crate::log_info!("  Task Stack Size  : {} KiB per task (guard-page spaced)", STACK_SIZE / 1024);
    crate::log_info!("  Max Concurrency  : {} kernel tasks", MAX_TASKS);

    // Setup Main / Kernel task as Task 0
    let main_task = Task {
        id: 0,
        name: "kernel_main",
        state: TaskState::Running,
        context: Context::empty(),
        stack_bottom: VirtAddr::new(0),
        stack_top: VirtAddr::new(0),
    };

    unsafe {
        #[allow(static_mut_refs)]
        let _ = SCHEDULER.add_task(main_task);
    }

    crate::log_info!("Kernel Tasking subsystem initialized (Main Task registered as Task 0).");
}
