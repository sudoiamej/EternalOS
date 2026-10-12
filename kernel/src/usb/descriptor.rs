//! Standard USB 2.0 / 3.0 Descriptors & Control Request Primitives
//!
//! Provides strict length-bounded parsers for:
//! - Device Descriptors (18 bytes)
//! - Configuration Descriptors (9 bytes)
//! - Interface Descriptors (9 bytes)
//! - Endpoint Descriptors (7 bytes)
//! - String Descriptors (UTF-16LE to ASCII conversion)

// Standard USB Descriptor Types
pub const DESC_TYPE_DEVICE: u8 = 1;
pub const DESC_TYPE_CONFIGURATION: u8 = 2;
pub const DESC_TYPE_STRING: u8 = 3;
pub const DESC_TYPE_INTERFACE: u8 = 4;
pub const DESC_TYPE_ENDPOINT: u8 = 5;

// Standard USB Request Codes
pub const REQ_GET_STATUS: u8 = 0;
pub const REQ_CLEAR_FEATURE: u8 = 1;
pub const REQ_SET_FEATURE: u8 = 3;
pub const REQ_SET_ADDRESS: u8 = 5;
pub const REQ_GET_DESCRIPTOR: u8 = 6;
pub const REQ_SET_DESCRIPTOR: u8 = 7;
pub const REQ_GET_CONFIGURATION: u8 = 8;
pub const REQ_SET_CONFIGURATION: u8 = 9;

// USB Device Classes
pub const CLASS_PER_INTERFACE: u8 = 0x00;
pub const CLASS_AUDIO: u8 = 0x01;
pub const CLASS_COMMUNICATIONS: u8 = 0x02;
pub const CLASS_HID: u8 = 0x03;
pub const CLASS_PHYSICAL: u8 = 0x05;
pub const CLASS_IMAGE: u8 = 0x06;
pub const CLASS_PRINTER: u8 = 0x07;
pub const CLASS_MASS_STORAGE: u8 = 0x08;
pub const CLASS_HUB: u8 = 0x09;
pub const CLASS_CDC_DATA: u8 = 0x0A;
pub const CLASS_SMART_CARD: u8 = 0x0B;
pub const CLASS_VIDEO: u8 = 0x0E;
pub const CLASS_HEALTHCARE: u8 = 0x0F;
pub const CLASS_WIRELESS: u8 = 0xE0;
pub const CLASS_MISCELLANEOUS: u8 = 0xEF;
pub const CLASS_VENDOR_SPECIFIC: u8 = 0xFF;

/// Standard 8-byte USB Setup Packet (bmRequestType, bRequest, wValue, wIndex, wLength)
#[repr(C, packed)]
#[derive(Debug, Clone, Copy, Default)]
pub struct SetupPacket {
    pub request_type: u8,
    pub request: u8,
    pub value: u16,
    pub index: u16,
    pub length: u16,
}

impl SetupPacket {
    pub const fn new(request_type: u8, request: u8, value: u16, index: u16, length: u16) -> Self {
        Self {
            request_type,
            request,
            value,
            index,
            length,
        }
    }

    /// Standard GET_DESCRIPTOR request (e.g. Device Descriptor)
    pub const fn get_descriptor(desc_type: u8, desc_index: u8, length: u16) -> Self {
        let value = ((desc_type as u16) << 8) | (desc_index as u16);
        Self {
            request_type: 0x80, // Device-to-Host, Standard, Device recipient
            request: REQ_GET_DESCRIPTOR,
            value,
            index: 0,
            length,
        }
    }

    /// Standard SET_ADDRESS request
    pub const fn set_address(address: u8) -> Self {
        Self {
            request_type: 0x00, // Host-to-Device, Standard, Device recipient
            request: REQ_SET_ADDRESS,
            value: address as u16,
            index: 0,
            length: 0,
        }
    }

    /// Standard SET_CONFIGURATION request
    pub const fn set_configuration(config_val: u8) -> Self {
        Self {
            request_type: 0x00,
            request: REQ_SET_CONFIGURATION,
            value: config_val as u16,
            index: 0,
            length: 0,
        }
    }
}

/// USB Standard Device Descriptor (18 bytes)
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DeviceDescriptor {
    pub b_length: u8,
    pub b_descriptor_type: u8,
    pub bcd_usb: u16,
    pub b_device_class: u8,
    pub b_device_subclass: u8,
    pub b_device_protocol: u8,
    pub b_max_packet_size0: u8,
    pub id_vendor: u16,
    pub id_product: u16,
    pub bcd_device: u16,
    pub i_manufacturer: u8,
    pub i_product: u8,
    pub i_serial_number: u8,
    pub b_num_configurations: u8,
}

impl DeviceDescriptor {
    pub const SIZE: usize = 18;

    pub fn parse(buf: &[u8]) -> Result<Self, &'static str> {
        if buf.len() < Self::SIZE {
            return Err("Buffer too short for Device Descriptor (need 18 bytes)");
        }

        let b_length = buf[0];
        let b_descriptor_type = buf[1];

        if b_length < Self::SIZE as u8 {
            return Err("Device Descriptor bLength < 18");
        }
        if b_descriptor_type != DESC_TYPE_DEVICE {
            return Err("Not a Device Descriptor (type != 1)");
        }

        let bcd_usb = u16::from_le_bytes([buf[2], buf[3]]);
        let b_device_class = buf[4];
        let b_device_subclass = buf[5];
        let b_device_protocol = buf[6];
        let b_max_packet_size0 = buf[7];
        let id_vendor = u16::from_le_bytes([buf[8], buf[9]]);
        let id_product = u16::from_le_bytes([buf[10], buf[11]]);
        let bcd_device = u16::from_le_bytes([buf[12], buf[13]]);
        let i_manufacturer = buf[14];
        let i_product = buf[15];
        let i_serial_number = buf[16];
        let b_num_configurations = buf[17];

        Ok(Self {
            b_length,
            b_descriptor_type,
            bcd_usb,
            b_device_class,
            b_device_subclass,
            b_device_protocol,
            b_max_packet_size0,
            id_vendor,
            id_product,
            bcd_device,
            i_manufacturer,
            i_product,
            i_serial_number,
            b_num_configurations,
        })
    }

    pub fn class_name(&self) -> &'static str {
        match self.b_device_class {
            CLASS_PER_INTERFACE => "Interface Specific (Defined per Interface)",
            CLASS_AUDIO => "Audio Device",
            CLASS_COMMUNICATIONS => "Communications / CDC",
            CLASS_HID => "Human Interface Device (HID)",
            CLASS_PHYSICAL => "Physical Interface",
            CLASS_IMAGE => "Still Image Capture",
            CLASS_PRINTER => "Printer Device",
            CLASS_MASS_STORAGE => "Mass Storage Controller",
            CLASS_HUB => "USB Hub",
            CLASS_CDC_DATA => "CDC Data",
            CLASS_SMART_CARD => "Smart Card Reader",
            CLASS_VIDEO => "Video Device",
            CLASS_HEALTHCARE => "Personal Healthcare",
            CLASS_WIRELESS => "Wireless Controller (Bluetooth/WiFi)",
            CLASS_MISCELLANEOUS => "Miscellaneous Device",
            CLASS_VENDOR_SPECIFIC => "Vendor Specific",
            _ => "Unknown Class",
        }
    }
}

/// USB Standard Configuration Descriptor Header (9 bytes)
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ConfigurationDescriptor {
    pub b_length: u8,
    pub b_descriptor_type: u8,
    pub w_total_length: u16,
    pub b_num_interfaces: u8,
    pub b_configuration_value: u8,
    pub i_configuration: u8,
    pub bm_attributes: u8,
    pub b_max_power: u8,
}

impl ConfigurationDescriptor {
    pub const SIZE: usize = 9;

    pub fn parse(buf: &[u8]) -> Result<Self, &'static str> {
        if buf.len() < Self::SIZE {
            return Err("Buffer too short for Configuration Descriptor (need 9 bytes)");
        }
        let b_length = buf[0];
        let b_descriptor_type = buf[1];
        if b_length < Self::SIZE as u8 {
            return Err("Configuration Descriptor bLength < 9");
        }
        if b_descriptor_type != DESC_TYPE_CONFIGURATION {
            return Err("Not a Configuration Descriptor (type != 2)");
        }
        let w_total_length = u16::from_le_bytes([buf[2], buf[3]]);
        let b_num_interfaces = buf[4];
        let b_configuration_value = buf[5];
        let i_configuration = buf[6];
        let bm_attributes = buf[7];
        let b_max_power = buf[8];

        Ok(Self {
            b_length,
            b_descriptor_type,
            w_total_length,
            b_num_interfaces,
            b_configuration_value,
            i_configuration,
            bm_attributes,
            b_max_power,
        })
    }
}

/// USB Standard Interface Descriptor (9 bytes)
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct InterfaceDescriptor {
    pub b_length: u8,
    pub b_descriptor_type: u8,
    pub b_interface_number: u8,
    pub b_alternate_setting: u8,
    pub b_num_endpoints: u8,
    pub b_interface_class: u8,
    pub b_interface_subclass: u8,
    pub b_interface_protocol: u8,
    pub i_interface: u8,
}

impl InterfaceDescriptor {
    pub const SIZE: usize = 9;

    pub fn parse(buf: &[u8]) -> Result<Self, &'static str> {
        if buf.len() < Self::SIZE {
            return Err("Buffer too short for Interface Descriptor");
        }
        if buf[0] < Self::SIZE as u8 || buf[1] != DESC_TYPE_INTERFACE {
            return Err("Invalid Interface Descriptor header");
        }
        Ok(Self {
            b_length: buf[0],
            b_descriptor_type: buf[1],
            b_interface_number: buf[2],
            b_alternate_setting: buf[3],
            b_num_endpoints: buf[4],
            b_interface_class: buf[5],
            b_interface_subclass: buf[6],
            b_interface_protocol: buf[7],
            i_interface: buf[8],
        })
    }
}

/// USB Standard Endpoint Descriptor (7 bytes)
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct EndpointDescriptor {
    pub b_length: u8,
    pub b_descriptor_type: u8,
    pub b_endpoint_address: u8,
    pub bm_attributes: u8,
    pub w_max_packet_size: u16,
    pub b_interval: u8,
}

impl EndpointDescriptor {
    pub const SIZE: usize = 7;

    pub fn parse(buf: &[u8]) -> Result<Self, &'static str> {
        if buf.len() < Self::SIZE {
            return Err("Buffer too short for Endpoint Descriptor");
        }
        if buf[0] < Self::SIZE as u8 || buf[1] != DESC_TYPE_ENDPOINT {
            return Err("Invalid Endpoint Descriptor header");
        }
        let w_max_packet_size = u16::from_le_bytes([buf[4], buf[5]]);
        Ok(Self {
            b_length: buf[0],
            b_descriptor_type: buf[1],
            b_endpoint_address: buf[2],
            bm_attributes: buf[3],
            w_max_packet_size,
            b_interval: buf[6],
        })
    }
}

/// Decode USB String Descriptor (UTF-16LE) to bounded ASCII slice
pub fn parse_string_descriptor<'a>(buf: &[u8], out: &'a mut [u8]) -> Result<&'a str, &'static str> {
    if buf.len() < 2 {
        return Err("String descriptor too short");
    }
    let b_len = buf[0] as usize;
    if b_len > buf.len() || buf[1] != DESC_TYPE_STRING {
        return Err("Invalid String Descriptor header");
    }

    let utf16_bytes = &buf[2..b_len];
    let num_chars = utf16_bytes.len() / 2;
    let max_copy = num_chars.min(out.len());

    for i in 0..max_copy {
        let code = u16::from_le_bytes([utf16_bytes[i * 2], utf16_bytes[i * 2 + 1]]);
        out[i] = if code < 128 && code >= 32 { code as u8 } else { b'?' };
    }

    core::str::from_utf8(&out[..max_copy]).map_err(|_| "Failed to format ASCII string")
}
