use core::alloc::{GlobalAlloc, Layout};
use core::cell::UnsafeCell;
use core::sync::atomic::{AtomicBool, Ordering};
use crate::mm::allocate_frame;
use crate::vm::{map_page, PageTableFlags, VirtAddr, PAGE_SIZE};

pub const HEAP_START: usize = 0xffff_9000_0000_0000;
pub const HEAP_INITIAL_SIZE: usize = 128 * 1024; // 128 KiB initial heap (32 4KiB pages)
pub const HEAP_MAX_SIZE: usize = 16 * 1024 * 1024; // 16 MiB max heap capacity

#[repr(C)]
struct FreeBlockHeader {
    size: usize, // Size of payload (excluding header)
    next: Option<&'static mut FreeBlockHeader>,
}

pub struct FreeListAllocator {
    head: Option<&'static mut FreeBlockHeader>,
    heap_start: usize,
    heap_end: usize,
    total_allocated: usize,
    total_freed: usize,
}

impl FreeListAllocator {
    pub const fn new() -> Self {
        Self {
            head: None,
            heap_start: 0,
            heap_end: 0,
            total_allocated: 0,
            total_freed: 0,
        }
    }

    pub unsafe fn init(&mut self, start: usize, size: usize) {
        self.heap_start = start;
        self.heap_end = start + size;
        self.total_allocated = 0;
        self.total_freed = 0;

        let header_ptr = start as *mut FreeBlockHeader;
        let payload_size = size - core::mem::size_of::<FreeBlockHeader>();

        unsafe {
            header_ptr.write(FreeBlockHeader {
                size: payload_size,
                next: None,
            });
            self.head = Some(&mut *header_ptr);
        }
    }

    pub fn allocate(&mut self, layout: Layout) -> Result<*mut u8, &'static str> {
        let align_req = layout.align().max(core::mem::align_of::<FreeBlockHeader>());
        // Round size up to multiple of 8 to maintain header alignment for subsequent splits
        let raw_size = layout.size().max(8);
        let size = (raw_size + 7) & !7;

        // Search for a suitable free block
        let mut prev_ptr: *mut Option<&'static mut FreeBlockHeader> = &raw mut self.head;
        let mut current = self.head.take();

        while let Some(block) = current {
            let block_ptr = (&raw mut *block) as *mut u8;
            let header_size = core::mem::size_of::<FreeBlockHeader>();
            let payload_start = (block_ptr as usize) + header_size;

            let align_offset = if payload_start % align_req == 0 {
                0
            } else {
                align_req - (payload_start % align_req)
            };

            let needed_size = size + align_offset;

            if block.size >= needed_size {
                let remaining_size = block.size - needed_size;
                let next_block = block.next.take();

                if remaining_size >= header_size + 16 {
                    // Split the block ensuring next header is properly aligned
                    let raw_split_addr = payload_start + needed_size;
                    let new_block_addr = (raw_split_addr + (header_size - 1)) & !(header_size - 1);
                    let actual_needed = new_block_addr - (block_ptr as usize);

                    if block.size > actual_needed && (block.size - actual_needed) >= header_size + 16 {
                        let new_block_ptr = new_block_addr as *mut FreeBlockHeader;
                        let new_block_size = block.size - actual_needed - header_size;

                        unsafe {
                            new_block_ptr.write(FreeBlockHeader {
                                size: new_block_size,
                                next: next_block,
                            });
                            *prev_ptr = Some(&mut *new_block_ptr);
                        }
                    } else {
                        unsafe { *prev_ptr = next_block };
                    }
                } else {
                    // Hand over the entire block
                    unsafe { *prev_ptr = next_block };
                }

                self.total_allocated += size;
                let result_ptr = (payload_start + align_offset) as *mut u8;
                return Ok(result_ptr);
            }

            // Move to next block
            unsafe {
                *prev_ptr = Some(block);
                let current_ref = (*prev_ptr).as_mut().unwrap();
                prev_ptr = &raw mut current_ref.next;
                current = current_ref.next.take();
            }
        }

        Err("Kernel heap exhausted")
    }

    pub fn deallocate(&mut self, ptr: *mut u8, layout: Layout) -> Result<(), &'static str> {
        let addr = ptr as usize;
        if addr < self.heap_start || addr >= self.heap_end {
            return Err("Deallocation rejected: pointer out of heap bounds");
        }

        let size = layout.size().max(1);
        self.total_freed += size;

        // Simplified deallocation: push freed block to head of free list
        let header_size = core::mem::size_of::<FreeBlockHeader>();
        if addr < self.heap_start + header_size {
            return Err("Deallocation rejected: invalid header alignment");
        }

        let header_ptr = (addr - header_size) as *mut FreeBlockHeader;
        unsafe {
            header_ptr.write(FreeBlockHeader {
                size,
                next: self.head.take(),
            });
            self.head = Some(&mut *header_ptr);
        }

        Ok(())
    }

    pub fn stats(&self) -> (usize, usize, usize) {
        (self.heap_end - self.heap_start, self.total_allocated, self.total_freed)
    }

    pub fn verify_invariants(&self) -> Result<(), &'static str> {
        if self.heap_start >= self.heap_end {
            return Err("Heap invariant violated: heap_start >= heap_end");
        }
        Ok(())
    }
}

pub struct LockedHeap {
    lock: AtomicBool,
    allocator: UnsafeCell<FreeListAllocator>,
}

unsafe impl Sync for LockedHeap {}

impl LockedHeap {
    pub const fn new() -> Self {
        Self {
            lock: AtomicBool::new(false),
            allocator: UnsafeCell::new(FreeListAllocator::new()),
        }
    }

    pub fn lock(&self) -> HeapGuard<'_> {
        while self.lock.compare_exchange_weak(false, true, Ordering::Acquire, Ordering::Relaxed).is_err() {
            core::hint::spin_loop();
        }
        HeapGuard { heap: self }
    }
}

pub struct HeapGuard<'a> {
    heap: &'a LockedHeap,
}

impl core::ops::Deref for HeapGuard<'_> {
    type Target = FreeListAllocator;
    fn deref(&self) -> &FreeListAllocator {
        unsafe { &*self.heap.allocator.get() }
    }
}

impl core::ops::DerefMut for HeapGuard<'_> {
    fn deref_mut(&mut self) -> &mut FreeListAllocator {
        unsafe { &mut *self.heap.allocator.get() }
    }
}

impl Drop for HeapGuard<'_> {
    fn drop(&mut self) {
        self.heap.lock.store(false, Ordering::Release);
    }
}

#[global_allocator]
pub static HEAP_ALLOCATOR: LockedHeap = LockedHeap::new();

unsafe impl GlobalAlloc for LockedHeap {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        match self.lock().allocate(layout) {
            Ok(ptr) => ptr,
            Err(_) => core::ptr::null_mut(),
        }
    }

    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        let _ = self.lock().deallocate(ptr, layout);
    }
}

pub fn init_heap() -> Result<(), &'static str> {
    crate::log_info!("Initializing Kernel Heap subsystem...");
    crate::log_info!("  Heap Virtual Base: {:#018x}", HEAP_START);
    crate::log_info!("  Initial Capacity : {} KiB ({} pages)", HEAP_INITIAL_SIZE / 1024, HEAP_INITIAL_SIZE / PAGE_SIZE);

    let num_pages = HEAP_INITIAL_SIZE / PAGE_SIZE;

    // Allocate backing physical frames and map each page into the active address space
    for i in 0..num_pages {
        let vaddr = VirtAddr::new((HEAP_START + i * PAGE_SIZE) as u64);
        let pframe = allocate_frame().ok_or("PMM failed to supply frame for kernel heap backing")?;

        let flags = PageTableFlags::PRESENT | PageTableFlags::WRITABLE | PageTableFlags::NO_EXECUTE;
        unsafe {
            map_page(vaddr, pframe, flags)?;
        }
    }

    unsafe {
        HEAP_ALLOCATOR.lock().init(HEAP_START, HEAP_INITIAL_SIZE);
    }

    crate::log_info!("Kernel Heap initialized and mapped successfully with WRITABLE + NO_EXECUTE permissions.");
    Ok(())
}

#[derive(Debug, Clone, Copy)]
pub struct HeapStats {
    pub total_bytes: u64,
    pub allocated_bytes: u64,
    pub free_bytes: u64,
}

pub fn get_heap_stats() -> HeapStats {
    let (total, alloc, freed) = HEAP_ALLOCATOR.lock().stats();
    let currently_allocated = alloc.saturating_sub(freed);
    HeapStats {
        total_bytes: total as u64,
        allocated_bytes: currently_allocated as u64,
        free_bytes: (total.saturating_sub(currently_allocated)) as u64,
    }
}

