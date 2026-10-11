use core::cell::UnsafeCell;
use core::ops::{Deref, DerefMut};
use core::sync::atomic::{AtomicBool, AtomicIsize, AtomicU64, Ordering};
use crate::task::{block_current_task, current_task_id, wake_task, MAX_TASKS};

/// A bounded FIFO wait queue of task IDs for task blocking and wakeups.
pub struct WaitQueue {
    waiters: [u64; MAX_TASKS],
    head: usize,
    tail: usize,
    count: usize,
}

impl WaitQueue {
    pub const fn new() -> Self {
        Self {
            waiters: [0; MAX_TASKS],
            head: 0,
            tail: 0,
            count: 0,
        }
    }

    /// Enqueue a task ID into the wait queue. Returns true if queued, false if full.
    pub fn enqueue(&mut self, task_id: u64) -> bool {
        if self.count >= MAX_TASKS {
            return false;
        }
        self.waiters[self.tail] = task_id;
        self.tail = (self.tail + 1) % MAX_TASKS;
        self.count += 1;
        true
    }

    /// Dequeue the next waiting task ID.
    pub fn dequeue(&mut self) -> Option<u64> {
        if self.count == 0 {
            return None;
        }
        let task_id = self.waiters[self.head];
        self.head = (self.head + 1) % MAX_TASKS;
        self.count -= 1;
        Some(task_id)
    }

    pub fn is_empty(&self) -> bool {
        self.count == 0
    }

    pub fn len(&self) -> usize {
        self.count
    }

    pub fn contains(&self, task_id: u64) -> bool {
        for i in 0..self.count {
            let idx = (self.head + i) % MAX_TASKS;
            if self.waiters[idx] == task_id {
                return true;
            }
        }
        false
    }

    pub fn verify_invariants(&self) -> Result<(), &'static str> {
        if self.count > MAX_TASKS {
            return Err("WaitQueue invariant violated: count exceeds MAX_TASKS capacity");
        }
        if self.head >= MAX_TASKS || self.tail >= MAX_TASKS {
            return Err("WaitQueue invariant violated: head or tail pointer out of bounds");
        }
        let computed_tail = (self.head + self.count) % MAX_TASKS;
        if computed_tail != self.tail {
            return Err("WaitQueue invariant violated: tail does not match (head + count) % MAX_TASKS");
        }
        Ok(())
    }
}

/// A kernel spinlock used solely to protect synchronization metadata (WaitQueues)
/// in short, non-blocking critical sections with interrupts disabled.
pub struct SpinLock<T> {
    lock: AtomicBool,
    data: UnsafeCell<T>,
}

unsafe impl<T: Send> Sync for SpinLock<T> {}
unsafe impl<T: Send> Send for SpinLock<T> {}

impl<T> SpinLock<T> {
    pub const fn new(data: T) -> Self {
        Self {
            lock: AtomicBool::new(false),
            data: UnsafeCell::new(data),
        }
    }

    /// Acquire the spinlock. Disables CPU interrupts to prevent ISR deadlocks on single-core BSP.
    #[inline]
    pub fn lock(&self) -> SpinLockGuard<'_, T> {
        // Disable interrupts while holding this spinlock
        unsafe {
            core::arch::asm!("cli", options(nomem, nostack, preserves_flags));
        }

        while self.lock.compare_exchange_weak(false, true, Ordering::Acquire, Ordering::Relaxed).is_err() {
            core::hint::spin_loop();
        }

        SpinLockGuard { parent: self }
    }
}

pub struct SpinLockGuard<'a, T> {
    parent: &'a SpinLock<T>,
}

impl<T> Deref for SpinLockGuard<'_, T> {
    type Target = T;
    fn deref(&self) -> &Self::Target {
        unsafe { &*self.parent.data.get() }
    }
}

impl<T> DerefMut for SpinLockGuard<'_, T> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        unsafe { &mut *self.parent.data.get() }
    }
}

impl<T> Drop for SpinLockGuard<'_, T> {
    fn drop(&mut self) {
        self.parent.lock.store(false, Ordering::Release);
        unsafe {
            core::arch::asm!("sti", options(nomem, nostack, preserves_flags));
        }
    }
}

pub const NO_OWNER: u64 = u64::MAX;

/// A blocking Kernel Mutex providing mutual exclusion with task sleep/wakeup.
pub struct Mutex<T> {
    owner: AtomicU64, // NO_OWNER = unlocked, 0..=MAX = task ID owning the mutex
    wait_queue: SpinLock<WaitQueue>,
    data: UnsafeCell<T>,
}

unsafe impl<T: Send> Sync for Mutex<T> {}
unsafe impl<T: Send> Send for Mutex<T> {}

impl<T> Mutex<T> {
    pub const fn new(data: T) -> Self {
        Self {
            owner: AtomicU64::new(NO_OWNER),
            wait_queue: SpinLock::new(WaitQueue::new()),
            data: UnsafeCell::new(data),
        }
    }

    /// Try to acquire the mutex without blocking.
    pub fn try_lock(&self) -> Option<MutexGuard<'_, T>> {
        let current = current_task_id();
        if self.owner.compare_exchange(NO_OWNER, current, Ordering::Acquire, Ordering::Relaxed).is_ok() {
            Some(MutexGuard { mutex: self })
        } else {
            None
        }
    }

    /// Acquire the mutex. If contested, blocks the current task on the wait queue.
    pub fn lock(&self) -> MutexGuard<'_, T> {
        let current = current_task_id();

        loop {
            // First attempt to grab ownership
            if self.owner.compare_exchange(NO_OWNER, current, Ordering::Acquire, Ordering::Relaxed).is_ok() {
                return MutexGuard { mutex: self };
            }

            // Contended: enqueue current task under the wait_queue spinlock
            {
                let mut wq = self.wait_queue.lock();
                // Double check before sleeping: did owner release just now?
                if self.owner.compare_exchange(NO_OWNER, current, Ordering::Acquire, Ordering::Relaxed).is_ok() {
                    return MutexGuard { mutex: self };
                }
                if !wq.contains(current) {
                    wq.enqueue(current);
                }
            }

            // Block current task in scheduler; scheduler will context switch to another task
            block_current_task();
        }
    }

    /// Release the mutex. Verifies that the caller is the actual owner.
    pub fn unlock(&self) -> Result<(), &'static str> {
        let current = current_task_id();
        let owner = self.owner.load(Ordering::Relaxed);
        if owner != current {
            return Err("Cannot unlock mutex: calling task does not own mutex");
        }

        // Pop next waiter if any
        let next_waiter = {
            let mut wq = self.wait_queue.lock();
            // Clear owner back to NO_OWNER
            self.owner.store(NO_OWNER, Ordering::Release);
            wq.dequeue()
        };

        // If there was a waiter, wake it up
        if let Some(waiter_id) = next_waiter {
            wake_task(waiter_id);
        }

        Ok(())
    }

    pub fn is_locked(&self) -> bool {
        self.owner.load(Ordering::Relaxed) != NO_OWNER
    }

    pub fn current_owner(&self) -> u64 {
        self.owner.load(Ordering::Relaxed)
    }
}

pub struct MutexGuard<'a, T> {
    mutex: &'a Mutex<T>,
}

impl<T> Deref for MutexGuard<'_, T> {
    type Target = T;
    fn deref(&self) -> &Self::Target {
        unsafe { &*self.mutex.data.get() }
    }
}

impl<T> DerefMut for MutexGuard<'_, T> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        unsafe { &mut *self.mutex.data.get() }
    }
}

impl<T> Drop for MutexGuard<'_, T> {
    fn drop(&mut self) {
        let _ = self.mutex.unlock();
    }
}

/// A counting semaphore supporting permit tracking and task blocking/wakeups.
pub struct Semaphore {
    permits: AtomicIsize,
    wait_queue: SpinLock<WaitQueue>,
}

impl Semaphore {
    pub const fn new(initial_permits: isize) -> Self {
        Self {
            permits: AtomicIsize::new(initial_permits),
            wait_queue: SpinLock::new(WaitQueue::new()),
        }
    }

    /// Try to acquire a permit without blocking.
    pub fn try_acquire(&self) -> bool {
        let mut curr = self.permits.load(Ordering::Relaxed);
        loop {
            if curr <= 0 {
                return false;
            }
            match self.permits.compare_exchange_weak(curr, curr - 1, Ordering::Acquire, Ordering::Relaxed) {
                Ok(_) => return true,
                Err(actual) => curr = actual,
            }
        }
    }

    /// Acquire a permit. Blocks if no permits are available.
    pub fn acquire(&self) {
        let current = current_task_id();

        // Decrement permit count
        let prev = self.permits.fetch_sub(1, Ordering::AcqRel);
        if prev <= 0 {
            // No permit was available, we must block on the wait queue
            {
                let mut wq = self.wait_queue.lock();
                if !wq.contains(current) {
                    wq.enqueue(current);
                }
            }
            block_current_task();
        }
    }

    /// Release a permit. Wakes an eligible waiter if any tasks were blocked.
    pub fn release(&self) {
        let prev = self.permits.fetch_add(1, Ordering::Release);
        if prev < 0 {
            // There are waiters waiting for a permit
            let next_waiter = {
                let mut wq = self.wait_queue.lock();
                wq.dequeue()
            };

            if let Some(waiter_id) = next_waiter {
                wake_task(waiter_id);
            }
        }
    }

    pub fn available_permits(&self) -> isize {
        self.permits.load(Ordering::Relaxed)
    }
}
