use crate::net::wire::EthernetAddress;
use crate::net::{NetStats, NetworkDevice};
use crate::pci::{read_config_u16, write_config_u16, PciDeviceFunction};
use crate::vm::{phys_to_virt, translate_addr, PhysAddr, VirtAddr};
use core::sync::atomic::{fence, Ordering};

// Intel 82540EM PCI Device IDs
pub const E1000_VENDOR_INTEL: u16 = 0x8086;
pub const E1000_DEV_82540EM: u16 = 0x100E;
pub const E1000_DEV_82545EM: u16 = 0x100F;
pub const E1000_DEV_82543GC: u16 = 0x1004;

// E1000 MMIO Register Offsets
pub const REG_CTRL: usize = 0x00000;
pub const REG_STATUS: usize = 0x00008;
pub const REG_EECD: usize = 0x00010;
pub const REG_IMS: usize = 0x000D0;
pub const REG_IMC: usize = 0x000D8;
pub const REG_RCTL: usize = 0x00100;
pub const REG_RDBAL: usize = 0x02800;
pub const REG_RDBAH: usize = 0x02804;
pub const REG_RDLEN: usize = 0x02808;
pub const REG_RDH: usize = 0x02810;
pub const REG_RDT: usize = 0x02818;
pub const REG_TCTL: usize = 0x00400;
pub const REG_TDBAL: usize = 0x03800;
pub const REG_TDBAH: usize = 0x03804;
pub const REG_TDLEN: usize = 0x03808;
pub const REG_TDH: usize = 0x03810;
pub const REG_TDT: usize = 0x03818;
pub const REG_MTA: usize = 0x05200;
pub const REG_RAL0: usize = 0x05400;
pub const REG_RAH0: usize = 0x05404;

// Control register bits
pub const CTRL_SLU: u32 = 1 << 6; // Set Link Up
pub const CTRL_RST: u32 = 1 << 26; // Device Reset

// Receive Control (RCTL) bits
pub const RCTL_EN: u32 = 1 << 1; // Receiver Enable
pub const RCTL_SBP: u32 = 1 << 2; // Store Bad Packets
pub const RCTL_UPE: u32 = 1 << 3; // Unicast Promiscuous
pub const RCTL_MPE: u32 = 1 << 4; // Multicast Promiscuous
pub const RCTL_BAM: u32 = 1 << 15; // Broadcast Accept Mode
pub const RCTL_BSIZE_2048: u32 = 0 << 16; // Buffer size 2048 bytes
pub const RCTL_SECRC: u32 = 1 << 26; // Strip Ethernet CRC

// Transmit Control (TCTL) bits
pub const TCTL_EN: u32 = 1 << 1; // Transmit Enable
pub const TCTL_PSP: u32 = 1 << 3; // Pad Short Packets
pub const TCTL_CT_SHIFT: u32 = 4; // Collision Threshold
pub const TCTL_COLD_SHIFT: u32 = 12; // Collision Distance

// Transmit Descriptor Command Bits
pub const TX_CMD_EOP: u8 = 1 << 0; // End Of Packet
pub const TX_CMD_IFCS: u8 = 1 << 1; // Insert FCS / CRC
pub const TX_CMD_RS: u8 = 1 << 3; // Report Status
pub const TX_STATUS_DD: u8 = 1 << 0; // Descriptor Done

// Receive Descriptor Status Bits
pub const RX_STATUS_DD: u8 = 1 << 0; // Descriptor Done
pub const RX_STATUS_EOP: u8 = 1 << 1; // End of Packet

pub const NUM_RX_DESC: usize = 32;
pub const NUM_TX_DESC: usize = 32;
pub const RX_BUFFER_SIZE: usize = 2048;
pub const TX_BUFFER_SIZE: usize = 2048;

/// Receive Descriptor (16 bytes)
#[repr(C, align(16))]
#[derive(Debug, Clone, Copy, Default)]
pub struct E1000RxDesc {
    pub addr: u64,
    pub length: u16,
    pub checksum: u16,
    pub status: u8,
    pub errors: u8,
    pub special: u16,
}

/// Transmit Descriptor (16 bytes)
#[repr(C, align(16))]
#[derive(Debug, Clone, Copy, Default)]
pub struct E1000TxDesc {
    pub addr: u64,
    pub length: u16,
    pub cso: u8,
    pub cmd: u8,
    pub status: u8,
    pub css: u8,
    pub special: u16,
}

/// Statically allocated, page-aligned DMA memory pool for E1000 rings and buffers
#[repr(C, align(4096))]
pub struct E1000DmaPool {
    pub rx_descs: [E1000RxDesc; NUM_RX_DESC],
    pub tx_descs: [E1000TxDesc; NUM_TX_DESC],
    pub rx_buffers: [[u8; RX_BUFFER_SIZE]; NUM_RX_DESC],
    pub tx_buffers: [[u8; TX_BUFFER_SIZE]; NUM_TX_DESC],
}

static mut E1000_DMA: E1000DmaPool = E1000DmaPool {
    rx_descs: [E1000RxDesc {
        addr: 0,
        length: 0,
        checksum: 0,
        status: 0,
        errors: 0,
        special: 0,
    }; NUM_RX_DESC],
    tx_descs: [E1000TxDesc {
        addr: 0,
        length: 0,
        cso: 0,
        cmd: 0,
        status: 0,
        css: 0,
        special: 0,
    }; NUM_TX_DESC],
    rx_buffers: [[0; RX_BUFFER_SIZE]; NUM_RX_DESC],
    tx_buffers: [[0; TX_BUFFER_SIZE]; NUM_TX_DESC],
};

pub struct E1000Driver {
    pub mmio_base: usize,
    pub mac: EthernetAddress,
    pub rx_cur: usize,
    pub tx_cur: usize,
    pub stats: NetStats,
}

impl E1000Driver {
    #[inline]
    unsafe fn write_reg(&self, offset: usize, value: u32) {
        let ptr = (self.mmio_base + offset) as *mut u32;
        unsafe {
            core::ptr::write_volatile(ptr, value);
        }
    }

    #[inline]
    unsafe fn read_reg(&self, offset: usize) -> u32 {
        let ptr = (self.mmio_base + offset) as *const u32;
        unsafe {
            core::ptr::read_volatile(ptr)
        }
    }

    /// Read 16-bit word from EEPROM (Microwire / SPI interface via EECD)
    unsafe fn read_eeprom(&self, addr: u8) -> u16 {
        // E1000 EEPROM Read (EERD register) is at 0x00014
        let eerd_reg = (self.mmio_base + 0x00014) as *mut u32;
        let query = (1 << 0) | ((addr as u32) << 8); // Start bit (1) + address
        unsafe {
            core::ptr::write_volatile(eerd_reg, query);
        }

        let mut timeout = 100_000;
        while timeout > 0 {
            let val = unsafe { core::ptr::read_volatile(eerd_reg) };
            if (val & (1 << 4)) != 0 {
                // Done bit
                return (val >> 16) as u16;
            }
            core::hint::spin_loop();
            timeout -= 1;
        }
        0
    }

    /// Read hardware MAC address from EEPROM or RAL0/RAH0 registers
    fn read_mac_address(&self) -> EthernetAddress {
        // First try reading from RAL0/RAH0
        let ral0 = unsafe { self.read_reg(REG_RAL0) };
        let rah0 = unsafe { self.read_reg(REG_RAH0) };

        if ral0 != 0 || (rah0 & 0xFFFF) != 0 {
            let mut mac = [0u8; 6];
            mac[0] = (ral0 & 0xFF) as u8;
            mac[1] = ((ral0 >> 8) & 0xFF) as u8;
            mac[2] = ((ral0 >> 16) & 0xFF) as u8;
            mac[3] = ((ral0 >> 24) & 0xFF) as u8;
            mac[4] = (rah0 & 0xFF) as u8;
            mac[5] = ((rah0 >> 8) & 0xFF) as u8;
            if mac != [0; 6] && mac != [0xFF; 6] {
                return EthernetAddress(mac);
            }
        }

        // Fallback to EEPROM reading
        let word0 = unsafe { self.read_eeprom(0) };
        let word1 = unsafe { self.read_eeprom(1) };
        let word2 = unsafe { self.read_eeprom(2) };

        let mut mac = [0u8; 6];
        mac[0] = (word0 & 0xFF) as u8;
        mac[1] = (word0 >> 8) as u8;
        mac[2] = (word1 & 0xFF) as u8;
        mac[3] = (word1 >> 8) as u8;
        mac[4] = (word2 & 0xFF) as u8;
        mac[5] = (word2 >> 8) as u8;

        if mac == [0; 6] || mac == [0xFF; 6] {
            // Default QEMU fallback MAC if uninitialized
            EthernetAddress([0x52, 0x54, 0x00, 0x12, 0x34, 0x56])
        } else {
            EthernetAddress(mac)
        }
    }

    /// Initialize the E1000 controller from a detected PCI function
    pub fn probe_and_init(pci_dev: &PciDeviceFunction) -> Result<Self, &'static str> {
        crate::log_info!(
            "[E1000] Probing device at {:02x}:{:02x}.{:x} (Vendor: 0x{:04x}, Device: 0x{:04x})",
            pci_dev.bus,
            pci_dev.device,
            pci_dev.function,
            pci_dev.vendor_id,
            pci_dev.device_id
        );

        // 1. Find Memory BAR0
        let bar0 = pci_dev.bars.iter().find(|b| match b.bar_type {
            crate::pci::BarType::Memory32 { .. } | crate::pci::BarType::Memory64 { .. } => true,
            _ => false,
        }).ok_or("E1000: No Memory BAR found on PCI device")?;

        let bar_phys = match bar0.bar_type {
            crate::pci::BarType::Memory32 { address, .. } => address as u64,
            crate::pci::BarType::Memory64 { address, .. } => address,
            _ => return Err("Invalid BAR type"),
        };

        crate::log_info!("[E1000] Physical MMIO BAR0 Base: 0x{:016x}", bar_phys);

        // 2. Ensure MMIO space is mapped in the kernel page table
        let mmio_virt = match phys_to_virt(PhysAddr::new(bar_phys)) {
            Some(v) => v,
            None => return Err("E1000: Failed to convert MMIO physical address to virtual via HHDM"),
        };

        // E1000 MMIO region size is 128 KiB (32 pages of 4096 bytes)
        let num_pages = 32;
        let flags = crate::vm::PageTableFlags::WRITABLE | crate::vm::PageTableFlags::NO_CACHE;
        for i in 0..num_pages {
            let page_vaddr = VirtAddr::new(mmio_virt.as_u64() + (i * 4096));
            let frame_paddr = PhysAddr::new(bar_phys + (i * 4096));
            if crate::vm::translate_addr(page_vaddr).is_err() {
                unsafe {
                    if let Err(e) = crate::vm::map_page(page_vaddr, frame_paddr, flags) {
                        crate::log_warn!("[E1000] Warning mapping MMIO page {}: {}", i, e);
                    }
                }
            }
        }

        let mmio_base = mmio_virt.as_u64() as usize;

        // 2. Enable Bus Mastering and Memory Space in PCI Command register
        let cmd = read_config_u16(pci_dev.bus, pci_dev.device, pci_dev.function, 0x04);
        write_config_u16(pci_dev.bus, pci_dev.device, pci_dev.function, 0x04, cmd | 0x0006);

        let mut driver = Self {
            mmio_base,
            mac: EthernetAddress::ZERO,
            rx_cur: 0,
            tx_cur: 0,
            stats: NetStats::default(),
        };

        // 3. Reset device
        unsafe {
            let ctrl = driver.read_reg(REG_CTRL);
            driver.write_reg(REG_CTRL, ctrl | CTRL_RST);
            for _ in 0..10_000 {
                core::hint::spin_loop();
            }
            // Set Link Up
            driver.write_reg(REG_CTRL, (driver.read_reg(REG_CTRL) | CTRL_SLU) & !CTRL_RST);
        }

        // 4. Disable interrupts (polling mode initially)
        unsafe {
            driver.write_reg(REG_IMC, 0xFFFF_FFFF);
        }

        // 5. Read MAC address
        driver.mac = driver.read_mac_address();
        crate::log_info!(
            "[E1000] MAC Address: {:02x}:{:02x}:{:02x}:{:02x}:{:02x}:{:02x}",
            driver.mac.0[0],
            driver.mac.0[1],
            driver.mac.0[2],
            driver.mac.0[3],
            driver.mac.0[4],
            driver.mac.0[5]
        );

        // 6. Initialize Multicast Table Array (MTA) to 0
        for i in 0..128 {
            unsafe {
                driver.write_reg(REG_MTA + (i * 4), 0);
            }
        }

        // 7. Initialize RX and TX DMA rings
        driver.init_dma()?;

        crate::log_info!("[E1000] Controller initialized and ready.");
        Ok(driver)
    }

    fn init_dma(&mut self) -> Result<(), &'static str> {
        let pool_virt = &raw const E1000_DMA as u64;
        let pool_phys = translate_addr(VirtAddr::new(pool_virt))?.as_u64();

        // Calculate descriptor ring physical addresses
        let rx_descs_phys = pool_phys + core::mem::offset_of!(E1000DmaPool, rx_descs) as u64;
        let tx_descs_phys = pool_phys + core::mem::offset_of!(E1000DmaPool, tx_descs) as u64;

        unsafe {
            // Setup RX descriptors with physical buffer addresses
            for i in 0..NUM_RX_DESC {
                let buf_virt = &raw const E1000_DMA.rx_buffers[i] as u64;
                let buf_phys = translate_addr(VirtAddr::new(buf_virt))?.as_u64();
                E1000_DMA.rx_descs[i].addr = buf_phys;
                E1000_DMA.rx_descs[i].status = 0;
            }

            // Setup TX descriptors with physical buffer addresses
            for i in 0..NUM_TX_DESC {
                let buf_virt = &raw const E1000_DMA.tx_buffers[i] as u64;
                let buf_phys = translate_addr(VirtAddr::new(buf_virt))?.as_u64();
                E1000_DMA.tx_descs[i].addr = buf_phys;
                E1000_DMA.tx_descs[i].cmd = 0;
                E1000_DMA.tx_descs[i].status = TX_STATUS_DD; // Initially idle
            }

            // Program RX ring registers
            self.write_reg(REG_RDBAL, (rx_descs_phys & 0xFFFF_FFFF) as u32);
            self.write_reg(REG_RDBAH, (rx_descs_phys >> 32) as u32);
            self.write_reg(REG_RDLEN, (NUM_RX_DESC * core::mem::size_of::<E1000RxDesc>()) as u32);
            self.write_reg(REG_RDH, 0);
            self.write_reg(REG_RDT, (NUM_RX_DESC - 1) as u32);

            // Program TX ring registers
            self.write_reg(REG_TDBAL, (tx_descs_phys & 0xFFFF_FFFF) as u32);
            self.write_reg(REG_TDBAH, (tx_descs_phys >> 32) as u32);
            self.write_reg(REG_TDLEN, (NUM_TX_DESC * core::mem::size_of::<E1000TxDesc>()) as u32);
            self.write_reg(REG_TDH, 0);
            self.write_reg(REG_TDT, 0);

            // Enable Receiver
            let rctl = RCTL_EN | RCTL_BAM | RCTL_BSIZE_2048 | RCTL_SECRC | RCTL_UPE | RCTL_MPE;
            self.write_reg(REG_RCTL, rctl);

            // Enable Transmitter
            let tctl = TCTL_EN | TCTL_PSP | (15 << TCTL_CT_SHIFT) | (64 << TCTL_COLD_SHIFT);
            self.write_reg(REG_TCTL, tctl);
        }

        self.rx_cur = 0;
        self.tx_cur = 0;

        Ok(())
    }
}

impl NetworkDevice for E1000Driver {
    fn mac_address(&self) -> EthernetAddress {
        self.mac
    }

    fn is_link_up(&self) -> bool {
        let status = unsafe { self.read_reg(REG_STATUS) };
        (status & (1 << 1)) != 0
    }

    fn stats(&self) -> NetStats {
        self.stats
    }

    fn transmit(&mut self, frame: &[u8]) -> Result<(), &'static str> {
        if frame.len() > TX_BUFFER_SIZE {
            self.stats.tx_dropped += 1;
            return Err("Frame length exceeds TX buffer size");
        }

        let idx = self.tx_cur;
        unsafe {
            // Check if descriptor is available (DD bit set)
            let desc = &mut E1000_DMA.tx_descs[idx];
            if (desc.status & TX_STATUS_DD) == 0 {
                self.stats.tx_dropped += 1;
                return Err("E1000: TX ring full");
            }

            // Copy frame data into DMA bounce buffer
            E1000_DMA.tx_buffers[idx][..frame.len()].copy_from_slice(frame);

            desc.length = frame.len() as u16;
            desc.cmd = TX_CMD_EOP | TX_CMD_IFCS | TX_CMD_RS;
            desc.status = 0;

            fence(Ordering::SeqCst);

            // Advance Transmit Tail register to notify controller
            self.tx_cur = (idx + 1) % NUM_TX_DESC;
            self.write_reg(REG_TDT, self.tx_cur as u32);
        }

        self.stats.tx_packets += 1;
        self.stats.tx_bytes += frame.len() as u64;

        Ok(())
    }

    fn receive(&mut self, buf: &mut [u8]) -> Result<Option<usize>, &'static str> {
        let idx = self.rx_cur;
        unsafe {
            let desc = &mut E1000_DMA.rx_descs[idx];
            if (desc.status & RX_STATUS_DD) == 0 {
                return Ok(None); // No packet pending
            }

            let len = desc.length as usize;
            if len > buf.len() {
                self.stats.rx_dropped += 1;
                desc.status = 0;
                self.write_reg(REG_RDT, idx as u32);
                self.rx_cur = (idx + 1) % NUM_RX_DESC;
                return Err("Receive buffer too small for frame");
            }

            buf[..len].copy_from_slice(&E1000_DMA.rx_buffers[idx][..len]);

            // Re-arm descriptor
            desc.status = 0;
            fence(Ordering::SeqCst);

            // Update tail register to return descriptor to hardware
            self.write_reg(REG_RDT, idx as u32);
            self.rx_cur = (idx + 1) % NUM_RX_DESC;

            self.stats.rx_packets += 1;
            self.stats.rx_bytes += len as u64;

            Ok(Some(len))
        }
    }
}
