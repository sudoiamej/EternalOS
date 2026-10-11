use crate::block::{BlockDevice, BlockDeviceInfo, BlockError, validate_request};
use crate::pci::{PciDeviceFunction, read_config_u16, write_config_u16};
use crate::vm::VirtAddr;
use core::arch::asm;
use core::sync::atomic::{fence, Ordering};

// VirtIO PCI Registers (I/O Port Bar)
const VIRTIO_PCI_QUEUE_PFN: u16 = 0x08;      // 32-bit (RW)
const VIRTIO_PCI_QUEUE_NUM: u16 = 0x0c;      // 16-bit (RO)
const VIRTIO_PCI_QUEUE_SEL: u16 = 0x0e;      // 16-bit (RW)
const VIRTIO_PCI_QUEUE_NOTIFY: u16 = 0x10;   // 16-bit (RW)
const VIRTIO_PCI_STATUS: u16 = 0x12;         // 8-bit  (RW)
const VIRTIO_PCI_CONFIG_BASE: u16 = 0x14;    // Device-specific config space

// VirtIO Device Status Bits
const VIRTIO_STATUS_ACKNOWLEDGE: u8 = 1;
const VIRTIO_STATUS_DRIVER: u8 = 2;
const VIRTIO_STATUS_DRIVER_OK: u8 = 4;

// VirtIO Descriptor Flags
const VRING_DESC_F_NEXT: u16 = 1;
const VRING_DESC_F_WRITE: u16 = 2;

// VirtIO Block Request Types
const VIRTIO_BLK_T_IN: u32 = 0;   // Read
const VIRTIO_BLK_T_OUT: u32 = 1;  // Write

// VirtIO Block Status Byte Values
const VIRTIO_BLK_S_OK: u8 = 0;
const VIRTIO_BLK_S_IOERR: u8 = 1;
const VIRTIO_BLK_S_UNSUPP: u8 = 2;

/// A single VirtIO Split Ring Descriptor (16 bytes).
#[repr(C, align(16))]
#[derive(Debug, Clone, Copy)]
pub struct VirtqDesc {
    pub addr: u64,
    pub len: u32,
    pub flags: u16,
    pub next: u16,
}

/// VirtIO Split Ring Available Ring for 256 descriptors.
/// flags (2) + idx (2) + ring (256 * 2 = 512) + used_event (2) = 518 bytes.
#[repr(C)]
pub struct VirtqAvail {
    pub flags: u16,
    pub idx: u16,
    pub ring: [u16; 256],
    pub used_event: u16,
}

/// A used ring element (8 bytes).
#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct VirtqUsedElem {
    pub id: u32,
    pub len: u32,
}

/// VirtIO Split Ring Used Ring for 256 descriptors.
/// flags (2) + idx (2) + ring (256 * 8 = 2048) + avail_event (2) = 2054 bytes.
#[repr(C)]
pub struct VirtqUsed {
    pub flags: u16,
    pub idx: u16,
    pub ring: [VirtqUsedElem; 256],
    pub avail_event: u16,
}

/// VirtIO Block Request Header (16 bytes).
#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct VirtioBlkReqHeader {
    pub req_type: u32,
    pub reserved: u32,
    pub sector: u64,
}

/// Statically allocated, page-aligned DMA memory pool for VirtIO split queue and request buffers.
/// Standard Legacy VirtIO Layout for queue size 256:
/// - Descriptors: 256 * 16 = 4096 bytes (Page 0)
/// - Available ring: 4 + 2 * 256 = 516 bytes (Page 1)
/// - Padding to 4096-byte boundary: 4096 - 516 = 3580 bytes
/// - Used ring: 4 + 8 * 256 = 2052 bytes (Page 2, aligned to 4096 bytes)
/// - Bounce buffers & request structs (Page 3)
#[repr(C, align(4096))]
struct VirtioDmaPool {
    // Page 0 (4096 bytes): 256 Descriptors
    descriptors: [VirtqDesc; 256],

    // Page 1 (4096 bytes): Available Ring + padding to 4096-byte boundary
    avail: VirtqAvail,
    _pad0: [u8; 4096 - core::mem::size_of::<VirtqAvail>()],

    // Page 2 (4096 bytes): Used Ring (strictly aligned to 4096 bytes from pool base)
    used: VirtqUsed,
    _pad1: [u8; 4096 - core::mem::size_of::<VirtqUsed>()],

    // Page 3 (4096 bytes): Request headers, status bytes, and bounce buffer
    req_header: VirtioBlkReqHeader,
    req_status: u8,
    _pad2: [u8; 4096 - (core::mem::size_of::<VirtioBlkReqHeader>() + 1)],
    bounce_buffer: [u8; 4096],
}

static mut VIRTIO_DMA: VirtioDmaPool = VirtioDmaPool {
    descriptors: [VirtqDesc { addr: 0, len: 0, flags: 0, next: 0 }; 256],
    avail: VirtqAvail { flags: 0, idx: 0, ring: [0; 256], used_event: 0 },
    _pad0: [0; 4096 - core::mem::size_of::<VirtqAvail>()],
    used: VirtqUsed { flags: 0, idx: 0, ring: [VirtqUsedElem { id: 0, len: 0 }; 256], avail_event: 0 },
    _pad1: [0; 4096 - core::mem::size_of::<VirtqUsed>()],
    req_header: VirtioBlkReqHeader { req_type: 0, reserved: 0, sector: 0 },
    req_status: 0,
    _pad2: [0; 4096 - (core::mem::size_of::<VirtioBlkReqHeader>() + 1)],
    bounce_buffer: [0; 4096],
};

#[inline]
unsafe fn outb(port: u16, val: u8) {
    unsafe { asm!("out dx, al", in("dx") port, in("al") val, options(nomem, nostack, preserves_flags)); }
}

#[inline]
unsafe fn inw(port: u16) -> u16 {
    let val: u16;
    unsafe { asm!("in ax, dx", in("dx") port, out("ax") val, options(nomem, nostack, preserves_flags)); }
    val
}

#[inline]
unsafe fn outw(port: u16, val: u16) {
    unsafe { asm!("out dx, ax", in("dx") port, in("ax") val, options(nomem, nostack, preserves_flags)); }
}

#[inline]
unsafe fn inl(port: u16) -> u32 {
    let val: u32;
    unsafe { asm!("in eax, dx", in("dx") port, out("eax") val, options(nomem, nostack, preserves_flags)); }
    val
}

#[inline]
unsafe fn outl(port: u16, val: u32) {
    unsafe { asm!("out dx, eax", in("dx") port, in("eax") val, options(nomem, nostack, preserves_flags)); }
}

/// VirtIO Block Device Driver (Legacy PCI Transport).
pub struct VirtioBlockDevice {
    io_base: u16,
    _queue_size: u16,
    last_used_idx: u16,
    info: BlockDeviceInfo,
    _queue_pfn: u32,
}

impl VirtioBlockDevice {
    /// Initialize VirtIO Block device from PCI function configuration.
    pub fn init_from_pci(dev: &PciDeviceFunction) -> Result<Self, &'static str> {
        // 1. Identify I/O Base from BAR0
        let io_base = match dev.bars[0].bar_type {
            crate::pci::BarType::Io { port } => port as u16,
            _ => return Err("VirtIO Block: BAR0 is not an I/O port"),
        };

        crate::log_info!("[VIRTIO-BLK] Found device at PCI {:02x}:{:02x}.{:x}, I/O Port: 0x{:04x}",
            dev.bus, dev.device, dev.function, io_base);

        // 2. Enable Bus Mastering and I/O Space in PCI Command Register
        let cmd = dev.command | 0x0001 | 0x0004; // I/O Space (bit 0) | Bus Master (bit 2)
        write_config_u16(dev.bus, dev.device, dev.function, 0x04, cmd);
        let active_cmd = read_config_u16(dev.bus, dev.device, dev.function, 0x04);
        crate::log_info!("[VIRTIO-BLK] PCI Command register enabled: 0x{:04x}", active_cmd);

        // 3. Reset the device by writing 0 to Status
        unsafe {
            outb(io_base + VIRTIO_PCI_STATUS, 0);
        }

        // 4. Set ACKNOWLEDGE and DRIVER status bits
        unsafe {
            outb(io_base + VIRTIO_PCI_STATUS, VIRTIO_STATUS_ACKNOWLEDGE);
            outb(io_base + VIRTIO_PCI_STATUS, VIRTIO_STATUS_ACKNOWLEDGE | VIRTIO_STATUS_DRIVER);
        }

        // 5. Read Device Capacity from device-specific config space
        let cap_low = unsafe { inl(io_base + VIRTIO_PCI_CONFIG_BASE) };
        let cap_high = unsafe { inl(io_base + VIRTIO_PCI_CONFIG_BASE + 4) };
        let capacity_sectors = ((cap_high as u64) << 32) | (cap_low as u64);

        crate::log_info!("[VIRTIO-BLK] Capacity: {} sectors ({} MiB)",
            capacity_sectors, (capacity_sectors * 512) / (1024 * 1024));

        // 6. Select Queue 0 and read queue size
        unsafe {
            outw(io_base + VIRTIO_PCI_QUEUE_SEL, 0);
        }
        let q_size = unsafe { inw(io_base + VIRTIO_PCI_QUEUE_NUM) };
        if q_size == 0 {
            return Err("VirtIO Block: Queue 0 not available on controller");
        }
        crate::log_info!("[VIRTIO-BLK] Queue 0 size: {} descriptors", q_size);

        // 7. Calculate physical address of statically allocated DMA pool
        let virt_pool_addr = &raw const VIRTIO_DMA as u64;
        let phys_pool_addr = match crate::vm::translate_addr(VirtAddr::new(virt_pool_addr)) {
            Ok(p) => p.as_u64(),
            Err(_) => return Err("VirtIO Block: Failed to translate DMA pool virtual address to physical"),
        };

        if (phys_pool_addr & 0xFFF) != 0 {
            return Err("VirtIO Block: DMA pool is not 4KiB page-aligned");
        }

        let queue_pfn = (phys_pool_addr >> 12) as u32;
        crate::log_info!("[VIRTIO-BLK] DMA Pool Phys: 0x{:016x}, PFN: 0x{:08x}", phys_pool_addr, queue_pfn);

        // 8. Program Queue PFN
        unsafe {
            outl(io_base + VIRTIO_PCI_QUEUE_PFN, queue_pfn);
        }

        // 9. Set DRIVER_OK status
        unsafe {
            outb(io_base + VIRTIO_PCI_STATUS,
                VIRTIO_STATUS_ACKNOWLEDGE | VIRTIO_STATUS_DRIVER | VIRTIO_STATUS_DRIVER_OK);
        }

        let info = BlockDeviceInfo::new("QEMU VirtIO-Block", capacity_sectors, 512, false);

        Ok(Self {
            io_base,
            _queue_size: q_size,
            last_used_idx: 0,
            info,
            _queue_pfn: queue_pfn,
        })
    }

    /// Execute a block transfer command (read or write) using the static DMA pool.
    fn execute_transfer(
        &mut self,
        req_type: u32,
        start_sector: u64,
        sector_count: u32,
        data: &mut [u8],
    ) -> Result<(), BlockError> {
        if sector_count > 8 {
            return Err(BlockError::Unsupported); // Bounce buffer accommodates up to 8 sectors (4096 bytes)
        }

        let byte_count = (sector_count as usize) * 512;

        unsafe {
            let pool = &raw mut VIRTIO_DMA;

            // Setup Request Header
            (*pool).req_header.req_type = req_type;
            (*pool).req_header.reserved = 0;
            (*pool).req_header.sector = start_sector;
            (*pool).req_status = 0xFF; // In-flight indicator

            // For writes, copy incoming data into bounce buffer
            if req_type == VIRTIO_BLK_T_OUT {
                (&mut (*pool).bounce_buffer)[..byte_count].copy_from_slice(&data[..byte_count]);
            }

            // Translate physical addresses of header, buffer, and status
            let header_virt = &raw const (*pool).req_header as u64;
            let buffer_virt = &raw const (*pool).bounce_buffer as u64;
            let status_virt = &raw const (*pool).req_status as u64;

            let header_phys = crate::vm::translate_addr(VirtAddr::new(header_virt))
                .map_err(|_| BlockError::MemoryError)?
                .as_u64();
            let buffer_phys = crate::vm::translate_addr(VirtAddr::new(buffer_virt))
                .map_err(|_| BlockError::MemoryError)?
                .as_u64();
            let status_phys = crate::vm::translate_addr(VirtAddr::new(status_virt))
                .map_err(|_| BlockError::MemoryError)?
                .as_u64();

            // Descriptor 0: Request Header (Device Reads)
            (*pool).descriptors[0].addr = header_phys;
            (*pool).descriptors[0].len = core::mem::size_of::<VirtioBlkReqHeader>() as u32;
            (*pool).descriptors[0].flags = VRING_DESC_F_NEXT;
            (*pool).descriptors[0].next = 1;

            // Descriptor 1: Data Buffer (Device Reads for write, Device Writes for read)
            (*pool).descriptors[1].addr = buffer_phys;
            (*pool).descriptors[1].len = byte_count as u32;
            (*pool).descriptors[1].flags = if req_type == VIRTIO_BLK_T_IN {
                VRING_DESC_F_NEXT | VRING_DESC_F_WRITE
            } else {
                VRING_DESC_F_NEXT
            };
            (*pool).descriptors[1].next = 2;

            // Descriptor 2: Status Byte (Device Writes)
            (*pool).descriptors[2].addr = status_phys;
            (*pool).descriptors[2].len = 1;
            (*pool).descriptors[2].flags = VRING_DESC_F_WRITE;
            (*pool).descriptors[2].next = 0;

            // Update Available Ring
            let avail_idx = (*pool).avail.idx;
            (*pool).avail.ring[(avail_idx as usize) % 256] = 0; // Head descriptor = 0

            // Memory fence before incrementing avail.idx
            fence(Ordering::SeqCst);
            (*pool).avail.idx = avail_idx.wrapping_add(1);

            // Notify device of Queue 0 activity
            outw(self.io_base + VIRTIO_PCI_QUEUE_NOTIFY, 0);

            // Poll for completion (Bounded Timeout)
            let mut timeout = 1_000_000usize;
            while timeout > 0 {
                fence(Ordering::SeqCst);
                if (*pool).used.idx != self.last_used_idx {
                    break;
                }
                core::hint::spin_loop();
                timeout -= 1;
            }

            if timeout == 0 {
                return Err(BlockError::Timeout);
            }

            self.last_used_idx = (*pool).used.idx;

            // Check device status
            let status = (*pool).req_status;
            match status {
                VIRTIO_BLK_S_OK => {
                    // For reads, copy from bounce buffer to caller buffer
                    if req_type == VIRTIO_BLK_T_IN {
                        data[..byte_count].copy_from_slice(&(&(*pool).bounce_buffer)[..byte_count]);
                    }
                    Ok(())
                }
                VIRTIO_BLK_S_IOERR => Err(BlockError::HardwareError(status as u32)),
                VIRTIO_BLK_S_UNSUPP => Err(BlockError::Unsupported),
                _ => Err(BlockError::HardwareError(status as u32)),
            }
        }
    }
}

impl BlockDevice for VirtioBlockDevice {
    fn info(&self) -> BlockDeviceInfo {
        self.info
    }

    fn read_sectors(&mut self, start_sector: u64, sector_count: u32, buf: &mut [u8]) -> Result<(), BlockError> {
        validate_request(start_sector, sector_count, buf.len(), self.info.total_sectors, self.info.sector_size)?;

        let mut curr_sector = start_sector;
        let mut remaining = sector_count;
        let mut buf_offset = 0usize;

        while remaining > 0 {
            let chunk = remaining.min(8); // Transfer up to 8 sectors per batch
            let chunk_bytes = (chunk as usize) * 512;
            self.execute_transfer(
                VIRTIO_BLK_T_IN,
                curr_sector,
                chunk,
                &mut buf[buf_offset..buf_offset + chunk_bytes],
            )?;

            curr_sector += chunk as u64;
            remaining -= chunk;
            buf_offset += chunk_bytes;
        }

        Ok(())
    }

    fn write_sectors(&mut self, start_sector: u64, sector_count: u32, buf: &[u8]) -> Result<(), BlockError> {
        validate_request(start_sector, sector_count, buf.len(), self.info.total_sectors, self.info.sector_size)?;

        if self.info.read_only {
            return Err(BlockError::ReadOnly);
        }

        let mut curr_sector = start_sector;
        let mut remaining = sector_count;
        let mut buf_offset = 0usize;

        // Need mutable scratch slice for bounce transfer
        let mut scratch = [0u8; 4096];

        while remaining > 0 {
            let chunk = remaining.min(8);
            let chunk_bytes = (chunk as usize) * 512;
            scratch[..chunk_bytes].copy_from_slice(&buf[buf_offset..buf_offset + chunk_bytes]);

            self.execute_transfer(
                VIRTIO_BLK_T_OUT,
                curr_sector,
                chunk,
                &mut scratch[..chunk_bytes],
            )?;

            curr_sector += chunk as u64;
            remaining -= chunk;
            buf_offset += chunk_bytes;
        }

        Ok(())
    }
}

static VIRTIO_DEV: crate::sync::SpinLock<Option<VirtioBlockDevice>> = crate::sync::SpinLock::new(None);

/// Scan PCI registry for VirtIO Block Controller and initialize driver if present.
pub fn init() -> bool {
    let dev_opt = crate::pci::with_pci_registry(|reg| {
        reg.find_by_vendor_device(0x1af4, 0x1001).cloned()
    });

    match dev_opt {
        Some(dev) => {
            match VirtioBlockDevice::init_from_pci(&dev) {
                Ok(driver) => {
                    let mut guard = VIRTIO_DEV.lock();
                    *guard = Some(driver);
                    crate::log_info!("[VIRTIO-BLK] Driver registered successfully.");
                    true
                }
                Err(e) => {
                    crate::log_error!("[VIRTIO-BLK] Initialization failed: {}", e);
                    false
                }
            }
        }
        None => {
            crate::log_info!("[VIRTIO-BLK] No compatible VirtIO block device discovered on PCI bus.");
            false
        }
    }
}

/// Access global VirtIO block device safely.
pub fn with_block_device<F, R>(f: F) -> Option<R>
where
    F: FnOnce(&mut VirtioBlockDevice) -> R,
{
    let mut guard = VIRTIO_DEV.lock();
    guard.as_mut().map(f)
}
