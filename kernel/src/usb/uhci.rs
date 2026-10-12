//! Universal Host Controller Interface (UHCI) Driver (USB 1.1)
//!
//! Handles:
//! - PCI class discovery (0x0C:0x03, ProgIF 0x00)
//! - I/O Space BAR4 configuration
//! - Frame List & Queue Head (QH) / Transfer Descriptor (TD) schedule
//! - Root Hub Port Reset and Connection Detection
//! - Control Endpoint 0 transfers (GET_DESCRIPTOR)

use crate::pci::PciDeviceFunction;
use crate::usb::descriptor::{DeviceDescriptor, SetupPacket, DESC_TYPE_DEVICE};
use crate::usb::device::{with_usb_registry_mut, UsbDevice, UsbDeviceStatus, UsbSpeed};
use crate::vm::{translate_addr, VirtAddr};
use core::arch::asm;
use core::sync::atomic::{fence, Ordering};

// UHCI I/O Register Offsets (from BAR4 base port)
pub const USBCMD: u16 = 0x00; // USB Command
pub const USBSTS: u16 = 0x02; // USB Status
pub const USBINTR: u16 = 0x04; // USB Interrupt Enable
pub const FRNUM: u16 = 0x06; // Frame Number
pub const FRBASEADD: u16 = 0x08; // Frame List Base Address
pub const SOFMOD: u16 = 0x0C; // Start of Frame Modify
pub const PORTSC1: u16 = 0x10; // Port 1 Status/Control
pub const PORTSC2: u16 = 0x12; // Port 2 Status/Control

// USBCMD Register Bits
pub const USBCMD_RS: u16 = 1 << 0; // Run/Stop (1 = Run)
pub const USBCMD_HCRESET: u16 = 1 << 1; // Host Controller Reset
pub const USBCMD_GRESET: u16 = 1 << 2; // Global Reset
pub const USBCMD_MAXP: u16 = 1 << 7; // Max Packet (1 = 64 bytes)

// USBSTS Register Bits
pub const USBSTS_HCHALTED: u16 = 1 << 5; // HCHalted

// PORTSC Register Bits
pub const PORTSC_CCS: u16 = 1 << 0; // Current Connect Status
pub const PORTSC_CSC: u16 = 1 << 1; // Connect Status Change (write 1 to clear)
pub const PORTSC_PE: u16 = 1 << 2; // Port Enabled/Disabled
pub const PORTSC_PEC: u16 = 1 << 3; // Port Enable/Disable Change
pub const PORTSC_LSDA: u16 = 1 << 8; // Low Speed Device Attached (1 = Low Speed)
pub const PORTSC_PR: u16 = 1 << 9; // Port Reset

// UHCI Transfer Descriptor (TD) - 32 bytes aligned to 16 bytes
#[repr(C, align(16))]
#[derive(Debug, Clone, Copy, Default)]
pub struct UhciTd {
    pub link_ptr: u32,  // Link to next TD or QH (bit 0 = Terminate, bit 1 = QH/TD)
    pub ctrl_status: u32,// Control & Status (Active bit 23, Error counters, etc.)
    pub token: u32,     // Packet Token (PID, Device Address, Endpoint, MaxLen)
    pub buffer_ptr: u32,// Physical address of data buffer
    pub reserved: [u32; 4],
}

// UHCI Queue Head (QH) - 16 bytes aligned to 16 bytes
#[repr(C, align(16))]
#[derive(Debug, Clone, Copy, Default)]
pub struct UhciQh {
    pub head_link: u32,    // Next Queue Head (bit 0 = Terminate, bit 1 = QH)
    pub element_link: u32, // Next TD or QH element
    pub reserved: [u32; 2],
}

pub const TD_CTRL_ACTIVE: u32 = 1 << 23;
pub const TD_CTRL_IOC: u32 = 1 << 24;
pub const TD_CTRL_LS: u32 = 1 << 26; // Low Speed device flag
pub const TD_PID_SETUP: u32 = 0x2D;
pub const TD_PID_IN: u32 = 0x69;
pub const TD_PID_OUT: u32 = 0xE1;

/// Statically allocated, page-aligned DMA memory pool for UHCI
#[repr(C, align(4096))]
pub struct UhciDmaPool {
    pub frame_list: [u32; 1024], // 1024 physical pointers for 1ms frames
    pub qh: UhciQh,
    pub setup_td: UhciTd,
    pub data_td: UhciTd,
    pub status_td: UhciTd,
    pub setup_buf: [u8; 8],
    pub data_buf: [u8; 64],
}

static mut UHCI_DMA: UhciDmaPool = UhciDmaPool {
    frame_list: [1; 1024], // 1 = Terminate
    qh: UhciQh { head_link: 1, element_link: 1, reserved: [0; 2] },
    setup_td: UhciTd { link_ptr: 1, ctrl_status: 0, token: 0, buffer_ptr: 0, reserved: [0; 4] },
    data_td: UhciTd { link_ptr: 1, ctrl_status: 0, token: 0, buffer_ptr: 0, reserved: [0; 4] },
    status_td: UhciTd { link_ptr: 1, ctrl_status: 0, token: 0, buffer_ptr: 0, reserved: [0; 4] },
    setup_buf: [0; 8],
    data_buf: [0; 64],
};

#[inline]
unsafe fn inw(port: u16) -> u16 {
    let val: u16;
    unsafe {
        asm!("in ax, dx", in("dx") port, out("ax") val, options(nomem, nostack, preserves_flags));
    }
    val
}

#[inline]
unsafe fn outw(port: u16, val: u16) {
    unsafe {
        asm!("out dx, ax", in("dx") port, in("ax") val, options(nomem, nostack, preserves_flags));
    }
}

#[inline]
unsafe fn outl(port: u16, val: u32) {
    unsafe {
        asm!("out dx, eax", in("dx") port, in("eax") val, options(nomem, nostack, preserves_flags));
    }
}

/// UHCI Host Controller Driver
pub struct UhciController {
    pub io_base: u16,
    pub pci: PciDeviceFunction,
}

impl UhciController {
    pub fn new(pci: PciDeviceFunction, io_base: u16) -> Self {
        Self { io_base, pci }
    }

    /// Read 16-bit register
    pub unsafe fn read_reg(&self, reg: u16) -> u16 {
        unsafe { inw(self.io_base + reg) }
    }

    /// Write 16-bit register
    pub unsafe fn write_reg(&self, reg: u16, val: u16) {
        unsafe { outw(self.io_base + reg, val) };
    }

    /// Initialize controller hardware
    pub fn init_hardware(&mut self) -> Result<(), &'static str> {
        unsafe {
            // 1. Enable Bus Mastering and I/O Space in PCI Command register
            let mut cmd = crate::pci::read_config_u16(self.pci.bus, self.pci.device, self.pci.function, 0x04);
            cmd |= 0x0005; // Bit 0 = I/O Space, Bit 2 = Bus Master
            crate::pci::write_config_u16(self.pci.bus, self.pci.device, self.pci.function, 0x04, cmd);

            // 2. Issue Host Controller Reset (HCRESET)
            self.write_reg(USBCMD, USBCMD_HCRESET);
            let mut reset_ticks = 0;
            while (self.read_reg(USBCMD) & USBCMD_HCRESET) != 0 {
                reset_ticks += 1;
                if reset_ticks > 100_000 {
                    return Err("UHCI: Host controller reset timed out");
                }
                core::hint::spin_loop();
            }

            // 3. Clear all pending status flags (write 1 to clear)
            self.write_reg(USBSTS, 0x003F);

            // 4. Disable interrupts (polling used for initial control transfers)
            self.write_reg(USBINTR, 0x0000);

            // 5. Setup Frame List and Queue Head in DMA memory
            let qh_virt = &raw const UHCI_DMA.qh as u64;
            let qh_phys = translate_addr(VirtAddr::new(qh_virt))
                .map_err(|_| "Failed to translate UHCI QH virtual address")?
                .as_u64() as u32;

            // Frame list points all 1024 frames to Queue Head (bit 1 = 1 indicates QH)
            for i in 0..1024 {
                UHCI_DMA.frame_list[i] = qh_phys | 0x02;
            }
            UHCI_DMA.qh.head_link = 1; // Terminate next QH
            UHCI_DMA.qh.element_link = 1; // Terminate element link

            let fl_virt = &raw const UHCI_DMA.frame_list as u64;
            let fl_phys = translate_addr(VirtAddr::new(fl_virt))
                .map_err(|_| "Failed to translate UHCI Frame List virtual address")?
                .as_u64() as u32;

            self.write_reg(FRNUM, 0);
            outl(self.io_base + FRBASEADD, fl_phys);

            // 6. Start the controller (USBCMD_RS | USBCMD_MAXP)
            self.write_reg(USBCMD, USBCMD_RS | USBCMD_MAXP);

            let status = self.read_reg(USBSTS);
            if (status & USBSTS_HCHALTED) != 0 {
                return Err("UHCI: Controller remained halted after start");
            }

            crate::log_info!("[UHCI] Controller initialized on I/O base 0x{:04x}", self.io_base);
            Ok(())
        }
    }

    /// Reset Root Hub Port and check if a device is connected
    pub fn reset_port(&self, port_reg: u16) -> Result<Option<UsbSpeed>, &'static str> {
        unsafe {
            let status = self.read_reg(port_reg);
            if (status & PORTSC_CCS) == 0 {
                return Ok(None); // No device connected
            }

            let is_low_speed = (status & PORTSC_LSDA) != 0;
            let speed = if is_low_speed { UsbSpeed::LowSpeed } else { UsbSpeed::FullSpeed };

            // Assert Port Reset (PR) for at least 50ms
            self.write_reg(port_reg, PORTSC_PR);
            let start = crate::timer::get_ticks();
            while crate::timer::get_ticks().saturating_sub(start) < 6 { // 60ms
                core::hint::spin_loop();
            }

            // De-assert Reset
            self.write_reg(port_reg, 0);

            // Wait 10ms for port enable
            let start2 = crate::timer::get_ticks();
            while crate::timer::get_ticks().saturating_sub(start2) < 2 { // 20ms
                core::hint::spin_loop();
            }

            // Enable port (PE)
            self.write_reg(port_reg, PORTSC_PE);

            let post_status = self.read_reg(port_reg);
            if (post_status & PORTSC_PE) == 0 {
                // If not auto-enabled, write enable again
                self.write_reg(port_reg, post_status | PORTSC_PE);
            }

            Ok(Some(speed))
        }
    }

    /// Perform a USB Control Transfer on Endpoint 0 to retrieve Device Descriptor
    pub fn get_device_descriptor(&mut self, is_low_speed: bool) -> Result<DeviceDescriptor, &'static str> {
        unsafe {
            let setup_virt = &raw const UHCI_DMA.setup_buf as u64;
            let setup_phys = translate_addr(VirtAddr::new(setup_virt))
                .map_err(|_| "Failed to translate setup buffer")?
                .as_u64() as u32;

            let data_virt = &raw const UHCI_DMA.data_buf as u64;
            let data_phys = translate_addr(VirtAddr::new(data_virt))
                .map_err(|_| "Failed to translate data buffer")?
                .as_u64() as u32;

            let td_setup_virt = &raw const UHCI_DMA.setup_td as u64;
            let td_setup_phys = translate_addr(VirtAddr::new(td_setup_virt))
                .map_err(|_| "Failed to translate setup TD")?
                .as_u64() as u32;

            let td_data_virt = &raw const UHCI_DMA.data_td as u64;
            let td_data_phys = translate_addr(VirtAddr::new(td_data_virt))
                .map_err(|_| "Failed to translate data TD")?
                .as_u64() as u32;

            let td_status_virt = &raw const UHCI_DMA.status_td as u64;
            let td_status_phys = translate_addr(VirtAddr::new(td_status_virt))
                .map_err(|_| "Failed to translate status TD")?
                .as_u64() as u32;

            // Prepare 8-byte SETUP packet: GET_DESCRIPTOR (Device, 18 bytes)
            let setup_pkt = SetupPacket::get_descriptor(DESC_TYPE_DEVICE, 0, 18);
            let pkt_bytes = core::slice::from_raw_parts(&setup_pkt as *const _ as *const u8, 8);
            let setup_buf_ptr = &raw mut UHCI_DMA.setup_buf;
            core::ptr::copy_nonoverlapping(pkt_bytes.as_ptr(), (*setup_buf_ptr).as_mut_ptr(), 8);
            let data_buf_ptr = &raw mut UHCI_DMA.data_buf;
            (*data_buf_ptr).fill(0);

            let ls_flag = if is_low_speed { TD_CTRL_LS } else { 0 };

            // 1. SETUP TD: PID 0x2D, Addr 0, Ep 0, Len 8 (7 in token), Data0
            let setup_td_ptr = &raw mut UHCI_DMA.setup_td;
            *setup_td_ptr = UhciTd {
                link_ptr: td_data_phys, // Point to Data TD
                ctrl_status: TD_CTRL_ACTIVE | (3 << 27) | ls_flag, // 3 error retries
                token: TD_PID_SETUP | (7 << 21), // Length 8 = (8-1) << 21
                buffer_ptr: setup_phys,
                reserved: [0; 4],
            };

            // 2. DATA IN TD: PID 0x69, Addr 0, Ep 0, Len 18 (17 in token), Data1 (bit 19)
            let data_td_ptr = &raw mut UHCI_DMA.data_td;
            *data_td_ptr = UhciTd {
                link_ptr: td_status_phys, // Point to Status TD
                ctrl_status: TD_CTRL_ACTIVE | (3 << 27) | ls_flag,
                token: TD_PID_IN | (1 << 19) | (17 << 21), // Data1, Length 18
                buffer_ptr: data_phys,
                reserved: [0; 4],
            };

            // 3. STATUS OUT TD: PID 0xE1, Addr 0, Ep 0, Len 0 (0x7FF in token), Data1 (bit 19), IOC
            let status_td_ptr = &raw mut UHCI_DMA.status_td;
            *status_td_ptr = UhciTd {
                link_ptr: 1, // Terminate
                ctrl_status: TD_CTRL_ACTIVE | TD_CTRL_IOC | (3 << 27) | ls_flag,
                token: TD_PID_OUT | (1 << 19) | (0x7FF << 21), // Null packet (len 0)
                buffer_ptr: 0,
                reserved: [0; 4],
            };

            fence(Ordering::SeqCst);

            // Hook first TD into Queue Head element link
            let qh_ptr = &raw mut UHCI_DMA.qh;
            (*qh_ptr).element_link = td_setup_phys;

            // Poll for transfer completion (timeout 500ms)
            let start = crate::timer::get_ticks();
            while crate::timer::get_ticks().saturating_sub(start) < 50 {
                fence(Ordering::SeqCst);
                let status_ctrl = (*status_td_ptr).ctrl_status;
                if (status_ctrl & TD_CTRL_ACTIVE) == 0 {
                    // Transfer complete! Check error bits
                    if (status_ctrl & 0x7E0000) != 0 {
                        // Unlink QH
                        (*qh_ptr).element_link = 1;
                        return Err("UHCI: Transfer completed with error flags");
                    }
                    break;
                }
                core::hint::spin_loop();
            }

            // Unlink element from QH
            (*qh_ptr).element_link = 1;
            fence(Ordering::SeqCst);

            if ((*status_td_ptr).ctrl_status & TD_CTRL_ACTIVE) != 0 {
                return Err("UHCI: Control transfer timed out waiting for status TD");
            }

            // Parse returned Device Descriptor
            let data_slice = core::slice::from_raw_parts((*data_buf_ptr).as_ptr(), 18);
            DeviceDescriptor::parse(data_slice)
        }
    }

    /// Enumerate attached devices across all Root Hub ports
    pub fn probe_ports(&mut self, controller_id: u8) {
        let ports = [(1u8, PORTSC1), (2u8, PORTSC2)];
        for (port_num, port_reg) in ports {
            match self.reset_port(port_reg) {
                Ok(Some(speed)) => {
                    let is_low_speed = speed == UsbSpeed::LowSpeed;
                    crate::log_info!(
                        "[UHCI] Port {} connected (Speed: {}). Requesting Device Descriptor...",
                        port_num,
                        speed
                    );

                    match self.get_device_descriptor(is_low_speed) {
                        Ok(desc) => {
                            crate::log_info!(
                                "[UHCI] Port {} enumerated: {:04x}:{:04x} (Class: 0x{:02x} -> {})",
                                port_num,
                                desc.id_vendor,
                                desc.id_product,
                                desc.b_device_class,
                                desc.class_name()
                            );

                            let mut dev = UsbDevice::empty();
                            dev.controller_id = controller_id;
                            dev.port_num = port_num;
                            dev.address = 0; // Default address
                            dev.speed = speed;
                            dev.descriptor = desc;
                            dev.status = UsbDeviceStatus::Enumerated;

                            let _ = with_usb_registry_mut(|reg| reg.add(dev));
                        }
                        Err(e) => {
                            crate::log_warn!("[UHCI] Failed to get descriptor on port {}: {}", port_num, e);
                        }
                    }
                }
                Ok(None) => {
                    // No device connected on port
                }
                Err(e) => {
                    crate::log_warn!("[UHCI] Port {} error: {}", port_num, e);
                }
            }
        }
    }
}
