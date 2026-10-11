use crate::vm::{PAGE_SIZE, PhysAddr};
use core::cell::UnsafeCell;
use core::sync::atomic::{AtomicBool, Ordering};
use limine::memory_map::EntryType;
use limine::request::{HhdmRequest, MemoryMapRequest};

#[used]
#[unsafe(link_section = ".requests")]
static MEMORY_MAP_REQUEST: MemoryMapRequest = MemoryMapRequest::new();

#[used]
#[unsafe(link_section = ".requests")]
static HHDM_REQUEST: HhdmRequest = HhdmRequest::new();

pub const MAX_PHYSICAL_MEM: u64 = 4 * 1024 * 1024 * 1024;
pub const MAX_FRAMES: usize = (MAX_PHYSICAL_MEM as usize) / PAGE_SIZE;
const BITMAP_WORDS: usize = MAX_FRAMES / 64;

pub struct FrameAllocator {
    bitmap: [u64; BITMAP_WORDS],
    total_frames: usize,
    used_frames: usize,
    reserved_frames: usize,
    last_allocated_idx: usize,
}

impl FrameAllocator {
    pub const fn new() -> Self {
        Self {
            bitmap: [!0u64; BITMAP_WORDS], // Initially mark all frames as allocated/reserved
            total_frames: 0,
            used_frames: 0,
            reserved_frames: 0,
            last_allocated_idx: 0,
        }
    }

    #[inline]
    fn set_bit(&mut self, idx: usize) {
        let word = idx / 64;
        let bit = idx % 64;
        self.bitmap[word] |= 1u64 << bit;
    }

    #[inline]
    fn clear_bit(&mut self, idx: usize) {
        let word = idx / 64;
        let bit = idx % 64;
        self.bitmap[word] &= !(1u64 << bit);
    }

    #[inline]
    fn is_set(&self, idx: usize) -> bool {
        let word = idx / 64;
        let bit = idx % 64;
        (self.bitmap[word] & (1u64 << bit)) != 0
    }

    pub fn mark_free(&mut self, frame: PhysAddr) -> Result<(), &'static str> {
        if !frame.is_aligned() {
            return Err("Frame address is not 4KiB aligned");
        }
        let addr = frame.as_u64();
        if addr >= MAX_PHYSICAL_MEM {
            return Err("Frame address exceeds allocator maximum physical capacity");
        }
        let idx = (addr as usize) / PAGE_SIZE;
        if self.is_set(idx) {
            self.clear_bit(idx);
            self.used_frames = self.used_frames.saturating_sub(1);
        }
        Ok(())
    }

    pub fn mark_allocated(&mut self, frame: PhysAddr) -> Result<(), &'static str> {
        if !frame.is_aligned() {
            return Err("Frame address is not 4KiB aligned");
        }
        let addr = frame.as_u64();
        if addr >= MAX_PHYSICAL_MEM {
            return Err("Frame address exceeds allocator maximum physical capacity");
        }
        let idx = (addr as usize) / PAGE_SIZE;
        if !self.is_set(idx) {
            self.set_bit(idx);
            self.used_frames = self.used_frames.saturating_add(1);
        }
        Ok(())
    }

    pub fn allocate_frame(&mut self) -> Option<PhysAddr> {
        let start_idx = self.last_allocated_idx;
        for i in 0..self.total_frames {
            let idx = (start_idx + i) % self.total_frames;
            let word = idx / 64;
            let bit = idx % 64;

            if (self.bitmap[word] & (1u64 << bit)) == 0 {
                self.bitmap[word] |= 1u64 << bit;
                self.used_frames = self.used_frames.saturating_add(1);
                self.last_allocated_idx = (idx + 1) % self.total_frames;
                return Some(PhysAddr::new((idx * PAGE_SIZE) as u64));
            }
        }
        None
    }

    pub fn deallocate_frame(&mut self, frame: PhysAddr) -> Result<(), &'static str> {
        if !frame.is_aligned() {
            return Err("Deallocation rejected: address is not 4KiB page-aligned");
        }
        let addr = frame.as_u64();
        if addr >= MAX_PHYSICAL_MEM {
            return Err("Deallocation rejected: address exceeds managed capacity");
        }
        let idx = (addr as usize) / PAGE_SIZE;
        if idx >= self.total_frames {
            return Err("Deallocation rejected: frame index outside managed memory limits");
        }
        if !self.is_set(idx) {
            return Err("Deallocation rejected: double-free detected (frame is already free)");
        }

        self.clear_bit(idx);
        self.used_frames = self.used_frames.saturating_sub(1);
        if idx < self.last_allocated_idx {
            self.last_allocated_idx = idx;
        }
        Ok(())
    }

    pub fn total_frames(&self) -> usize {
        self.total_frames
    }

    pub fn free_frames(&self) -> usize {
        self.total_frames.saturating_sub(self.used_frames)
    }

    pub fn used_frames(&self) -> usize {
        self.used_frames
    }

    pub fn verify_invariants(&self) -> Result<(), &'static str> {
        if self.used_frames > self.total_frames {
            return Err("FrameAllocator invariant violated: used_frames exceeds total_frames");
        }
        if self.last_allocated_idx > self.total_frames {
            return Err("FrameAllocator invariant violated: last_allocated_idx exceeds total_frames");
        }
        Ok(())
    }
}

/// Synchronization lock and interior mutability wrapper
pub struct Mutex<T> {
    lock: AtomicBool,
    data: UnsafeCell<T>,
}

unsafe impl<T: Send> Sync for Mutex<T> {}

impl<T> Mutex<T> {
    pub const fn new(data: T) -> Self {
        Self {
            lock: AtomicBool::new(false),
            data: UnsafeCell::new(data),
        }
    }

    pub fn lock(&self) -> MutexGuard<'_, T> {
        while self.lock.compare_exchange_weak(false, true, Ordering::Acquire, Ordering::Relaxed).is_err() {
            core::hint::spin_loop();
        }
        MutexGuard { mutex: self }
    }
}

pub struct MutexGuard<'a, T> {
    mutex: &'a Mutex<T>,
}

impl<T> core::ops::Deref for MutexGuard<'_, T> {
    type Target = T;
    fn deref(&self) -> &T {
        unsafe { &*self.mutex.data.get() }
    }
}

impl<T> core::ops::DerefMut for MutexGuard<'_, T> {
    fn deref_mut(&mut self) -> &mut T {
        unsafe { &mut *self.mutex.data.get() }
    }
}

impl<T> Drop for MutexGuard<'_, T> {
    fn drop(&mut self) {
        self.mutex.lock.store(false, Ordering::Release);
    }
}

pub static ALLOCATOR: Mutex<FrameAllocator> = Mutex::new(FrameAllocator::new());

pub fn get_hhdm_offset() -> Option<u64> {
    HHDM_REQUEST.get_response().map(|r| r.offset())
}

pub fn parse_memory_map() {
    crate::log_info!("Inspecting physical memory map via Limine protocol...");

    let Some(response) = MEMORY_MAP_REQUEST.get_response() else {
        crate::log_warn!("No Memory Map response provided by Limine bootloader.");
        return;
    };

    let entries = response.entries();
    crate::log_info!("Memory map entries enumerated: {}", entries.len());

    let mut usable_bytes: u64 = 0;
    let mut reserved_bytes: u64 = 0;
    let mut reclaimable_bytes: u64 = 0;
    let mut other_bytes: u64 = 0;

    for (idx, entry) in entries.iter().enumerate() {
        let base = entry.base;
        let length = entry.length;
        let entry_type = entry.entry_type;

        let end_address = match base.checked_add(length) {
            Some(end) => end,
            None => {
                crate::log_error!("[{:02}] OVERFLOW DETECTED: Base={:#018x} + Length={:#018x}", idx, base, length);
                continue;
            }
        };

        let type_str = match entry_type {
            EntryType::USABLE => {
                usable_bytes = usable_bytes.saturating_add(length);
                "USABLE"
            }
            EntryType::RESERVED => {
                reserved_bytes = reserved_bytes.saturating_add(length);
                "RESERVED"
            }
            EntryType::ACPI_RECLAIMABLE => {
                reclaimable_bytes = reclaimable_bytes.saturating_add(length);
                "ACPI_RECLAIMABLE"
            }
            EntryType::ACPI_NVS => {
                reserved_bytes = reserved_bytes.saturating_add(length);
                "ACPI_NVS"
            }
            EntryType::BAD_MEMORY => {
                other_bytes = other_bytes.saturating_add(length);
                "BAD_MEMORY"
            }
            EntryType::BOOTLOADER_RECLAIMABLE => {
                reclaimable_bytes = reclaimable_bytes.saturating_add(length);
                "BOOTLOADER_RECLAIMABLE"
            }
            EntryType::EXECUTABLE_AND_MODULES => {
                reserved_bytes = reserved_bytes.saturating_add(length);
                "EXECUTABLE_AND_MODULES"
            }
            EntryType::FRAMEBUFFER => {
                reserved_bytes = reserved_bytes.saturating_add(length);
                "FRAMEBUFFER"
            }
            _ => {
                other_bytes = other_bytes.saturating_add(length);
                "UNKNOWN"
            }
        };

        crate::log_debug!(
            "[{:02}] {:#018x} - {:#018x} ({:>10} KiB) -> {}",
            idx,
            base,
            end_address,
            length / 1024,
            type_str
        );
    }

    crate::log_info!(
        "Memory Summary: Usable: {} MiB | Reserved: {} MiB | Reclaimable: {} MiB",
        usable_bytes / (1024 * 1024),
        reserved_bytes / (1024 * 1024),
        reclaimable_bytes / (1024 * 1024)
    );
}

pub fn init_frame_allocator() {
    let Some(response) = MEMORY_MAP_REQUEST.get_response() else {
        crate::log_error!("Failed to initialize frame allocator: No Limine Memory Map response.");
        return;
    };

    let entries = response.entries();
    let mut max_usable_address: u64 = 0;

    for entry in entries.iter() {
        if entry.entry_type == EntryType::USABLE {
            let end = entry.base.saturating_add(entry.length);
            if end > max_usable_address {
                max_usable_address = end.min(MAX_PHYSICAL_MEM);
            }
        }
    }

    let total_frames = ((max_usable_address as usize) / PAGE_SIZE).min(MAX_FRAMES);

    let mut allocator = ALLOCATOR.lock();
    allocator.total_frames = total_frames;
    allocator.used_frames = total_frames; // Initially consider everything allocated/reserved
    allocator.reserved_frames = total_frames;

    // Mark exclusively USABLE regions as free
    for entry in entries.iter() {
        if entry.entry_type == EntryType::USABLE {
            let start_frame = PhysAddr::new(entry.base).align_up().unwrap_or(PhysAddr::new(entry.base));
            let end_addr = entry.base.saturating_add(entry.length).min(MAX_PHYSICAL_MEM);
            let end_frame = PhysAddr::new(end_addr).align_down();

            let mut cur = start_frame.as_u64();
            while cur < end_frame.as_u64() {
                let _ = allocator.mark_free(PhysAddr::new(cur));
                cur = cur.saturating_add(PAGE_SIZE as u64);
            }
        }
    }

    // Exclude the low 1MB (BIOS, IVT, BDA, VGA hardware regions)
    let mut cur = 0u64;
    while cur < 0x100000 {
        let _ = allocator.mark_allocated(PhysAddr::new(cur));
        cur += PAGE_SIZE as u64;
    }

    crate::log_info!(
        "Bitmap Physical Frame Allocator initialized: Total: {} frames ({} MiB), Free: {} frames ({} MiB), Used/Reserved: {} frames",
        allocator.total_frames(),
        (allocator.total_frames() * PAGE_SIZE) / (1024 * 1024),
        allocator.free_frames(),
        (allocator.free_frames() * PAGE_SIZE) / (1024 * 1024),
        allocator.used_frames()
    );
}

pub fn allocate_frame() -> Option<PhysAddr> {
    ALLOCATOR.lock().allocate_frame()
}

pub fn deallocate_frame(frame: PhysAddr) -> Result<(), &'static str> {
    ALLOCATOR.lock().deallocate_frame(frame)
}
