use core::arch::asm;

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
        ((self.0 >> 31) & 0x1ff) as usize
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

pub fn inspect_paging_environment() {
    let cr3 = Cr3::read();
    crate::log_info!("x86-64 Virtual Memory & Active Paging State:");
    crate::log_info!("  Paging Mode : 4-Level Paging (x86-64 standard)");
    crate::log_info!("  Active PML4 : PhysAddr {:#018x} (CR3 flags={:#x})", cr3.pml4_phys.as_u64(), cr3.flags);
    crate::log_info!("  Canonical VM: [0x0000000000000000..0x00007fffffffffff] & [0xffff800000000000..0xffffffffffffffff]");
    crate::log_info!("  Higher-Half : Limine kernel mapped in upper 2GiB (base 0xffffffff80000000)");
}
