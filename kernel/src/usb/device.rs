//! USB Device Registry
//!
//! Tracks enumerated USB devices across discovered host controllers.

use crate::usb::descriptor::DeviceDescriptor;
use core::fmt;

pub const MAX_USB_DEVICES: usize = 16;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UsbSpeed {
    FullSpeed, // 12 Mbps (USB 1.1)
    LowSpeed,  // 1.5 Mbps (USB 1.0)
    HighSpeed, // 480 Mbps (USB 2.0)
    SuperSpeed,// 5 Gbps (USB 3.0)
}

impl fmt::Display for UsbSpeed {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            UsbSpeed::FullSpeed => write!(f, "FullSpeed (12 Mbps)"),
            UsbSpeed::LowSpeed => write!(f, "LowSpeed (1.5 Mbps)"),
            UsbSpeed::HighSpeed => write!(f, "HighSpeed (480 Mbps)"),
            UsbSpeed::SuperSpeed => write!(f, "SuperSpeed (5 Gbps)"),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UsbDeviceStatus {
    PortConnected,
    ResetComplete,
    Enumerated,
    Configured,
    Error(&'static str),
}

/// Description of an enumerated USB device
#[derive(Debug, Clone, Copy)]
pub struct UsbDevice {
    pub controller_id: u8,
    pub port_num: u8,
    pub address: u8,
    pub speed: UsbSpeed,
    pub descriptor: DeviceDescriptor,
    pub status: UsbDeviceStatus,
    pub manufacturer: [u8; 32],
    pub manufacturer_len: usize,
    pub product: [u8; 32],
    pub product_len: usize,
}

impl UsbDevice {
    pub const fn empty() -> Self {
        Self {
            controller_id: 0,
            port_num: 0,
            address: 0,
            speed: UsbSpeed::FullSpeed,
            descriptor: DeviceDescriptor {
                b_length: 0,
                b_descriptor_type: 0,
                bcd_usb: 0,
                b_device_class: 0,
                b_device_subclass: 0,
                b_device_protocol: 0,
                b_max_packet_size0: 0,
                id_vendor: 0,
                id_product: 0,
                bcd_device: 0,
                i_manufacturer: 0,
                i_product: 0,
                i_serial_number: 0,
                b_num_configurations: 0,
            },
            status: UsbDeviceStatus::PortConnected,
            manufacturer: [0; 32],
            manufacturer_len: 0,
            product: [0; 32],
            product_len: 0,
        }
    }

    pub fn manufacturer_str(&self) -> &str {
        core::str::from_utf8(&self.manufacturer[..self.manufacturer_len]).unwrap_or("")
    }

    pub fn product_str(&self) -> &str {
        core::str::from_utf8(&self.product[..self.product_len]).unwrap_or("")
    }
}

pub struct UsbRegistry {
    pub devices: [UsbDevice; MAX_USB_DEVICES],
    pub count: usize,
}

impl UsbRegistry {
    pub const fn new() -> Self {
        Self {
            devices: [UsbDevice::empty(); MAX_USB_DEVICES],
            count: 0,
        }
    }

    pub fn add(&mut self, dev: UsbDevice) -> Result<(), &'static str> {
        if self.count >= MAX_USB_DEVICES {
            return Err("USB Device Registry full");
        }
        self.devices[self.count] = dev;
        self.count += 1;
        Ok(())
    }

    pub fn clear(&mut self) {
        self.count = 0;
    }
}

pub static USB_REGISTRY: crate::sync::SpinLock<UsbRegistry> = crate::sync::SpinLock::new(UsbRegistry::new());

pub fn with_usb_registry<F, R>(f: F) -> R
where
    F: FnOnce(&UsbRegistry) -> R,
{
    let guard = USB_REGISTRY.lock();
    f(&guard)
}

pub fn with_usb_registry_mut<F, R>(f: F) -> R
where
    F: FnOnce(&mut UsbRegistry) -> R,
{
    let mut guard = USB_REGISTRY.lock();
    f(&mut guard)
}
