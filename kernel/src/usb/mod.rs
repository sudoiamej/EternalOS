//! USB Subsystem Coordinator & Hardware Probing
//!
//! Scans PCI bus for UHCI / xHCI USB Host Controllers, initializes hardware,
//! and enumerates attached devices into the global USB registry.

pub mod descriptor;
pub mod device;
pub mod uhci;

use crate::pci::with_pci_registry;
use crate::usb::device::with_usb_registry;
use crate::usb::uhci::UhciController;

pub const PCI_CLASS_SERIAL_BUS: u8 = 0x0C;
pub const PCI_SUBCLASS_USB: u8 = 0x03;
pub const PCI_PROGIF_UHCI: u8 = 0x00;
pub const PCI_PROGIF_OHCI: u8 = 0x10;
pub const PCI_PROGIF_EHCI: u8 = 0x20;
pub const PCI_PROGIF_XHCI: u8 = 0x30;

/// USB Host Controller Metadata
#[derive(Debug, Clone, Copy)]
pub struct HostControllerInfo {
    pub bus: u8,
    pub device: u8,
    pub function: u8,
    pub vendor_id: u16,
    pub device_id: u16,
    pub prog_if: u8,
    pub is_initialized: bool,
}

static mut CONTROLLERS: [Option<HostControllerInfo>; 4] = [None; 4];
static mut CONTROLLER_COUNT: usize = 0;

pub fn get_controller_info(idx: usize) -> Option<HostControllerInfo> {
    unsafe {
        if idx < CONTROLLER_COUNT {
            CONTROLLERS[idx]
        } else {
            None
        }
    }
}

pub fn get_controller_count() -> usize {
    unsafe { CONTROLLER_COUNT }
}

/// Initialize USB Subsystem: Discover Host Controllers on PCI bus
pub fn init() {
    crate::log_info!("[USB] Scanning PCI bus for USB Host Controllers...");

    let usb_controllers = with_pci_registry(|reg| {
        let mut list = [None; 4];
        let mut cnt = 0;
        for dev in reg.devices() {
            if dev.class_code == PCI_CLASS_SERIAL_BUS && dev.subclass == PCI_SUBCLASS_USB {
                if cnt < 4 {
                    list[cnt] = Some(*dev);
                    cnt += 1;
                }
            }
        }
        (list, cnt)
    });

    let (dev_list, count) = usb_controllers;
    if count == 0 {
        crate::log_info!("[USB] No USB Host Controllers discovered on PCI bus.");
        return;
    }

    crate::log_info!("[USB] Found {} USB Host Controller(s) on PCI bus.", count);

    for i in 0..count {
        if let Some(pci_dev) = dev_list[i] {
            let mut info = HostControllerInfo {
                bus: pci_dev.bus,
                device: pci_dev.device,
                function: pci_dev.function,
                vendor_id: pci_dev.vendor_id,
                device_id: pci_dev.device_id,
                prog_if: pci_dev.prog_if,
                is_initialized: false,
            };

            match pci_dev.prog_if {
                PCI_PROGIF_UHCI => {
                    crate::log_info!(
                        "  [{:02x}:{:02x}.{:x}] UHCI USB 1.1 Controller ({:04x}:{:04x})",
                        pci_dev.bus, pci_dev.device, pci_dev.function,
                        pci_dev.vendor_id, pci_dev.device_id
                    );

                    // UHCI uses BAR4 for I/O ports
                    let bar4 = &pci_dev.bars[4];
                    if let crate::pci::BarType::Io { port } = bar4.bar_type {
                        let mut uhci = UhciController::new(pci_dev, port as u16);
                        match uhci.init_hardware() {
                            Ok(()) => {
                                info.is_initialized = true;
                                uhci.probe_ports(i as u8);
                            }
                            Err(e) => {
                                crate::log_error!("[USB] UHCI initialization failed: {}", e);
                            }
                        }
                    } else {
                        crate::log_warn!("[USB] UHCI BAR4 is not an I/O port resource.");
                    }
                }
                PCI_PROGIF_EHCI => {
                    crate::log_info!(
                        "  [{:02x}:{:02x}.{:x}] EHCI USB 2.0 Controller ({:04x}:{:04x}) - Companion to UHCI",
                        pci_dev.bus, pci_dev.device, pci_dev.function,
                        pci_dev.vendor_id, pci_dev.device_id
                    );
                }
                PCI_PROGIF_XHCI => {
                    crate::log_info!(
                        "  [{:02x}:{:02x}.{:x}] xHCI USB 3.x Controller ({:04x}:{:04x})",
                        pci_dev.bus, pci_dev.device, pci_dev.function,
                        pci_dev.vendor_id, pci_dev.device_id
                    );
                }
                _ => {
                    crate::log_info!(
                        "  [{:02x}:{:02x}.{:x}] Unknown USB Controller ProgIF 0x{:02x}",
                        pci_dev.bus, pci_dev.device, pci_dev.function, pci_dev.prog_if
                    );
                }
            }

            unsafe {
                if CONTROLLER_COUNT < 4 {
                    CONTROLLERS[CONTROLLER_COUNT] = Some(info);
                    CONTROLLER_COUNT += 1;
                }
            }
        }
    }

    let dev_count = with_usb_registry(|reg| reg.count);
    crate::log_info!("[USB] Enumeration complete: {} active USB device(s) registered.", dev_count);
}
