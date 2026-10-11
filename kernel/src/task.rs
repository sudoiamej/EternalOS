use core::arch::asm;
use core::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use crate::mm::allocate_frame;
use crate::vm::{map_page, PageTableFlags, VirtAddr, PAGE_SIZE};

pub const STACK_PAGES: usize = 4; // 16 KiB kernel stack per task
pub const STACK_SIZE: usize = STACK_PAGES * PAGE_SIZE;
const TASK_STACKS_BASE: u64 = 0xffff_a000_0000_0000;
pub const MAX_TASKS: usize = 16;

static NEXT_TASK_ID: AtomicU64 = AtomicU64::new(1);
pub static SCHEDULER_LOCK: AtomicBool = AtomicBool::new(false);
pub static PREEMPTION_COUNT: AtomicU64 = AtomicU64::new(0);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TaskState {
    Ready,
    Running,
    Blocked,
    Sleeping(u64), // Wake deadline in timer ticks
    Terminated,    // Exited; waiting for stack & resource reclamation
    Dead,          // Resources reclaimed; slot available for reuse
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
        // First look for completely empty None slots
        for slot in self.tasks.iter_mut() {
            if slot.is_none() {
                let id = task.id;
                *slot = Some(task);
                return Ok(id);
            }
        }
        // If no None slots, look for Dead slots whose resources have already been reclaimed
        for slot in self.tasks.iter_mut() {
            if let Some(t) = slot {
                if t.state == TaskState::Dead {
                    let id = task.id;
                    *slot = Some(task);
                    return Ok(id);
                }
            }
        }
        Err("Scheduler task table full")
    }

    pub fn has_runnable_workers(&self) -> bool {
        let mut count = 0;
        for t in self.tasks.iter().flatten() {
            if (t.state == TaskState::Ready || t.state == TaskState::Running) && t.id != 0 {
                count += 1;
            }
        }
        count > 0
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

    pub fn block_current(&mut self) {
        if let Some(ref mut t) = self.tasks[self.current] {
            t.state = TaskState::Blocked;
        }
        self.schedule_next();
    }

    pub fn sleep_current(&mut self, deadline: u64) {
        if let Some(ref mut t) = self.tasks[self.current] {
            t.state = TaskState::Sleeping(deadline);
        }
        self.schedule_next();
    }

    pub fn unblock(&mut self, task_id: u64) -> bool {
        for slot in self.tasks.iter_mut() {
            if let Some(t) = slot {
                if t.id == task_id {
                    if t.state == TaskState::Blocked {
                        t.state = TaskState::Ready;
                        return true;
                    }
                    return false;
                }
            }
        }
        false
    }

    /// Audit scheduler and task invariants. Returns Ok(()) or an error string describing invariant violation.
    pub fn verify_invariants(&self) -> Result<(), &'static str> {
        let mut running_count = 0;
        let mut ids: [u64; MAX_TASKS] = [0; MAX_TASKS];
        let mut id_count = 0;

        for (idx, slot) in self.tasks.iter().enumerate() {
            if let Some(t) = slot {
                // Invariant 1: Task ID must be unique
                for i in 0..id_count {
                    if ids[i] == t.id {
                        return Err("Scheduler invariant violated: duplicate task ID detected");
                    }
                }
                ids[id_count] = t.id;
                id_count += 1;

                // Invariant 2: Current index must match running task if state is Running
                if t.state == TaskState::Running {
                    running_count += 1;
                    if idx != self.current {
                        return Err("Scheduler invariant violated: task marked Running is not at current scheduler index");
                    }
                }

                // Invariant 3: Stack pointers must be canonical and properly bounded if task has a stack (id != 0)
                if t.id != 0 {
                    if !t.stack_bottom.is_canonical() || !t.stack_top.is_canonical() {
                        return Err("Scheduler invariant violated: non-canonical stack pointer in task");
                    }
                    if t.stack_bottom.as_u64() >= t.stack_top.as_u64() {
                        return Err("Scheduler invariant violated: stack_bottom >= stack_top");
                    }
                }
            }
        }

        // Invariant 4: No more than 1 task may be in Running state simultaneously on single-core BSP
        if running_count > 1 {
            return Err("Scheduler invariant violated: multiple tasks simultaneously marked Running");
        }

        Ok(())
    }

    /// Expose tasks slice for read-only diagnostics/tests
    pub fn task_at(&self, idx: usize) -> Option<&Task> {
        if idx < MAX_TASKS {
            self.tasks[idx].as_ref()
        } else {
            None
        }
    }

    /// Expose mutable task reference for controlled invariant violation testing
    #[cfg(feature = "test-reliability")]
    pub unsafe fn task_at_mut(&mut self, idx: usize) -> Option<&mut Task> {
        if idx < MAX_TASKS {
            self.tasks[idx].as_mut()
        } else {
            None
        }
    }

    /// Wake up any sleeping tasks whose deadlines have arrived.
    pub fn check_sleeping_tasks(&mut self, current_ticks: u64) -> usize {
        let mut woken = 0;
        for slot in self.tasks.iter_mut() {
            if let Some(t) = slot {
                if let TaskState::Sleeping(deadline) = t.state {
                    if current_ticks >= deadline {
                        t.state = TaskState::Ready;
                        woken += 1;
                    }
                }
            }
        }
        woken
    }

    /// Safely reclaim stacks and memory for terminated tasks (excluding the currently running task).
    pub fn reclaim_terminated_tasks(&mut self) -> usize {
        let mut reclaimed = 0;
        for idx in 0..MAX_TASKS {
            if idx == self.current {
                // Never free the currently executing task's stack!
                continue;
            }

            let mut should_reclaim = false;
            let mut stack_base = 0u64;

            if let Some(ref t) = self.tasks[idx] {
                if t.state == TaskState::Terminated && t.id != 0 {
                    should_reclaim = true;
                    stack_base = t.stack_bottom.as_u64();
                }
            }

            if should_reclaim {
                // Unmap each 4KiB page and deallocate the physical frame back to the PMM
                for page in 0..STACK_PAGES {
                    let vaddr = VirtAddr::new(stack_base + (page * PAGE_SIZE) as u64);
                    unsafe {
                        if let Ok(pframe) = crate::vm::unmap_page(vaddr) {
                            let _ = crate::mm::deallocate_frame(pframe);
                        }
                    }
                }

                if let Some(ref mut t) = self.tasks[idx] {
                    t.state = TaskState::Dead;
                }
                reclaimed += 1;
            }
        }
        reclaimed
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
        // Re-enable interrupts so the task can be preempted by the timer
        asm!("sti", options(nomem, nostack, preserves_flags));
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

pub fn current_task_id() -> u64 {
    unsafe {
        #[allow(static_mut_refs)]
        SCHEDULER.current_task_id()
    }
}

pub fn yield_now() {
    // Disable interrupts to avoid preemption while initiating a cooperative yield
    unsafe { asm!("cli", options(nomem, nostack, preserves_flags)) };

    while SCHEDULER_LOCK.compare_exchange_weak(false, true, Ordering::Acquire, Ordering::Relaxed).is_err() {
        core::hint::spin_loop();
    }

    unsafe {
        #[allow(static_mut_refs)]
        SCHEDULER.schedule_next();
    }

    SCHEDULER_LOCK.store(false, Ordering::Release);

    unsafe { asm!("sti", options(nomem, nostack, preserves_flags)) };
}

pub fn block_current_task() {
    unsafe { asm!("cli", options(nomem, nostack, preserves_flags)) };

    while SCHEDULER_LOCK.compare_exchange_weak(false, true, Ordering::Acquire, Ordering::Relaxed).is_err() {
        core::hint::spin_loop();
    }

    unsafe {
        #[allow(static_mut_refs)]
        SCHEDULER.block_current();
    }

    SCHEDULER_LOCK.store(false, Ordering::Release);

    unsafe { asm!("sti", options(nomem, nostack, preserves_flags)) };
}

/// Put current task to sleep until the given number of ticks has passed.
/// If ticks == 0, voluntarily yields CPU and returns immediately.
pub fn sleep_ticks(ticks: u64) {
    if ticks == 0 {
        yield_now();
        return;
    }

    let start_tick = crate::timer::get_ticks();
    // Checked add prevents overflow when calculating sleep deadline
    let deadline = start_tick.checked_add(ticks).unwrap_or(u64::MAX);

    loop {
        if crate::timer::get_ticks() >= deadline {
            break;
        }

        unsafe { asm!("cli", options(nomem, nostack, preserves_flags)) };

        while SCHEDULER_LOCK.compare_exchange_weak(false, true, Ordering::Acquire, Ordering::Relaxed).is_err() {
            core::hint::spin_loop();
        }

        unsafe {
            #[allow(static_mut_refs)]
            SCHEDULER.sleep_current(deadline);
        }

        SCHEDULER_LOCK.store(false, Ordering::Release);

        // If schedule_next returned immediately because no other task is runnable,
        // safely halt the CPU with interrupts enabled so the timer interrupt can fire
        // and advance ticks until our deadline expires.
        unsafe { asm!("sti; hlt", options(nomem, nostack, preserves_flags)) };
    }
}

/// Put current task to sleep for the specified duration in milliseconds.
pub fn sleep_ms(ms: u64) {
    let ticks = crate::timer::ms_to_ticks(ms);
    sleep_ticks(ticks);
}

/// Reclaim stack and page table resources of terminated tasks safely.
pub fn reclaim_tasks() -> usize {
    unsafe { asm!("cli", options(nomem, nostack, preserves_flags)) };

    while SCHEDULER_LOCK.compare_exchange_weak(false, true, Ordering::Acquire, Ordering::Relaxed).is_err() {
        core::hint::spin_loop();
    }

    let count = unsafe {
        #[allow(static_mut_refs)]
        SCHEDULER.reclaim_terminated_tasks()
    };

    SCHEDULER_LOCK.store(false, Ordering::Release);

    unsafe { asm!("sti", options(nomem, nostack, preserves_flags)) };

    count
}

pub fn wake_task(task_id: u64) -> bool {
    unsafe { asm!("cli", options(nomem, nostack, preserves_flags)) };

    while SCHEDULER_LOCK.compare_exchange_weak(false, true, Ordering::Acquire, Ordering::Relaxed).is_err() {
        core::hint::spin_loop();
    }

    let woken = unsafe {
        #[allow(static_mut_refs)]
        SCHEDULER.unblock(task_id)
    };

    SCHEDULER_LOCK.store(false, Ordering::Release);

    unsafe { asm!("sti", options(nomem, nostack, preserves_flags)) };

    woken
}

pub fn terminate_task() -> ! {
    unsafe { asm!("cli", options(nomem, nostack, preserves_flags)) };

    while SCHEDULER_LOCK.compare_exchange_weak(false, true, Ordering::Acquire, Ordering::Relaxed).is_err() {
        core::hint::spin_loop();
    }

    unsafe {
        #[allow(static_mut_refs)]
        SCHEDULER.terminate_current();
    }

    SCHEDULER_LOCK.store(false, Ordering::Release);

    // If no more tasks, idle in halt loop with interrupts enabled
    loop {
        unsafe { asm!("sti; hlt", options(nomem, nostack, preserves_flags)) };
    }
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
    crate::log_info!("Initializing Kernel Tasking & Cooperative/Preemptive Scheduler...");
    crate::log_info!("  Scheduler Policy : Round-Robin with Hardware Timer Time-Slicing");
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
