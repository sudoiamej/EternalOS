use core::arch::asm;
use crate::mm::{allocate_frame, get_hhdm_offset};

pub const PAGE_SIZE: usize = 4096;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
#[repr(transparent)]
pub struct PhysAddr(pub u64);

impl PhysAddr {
    #[inline]
    pub const fn new(addr: u64) -> Self {
        Self(addr)
    }

    #[inline]
    pub const fn as_u64(self) -> u64 {
        self.0
    }

    #[inline]
    pub const fn is_aligned(self) -> bool {
        (self.0 % PAGE_SIZE as u64) == 0
    }

    #[inline]
    pub const fn align_down(self) -> Self {
        Self(self.0 & !(PAGE_SIZE as u64 - 1))
    }

    #[inline]
    pub const fn align_up(self) -> Option<Self> {
        let mask = PAGE_SIZE as u64 - 1;
        match self.0.checked_add(mask) {
            Some(added) => Some(Self(added & !mask)),
            None => None,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
#[repr(transparent)]
pub struct VirtAddr(pub u64);

impl VirtAddr {
    #[inline]
    pub const fn new(addr: u64) -> Self {
        Self(addr)
    }

    #[inline]
    pub const fn as_u64(self) -> u64 {
        self.0
    }

    #[inline]
    pub const fn as_ptr<T>(self) -> *const T {
        self.0 as *const T
    }

    #[inline]
    pub const fn as_mut_ptr<T>(self) -> *mut T {
        self.0 as *mut T
    }

    #[inline]
    pub const fn is_aligned(self) -> bool {
        (self.0 % PAGE_SIZE as u64) == 0
    }

    #[inline]
    pub const fn is_canonical(self) -> bool {
        let sign_extension = self.0 >> 47;
        sign_extension == 0 || sign_extension == 0x1ffff
    }

    #[inline]
    pub const fn p4_index(self) -> usize {
        ((self.0 >> 39) & 0x1ff) as usize
    }

    #[inline]
    pub const fn p3_index(self) -> usize {
        ((self.0 >> 30) & 0x1ff) as usize
    }

    #[inline]
    pub const fn p2_index(self) -> usize {
        ((self.0 >> 21) & 0x1ff) as usize
    }

    #[inline]
    pub const fn p1_index(self) -> usize {
        ((self.0 >> 12) & 0x1ff) as usize
    }

    #[inline]
    pub const fn page_offset(self) -> usize {
        (self.0 & 0xfff) as usize
    }
}

pub struct PageTableFlags;
impl PageTableFlags {
    pub const PRESENT: u64 = 1 << 0;
    pub const WRITABLE: u64 = 1 << 1;
    pub const USER_ACCESSIBLE: u64 = 1 << 2;
    pub const WRITE_THROUGH: u64 = 1 << 3;
    pub const NO_CACHE: u64 = 1 << 4;
    pub const ACCESSED: u64 = 1 << 5;
    pub const DIRTY: u64 = 1 << 6;
    pub const HUGE_PAGE: u64 = 1 << 7;
    pub const GLOBAL: u64 = 1 << 8;
    pub const NO_EXECUTE: u64 = 1 << 63;
}

#[derive(Clone, Copy)]
#[repr(transparent)]
pub struct PageTableEntry(pub u64);

impl PageTableEntry {
    pub const fn new() -> Self {
        Self(0)
    }

    pub fn is_present(&self) -> bool {
        (self.0 & PageTableFlags::PRESENT) != 0
    }

    pub fn is_huge(&self) -> bool {
        (self.0 & PageTableFlags::HUGE_PAGE) != 0
    }

    pub fn phys_addr(&self) -> PhysAddr {
        PhysAddr::new(self.0 & 0x000f_ffff_ffff_f000)
    }

    pub fn set_frame(&mut self, frame: PhysAddr, flags: u64) {
        self.0 = (frame.as_u64() & 0x000f_ffff_ffff_f000) | (flags & !0x000f_ffff_ffff_f000);
    }

    pub fn flags(&self) -> u64 {
        self.0 & !0x000f_ffff_ffff_f000
    }

    pub fn is_writable(&self) -> bool {
        (self.0 & PageTableFlags::WRITABLE) != 0
    }

    pub fn is_user(&self) -> bool {
        (self.0 & PageTableFlags::USER_ACCESSIBLE) != 0
    }

    pub fn is_no_execute(&self) -> bool {
        (self.0 & PageTableFlags::NO_EXECUTE) != 0
    }

    pub fn set_flags(&mut self, flags: u64) {
        let frame = self.phys_addr();
        self.set_frame(frame, flags);
    }

    pub fn clear(&mut self) {
        self.0 = 0;
    }
}

#[repr(C, align(4096))]
pub struct PageTable {
    pub entries: [PageTableEntry; 512],
}

impl PageTable {
    pub const fn new() -> Self {
        Self {
            entries: [PageTableEntry::new(); 512],
        }
    }

    pub fn is_empty(&self) -> bool {
        for entry in self.entries.iter() {
            if entry.is_present() {
                return false;
            }
        }
        true
    }

    pub fn present_count(&self) -> usize {
        let mut count = 0;
        for entry in self.entries.iter() {
            if entry.is_present() {
                count += 1;
            }
        }
        count
    }

    pub fn zero(&mut self) {
        for entry in self.entries.iter_mut() {
            entry.clear();
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Cr3 {
    pub pml4_phys: PhysAddr,
    pub flags: u16,
}

impl Cr3 {
    pub fn read() -> Self {
        let val: u64;
        unsafe {
            asm!("mov {}, cr3", out(reg) val, options(nomem, nostack, preserves_flags));
        }
        Self {
            pml4_phys: PhysAddr::new(val & 0x000f_ffff_ffff_f000),
            flags: (val & 0xfff) as u16,
        }
    }
}

#[inline]
pub unsafe fn invlpg(vaddr: VirtAddr) {
    unsafe {
        asm!("invlpg [{}]", in(reg) vaddr.as_u64(), options(nostack, preserves_flags));
    }
}

pub fn phys_to_virt(phys: PhysAddr) -> Option<VirtAddr> {
    let hhdm = get_hhdm_offset()?;
    phys.as_u64().checked_add(hhdm).map(VirtAddr::new)
}

/// Maps a 4KiB virtual page to a physical frame in the active PML4
pub unsafe fn map_page(vaddr: VirtAddr, pframe: PhysAddr, flags: u64) -> Result<(), &'static str> {
    if !vaddr.is_aligned() {
        return Err("Virtual address is not 4KiB aligned");
    }
    if !vaddr.is_canonical() {
        return Err("Virtual address is non-canonical");
    }
    if !pframe.is_aligned() {
        return Err("Physical frame is not 4KiB aligned");
    }

    let cr3 = Cr3::read();
    let pml4_virt = phys_to_virt(cr3.pml4_phys).ok_or("HHDM offset not available")?;
    let pml4 = unsafe { &mut *pml4_virt.as_mut_ptr::<PageTable>() };

    let p4_idx = vaddr.p4_index();
    let p3_table = get_or_create_next_table(&mut pml4.entries[p4_idx], flags)?;

    let p3_idx = vaddr.p3_index();
    let p2_table = get_or_create_next_table(&mut p3_table.entries[p3_idx], flags)?;

    let p2_idx = vaddr.p2_index();
    let p1_table = get_or_create_next_table(&mut p2_table.entries[p2_idx], flags)?;

    let p1_idx = vaddr.p1_index();
    let p1_entry = &mut p1_table.entries[p1_idx];
    if p1_entry.is_present() {
        return Err("Page already mapped");
    }

    p1_entry.set_frame(pframe, flags | PageTableFlags::PRESENT);
    unsafe { invlpg(vaddr) };

    Ok(())
}

/// Translates a virtual address to its corresponding physical address by walking the page tables.
pub fn translate_addr(vaddr: VirtAddr) -> Result<PhysAddr, &'static str> {
    if !vaddr.is_canonical() {
        return Err("Virtual address is non-canonical");
    }

    let cr3 = Cr3::read();
    let pml4_virt = phys_to_virt(cr3.pml4_phys).ok_or("HHDM offset not available")?;
    let pml4 = unsafe { &*pml4_virt.as_ptr::<PageTable>() };

    let p4_entry = &pml4.entries[vaddr.p4_index()];
    if !p4_entry.is_present() {
        return Err("Virtual address is not mapped (PML4 missing)");
    }
    if p4_entry.is_huge() {
        return Err("Unexpected huge page entry at PML4 level");
    }

    let p3_virt = phys_to_virt(p4_entry.phys_addr()).ok_or("HHDM offset not available")?;
    let p3_table = unsafe { &*p3_virt.as_ptr::<PageTable>() };

    let p3_entry = &p3_table.entries[vaddr.p3_index()];
    if !p3_entry.is_present() {
        return Err("Virtual address is not mapped (PDPT missing)");
    }
    if p3_entry.is_huge() {
        // 1 GiB huge page translation
        let base = p3_entry.phys_addr().as_u64();
        let offset = vaddr.as_u64() & 0x3fff_ffff;
        return Ok(PhysAddr::new(base | offset));
    }

    let p2_virt = phys_to_virt(p3_entry.phys_addr()).ok_or("HHDM offset not available")?;
    let p2_table = unsafe { &*p2_virt.as_ptr::<PageTable>() };

    let p2_entry = &p2_table.entries[vaddr.p2_index()];
    if !p2_entry.is_present() {
        return Err("Virtual address is not mapped (PD missing)");
    }
    if p2_entry.is_huge() {
        // 2 MiB huge page translation
        let base = p2_entry.phys_addr().as_u64();
        let offset = vaddr.as_u64() & 0x1f_ffff;
        return Ok(PhysAddr::new(base | offset));
    }

    let p1_virt = phys_to_virt(p2_entry.phys_addr()).ok_or("HHDM offset not available")?;
    let p1_table = unsafe { &*p1_virt.as_ptr::<PageTable>() };

    let p1_entry = &p1_table.entries[vaddr.p1_index()];
    if !p1_entry.is_present() {
        return Err("Virtual address is not mapped (PT missing)");
    }

    let base = p1_entry.phys_addr().as_u64();
    let offset = vaddr.page_offset() as u64;
    Ok(PhysAddr::new(base | offset))
}

pub fn is_mapped(vaddr: VirtAddr) -> bool {
    translate_addr(vaddr).is_ok()
}

/// Updates the permissions and flags of an existing mapped 4KiB page and flushes the TLB
pub unsafe fn update_page_flags(vaddr: VirtAddr, flags: u64) -> Result<(), &'static str> {
    if !vaddr.is_aligned() {
        return Err("Virtual address is not 4KiB aligned");
    }
    if !vaddr.is_canonical() {
        return Err("Virtual address is non-canonical");
    }

    let cr3 = Cr3::read();
    let pml4_virt = phys_to_virt(cr3.pml4_phys).ok_or("HHDM offset not available")?;
    let pml4 = unsafe { &mut *pml4_virt.as_mut_ptr::<PageTable>() };

    let p4_entry = &mut pml4.entries[vaddr.p4_index()];
    if !p4_entry.is_present() {
        return Err("Page not mapped (PML4 missing)");
    }
    let p3_virt = phys_to_virt(p4_entry.phys_addr()).ok_or("HHDM offset not available")?;
    let p3_table = unsafe { &mut *p3_virt.as_mut_ptr::<PageTable>() };

    let p3_entry = &mut p3_table.entries[vaddr.p3_index()];
    if !p3_entry.is_present() {
        return Err("Page not mapped (PDPT missing)");
    }
    let p2_virt = phys_to_virt(p3_entry.phys_addr()).ok_or("HHDM offset not available")?;
    let p2_table = unsafe { &mut *p2_virt.as_mut_ptr::<PageTable>() };

    let p2_entry = &mut p2_table.entries[vaddr.p2_index()];
    if !p2_entry.is_present() {
        return Err("Page not mapped (PD missing)");
    }
    let p1_virt = phys_to_virt(p2_entry.phys_addr()).ok_or("HHDM offset not available")?;
    let p1_table = unsafe { &mut *p1_virt.as_mut_ptr::<PageTable>() };

    let p1_entry = &mut p1_table.entries[vaddr.p1_index()];
    if !p1_entry.is_present() {
        return Err("Page not mapped (PT missing)");
    }

    let frame = p1_entry.phys_addr();
    p1_entry.set_frame(frame, flags | PageTableFlags::PRESENT);
    unsafe { invlpg(vaddr) };

    Ok(())
}

/// Unmaps a 4KiB virtual page from the active PML4 and returns the mapped physical frame.
/// Reclaims empty intermediate page tables (PT, PD, PDPT) back to the PMM if they have no present entries.
pub unsafe fn unmap_page(vaddr: VirtAddr) -> Result<PhysAddr, &'static str> {
    if !vaddr.is_aligned() {
        return Err("Virtual address is not 4KiB aligned");
    }
    if !vaddr.is_canonical() {
        return Err("Virtual address is non-canonical");
    }

    let cr3 = Cr3::read();
    let pml4_virt = phys_to_virt(cr3.pml4_phys).ok_or("HHDM offset not available")?;
    let pml4 = unsafe { &mut *pml4_virt.as_mut_ptr::<PageTable>() };

    let p4_entry = &mut pml4.entries[vaddr.p4_index()];
    if !p4_entry.is_present() {
        return Err("Page not mapped (PML4 missing)");
    }
    let p3_frame = p4_entry.phys_addr();
    let p3_virt = phys_to_virt(p3_frame).ok_or("HHDM offset not available")?;
    let p3_table = unsafe { &mut *p3_virt.as_mut_ptr::<PageTable>() };

    let p3_entry = &mut p3_table.entries[vaddr.p3_index()];
    if !p3_entry.is_present() {
        return Err("Page not mapped (PDPT missing)");
    }
    let p2_frame = p3_entry.phys_addr();
    let p2_virt = phys_to_virt(p2_frame).ok_or("HHDM offset not available")?;
    let p2_table = unsafe { &mut *p2_virt.as_mut_ptr::<PageTable>() };

    let p2_entry = &mut p2_table.entries[vaddr.p2_index()];
    if !p2_entry.is_present() {
        return Err("Page not mapped (PD missing)");
    }
    let p1_frame = p2_entry.phys_addr();
    let p1_virt = phys_to_virt(p1_frame).ok_or("HHDM offset not available")?;
    let p1_table = unsafe { &mut *p1_virt.as_mut_ptr::<PageTable>() };

    let p1_entry = &mut p1_table.entries[vaddr.p1_index()];
    if !p1_entry.is_present() {
        return Err("Page not mapped (PT missing)");
    }

    let pframe = p1_entry.phys_addr();
    p1_entry.clear();
    unsafe { invlpg(vaddr) };

    // Reclaim intermediate page table pages if they become completely empty
    if p1_table.is_empty() {
        p2_entry.clear();
        let _ = crate::mm::deallocate_frame(p1_frame);

        if p2_table.is_empty() {
            p3_entry.clear();
            let _ = crate::mm::deallocate_frame(p2_frame);

            if p3_table.is_empty() {
                p4_entry.clear();
                let _ = crate::mm::deallocate_frame(p3_frame);
            }
        }
    }

    Ok(pframe)
}

fn get_or_create_next_table(
    entry: &mut PageTableEntry,
    flags: u64,
) -> Result<&'static mut PageTable, &'static str> {
    if entry.is_present() {
        if entry.is_huge() {
            return Err("Encountered unexpected huge page entry in page table hierarchy");
        }
        let next_table_virt = phys_to_virt(entry.phys_addr()).ok_or("HHDM offset not available")?;
        return Ok(unsafe { &mut *next_table_virt.as_mut_ptr::<PageTable>() });
    }

    // Allocate new page table frame from PMM
    let new_frame = allocate_frame().ok_or("PMM out of memory for page table allocation")?;
    let new_table_virt = phys_to_virt(new_frame).ok_or("HHDM offset not available")?;
    let table = unsafe { &mut *new_table_virt.as_mut_ptr::<PageTable>() };
    table.zero();

    // Intermediate entries must be present and writable so lower levels can enforce leaf permissions
    let table_flags = PageTableFlags::PRESENT | PageTableFlags::WRITABLE | (flags & PageTableFlags::USER_ACCESSIBLE);
    entry.set_frame(new_frame, table_flags);

    Ok(table)
}

pub fn inspect_paging_environment() {
    let cr3 = Cr3::read();
    crate::log_info!("x86-64 Virtual Memory & Active Paging State:");
    crate::log_info!("  Paging Mode : 4-Level Paging (x86-64 standard)");
    crate::log_info!("  Active PML4 : PhysAddr {:#018x} (CR3 flags={:#x})", cr3.pml4_phys.as_u64(), cr3.flags);
    crate::log_info!("  Canonical VM: [0x0000000000000000..0x00007fffffffffff] & [0xffff800000000000..0xffffffffffffffff]");
    crate::log_info!("  Higher-Half : Limine kernel mapped in upper 2GiB (base 0xffffffff80000000)");
}
