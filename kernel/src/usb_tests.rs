//! Automated test suite for the USB Subsystem and Descriptors
//!
//! Validates:
//! 1. Setup packet building (GET_DESCRIPTOR, SET_ADDRESS, SET_CONFIGURATION)
//! 2. Device descriptor parsing & bounds checking
//! 3. Configuration & Interface descriptor parsing
//! 4. Endpoint descriptor parsing
//! 5. UTF-16LE String descriptor parsing
//! 6. USB Device Registry state tracking

use crate::usb::descriptor::*;
use crate::usb::device::{UsbDevice, UsbRegistry, UsbSpeed, UsbDeviceStatus};
use crate::{log_info, println};

pub fn run_usb_tests() {
    log_info!("====================================================");
    log_info!("       RUNNING AUTOMATED TEST SUITE: USB SUBSYSTEM   ");
    log_info!("====================================================");

    test_setup_packets();
    test_device_descriptor_parsing();
    test_config_and_interface_descriptors();
    test_endpoint_descriptor();
    test_string_descriptor();
    test_usb_registry();

    log_info!("====================================================");
    log_info!("     ALL USB SUBSYSTEM TESTS PASSED CLEANLY!         ");
    log_info!("====================================================");
    println!(">>> USB TEST PASS: Setup packets, Descriptors & Registry verified.");
}

fn test_setup_packets() {
    log_info!("[TEST 1/6] Testing USB Setup Packet Construction...");

    let pkt = SetupPacket::get_descriptor(DESC_TYPE_DEVICE, 0, 18);
    let pkt_req_type = pkt.request_type;
    let pkt_req = pkt.request;
    let pkt_val = pkt.value;
    let pkt_len = pkt.length;
    assert_eq!(pkt_req_type, 0x80);
    assert_eq!(pkt_req, REQ_GET_DESCRIPTOR);
    assert_eq!(pkt_val, (DESC_TYPE_DEVICE as u16) << 8);
    assert_eq!(pkt_len, 18);

    let set_addr = SetupPacket::set_address(5);
    let addr_req_type = set_addr.request_type;
    let addr_req = set_addr.request;
    let addr_val = set_addr.value;
    assert_eq!(addr_req_type, 0x00);
    assert_eq!(addr_req, REQ_SET_ADDRESS);
    assert_eq!(addr_val, 5);

    let set_cfg = SetupPacket::set_configuration(1);
    let cfg_req = set_cfg.request;
    let cfg_val = set_cfg.value;
    assert_eq!(cfg_req, REQ_SET_CONFIGURATION);
    assert_eq!(cfg_val, 1);

    log_info!("  [PASS] Setup packet construction verified.");
}

fn test_device_descriptor_parsing() {
    log_info!("[TEST 2/6] Testing USB Device Descriptor Parsing...");

    // Standard 18-byte QEMU USB Keyboard descriptor
    let raw: [u8; 18] = [
        18,   // bLength
        1,    // bDescriptorType (Device)
        0x10, 0x01, // bcdUSB (1.10)
        0x00, // bDeviceClass (Per-interface)
        0x00, // bDeviceSubClass
        0x00, // bDeviceProtocol
        8,    // bMaxPacketSize0 (8 bytes)
        0x27, 0x06, // idVendor (0x0627)
        0x01, 0x00, // idProduct (0x0001)
        0x00, 0x01, // bcdDevice (1.00)
        1,    // iManufacturer
        2,    // iProduct
        0,    // iSerialNumber
        1,    // bNumConfigurations
    ];

    let desc = DeviceDescriptor::parse(&raw).expect("Failed to parse valid device descriptor");
    assert_eq!(desc.b_length, 18);
    assert_eq!(desc.b_descriptor_type, 1);
    assert_eq!(desc.bcd_usb, 0x0110);
    assert_eq!(desc.b_max_packet_size0, 8);
    assert_eq!(desc.id_vendor, 0x0627);
    assert_eq!(desc.id_product, 0x0001);
    assert_eq!(desc.b_num_configurations, 1);
    assert_eq!(desc.class_name(), "Interface Specific (Defined per Interface)");

    // Too short buffer check
    assert!(DeviceDescriptor::parse(&raw[..17]).is_err());

    log_info!("  [PASS] Device Descriptor parsing and validation verified.");
}

fn test_config_and_interface_descriptors() {
    log_info!("[TEST 3/6] Testing Configuration & Interface Descriptors...");

    let raw_cfg: [u8; 9] = [
        9,    // bLength
        2,    // bDescriptorType (Configuration)
        34, 0, // wTotalLength (34 bytes)
        1,    // bNumInterfaces
        1,    // bConfigurationValue
        0,    // iConfiguration
        0xA0, // bmAttributes (Bus powered, Remote Wakeup)
        50,   // bMaxPower (100 mA)
    ];

    let cfg = ConfigurationDescriptor::parse(&raw_cfg).expect("Failed to parse config descriptor");
    assert_eq!(cfg.b_length, 9);
    assert_eq!(cfg.w_total_length, 34);
    assert_eq!(cfg.b_num_interfaces, 1);
    assert_eq!(cfg.b_max_power, 50);

    let raw_iface: [u8; 9] = [
        9,    // bLength
        4,    // bDescriptorType (Interface)
        0,    // bInterfaceNumber
        0,    // bAlternateSetting
        1,    // bNumEndpoints
        3,    // bInterfaceClass (HID)
        1,    // bInterfaceSubClass (Boot Interface)
        1,    // bInterfaceProtocol (Keyboard)
        0,    // iInterface
    ];

    let iface = InterfaceDescriptor::parse(&raw_iface).expect("Failed to parse iface descriptor");
    assert_eq!(iface.b_interface_class, 3);
    assert_eq!(iface.b_num_endpoints, 1);

    log_info!("  [PASS] Configuration and Interface Descriptors verified.");
}

fn test_endpoint_descriptor() {
    log_info!("[TEST 4/6] Testing Endpoint Descriptor Parsing...");

    let raw_ep: [u8; 7] = [
        7,    // bLength
        5,    // bDescriptorType (Endpoint)
        0x81, // bEndpointAddress (EP 1 IN)
        0x03, // bmAttributes (Interrupt)
        8, 0, // wMaxPacketSize (8 bytes)
        10,   // bInterval (10 ms)
    ];

    let ep = EndpointDescriptor::parse(&raw_ep).expect("Failed to parse endpoint descriptor");
    assert_eq!(ep.b_length, 7);
    assert_eq!(ep.b_endpoint_address, 0x81);
    assert_eq!(ep.w_max_packet_size, 8);
    assert_eq!(ep.b_interval, 10);

    log_info!("  [PASS] Endpoint Descriptor verified.");
}

fn test_string_descriptor() {
    log_info!("[TEST 5/6] Testing UTF-16LE String Descriptor Decoding...");

    // "QEMU" in UTF-16LE: 'Q' = 0x0051, 'E' = 0x0045, 'M' = 0x004D, 'U' = 0x0055
    let raw_str: [u8; 10] = [
        10, // bLength
        3,  // bDescriptorType (String)
        0x51, 0x00,
        0x45, 0x00,
        0x4D, 0x00,
        0x55, 0x00,
    ];

    let mut ascii_buf = [0u8; 16];
    let decoded = parse_string_descriptor(&raw_str, &mut ascii_buf).expect("String decode failed");
    assert_eq!(decoded, "QEMU");

    log_info!("  [PASS] UTF-16LE to ASCII String Descriptor decoding verified.");
}

fn test_usb_registry() {
    log_info!("[TEST 6/6] Testing USB Registry Management...");

    let mut registry = UsbRegistry::new();
    assert_eq!(registry.count, 0);

    let mut dev = UsbDevice::empty();
    dev.controller_id = 0;
    dev.port_num = 1;
    dev.address = 1;
    dev.speed = UsbSpeed::LowSpeed;
    dev.descriptor.id_vendor = 0x0627;
    dev.descriptor.id_product = 0x0001;
    dev.status = UsbDeviceStatus::Enumerated;

    registry.add(dev).expect("Failed to add USB device to registry");
    assert_eq!(registry.count, 1);
    assert_eq!(registry.devices[0].descriptor.id_vendor, 0x0627);
    assert_eq!(registry.devices[0].speed, UsbSpeed::LowSpeed);

    log_info!("  [PASS] USB Registry operations verified.");
}
