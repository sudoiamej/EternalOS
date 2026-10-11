use core::arch::asm;
use crate::sync::SpinLock;

/// Port 0xCF8: PCI Configuration Address Register
const PCI_CONFIG_ADDRESS: u16 = 0xCF8;
/// Port 0xCFC: PCI Configuration Data Register
const PCI_CONFIG_DATA: u16 = 0xCFC;

/// Global spinlock protecting access to Configuration Mechanism #1 ports 0xCF8 and 0xCFC.
/// On our single-core BSP, SpinLock disables CPU interrupts (cli) while held,
/// preventing interrupt handler / scheduler preemption from interleaving port accesses.
static PCI_LOCK: SpinLock<()> = SpinLock::new(());

/// Encapsulated safe I/O port operations for PCI configuration
#[inline]
unsafe fn outl(port: u16, val: u32) {
    unsafe {
        asm!("out dx, eax", in("dx") port, in("eax") val, options(nomem, nostack, preserves_flags));
    }
}

#[inline]
unsafe fn inl(port: u16) -> u32 {
    let val: u32;
    unsafe {
        asm!("in eax, dx", in("dx") port, out("eax") val, options(nomem, nostack, preserves_flags));
    }
    val
}

#[inline]
unsafe fn inw(port: u16) -> u16 {
    let val: u16;
    unsafe {
        asm!("in ax, dx", in("dx") port, out("ax") val, options(nomem, nostack, preserves_flags));
    }
    val
}

#[inline]
unsafe fn inb(port: u16) -> u8 {
    let val: u8;
    unsafe {
        asm!("in al, dx", in("dx") port, out("al") val, options(nomem, nostack, preserves_flags));
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
unsafe fn outb(port: u16, val: u8) {
    unsafe {
        asm!("out dx, al", in("dx") port, in("al") val, options(nomem, nostack, preserves_flags));
    }
}

/// Compute the 32-bit Configuration Mechanism #1 address for a given (bus, device, function, offset).
/// Returns None if bus > 255, device > 31, function > 7, or offset > 255.
#[inline]
pub const fn make_pci_address(bus: u8, device: u8, function: u8, offset: u8) -> Option<u32> {
    if device > 31 || function > 7 {
        return None;
    }
    // Enable bit (bit 31) = 1, Bus [23:16], Device [15:11], Function [10:8], Register Offset [7:2]
    let addr = 0x8000_0000u32
        | ((bus as u32) << 16)
        | (((device & 0x1F) as u32) << 11)
        | (((function & 0x07) as u32) << 8)
        | ((offset as u32) & 0xFC);
    Some(addr)
}

/// Read a 32-bit dword from PCI configuration space at an aligned 4-byte offset.
/// Returns 0xFFFF_FFFF if parameters are out of bounds or offset is unaligned.
pub fn read_config_u32(bus: u8, device: u8, function: u8, offset: u8) -> u32 {
    if (offset & 0x03) != 0 {
        return 0xFFFF_FFFF;
    }
    let pci_addr = match make_pci_address(bus, device, function, offset) {
        Some(a) => a,
        None => return 0xFFFF_FFFF,
    };

    let _guard = PCI_LOCK.lock();
    unsafe {
        outl(PCI_CONFIG_ADDRESS, pci_addr);
        inl(PCI_CONFIG_DATA)
    }
}

/// Read a 16-bit word from PCI configuration space at an aligned 2-byte offset.
/// Returns 0xFFFF if parameters are out of bounds or offset is unaligned.
pub fn read_config_u16(bus: u8, device: u8, function: u8, offset: u8) -> u16 {
    if (offset & 0x01) != 0 {
        return 0xFFFF;
    }
    let pci_addr = match make_pci_address(bus, device, function, offset) {
        Some(a) => a,
        None => return 0xFFFF,
    };

    let _guard = PCI_LOCK.lock();
    unsafe {
        outl(PCI_CONFIG_ADDRESS, pci_addr);
        inw(PCI_CONFIG_DATA + ((offset & 0x02) as u16))
    }
}

/// Read an 8-bit byte from PCI configuration space at any offset (0..255).
pub fn read_config_u8(bus: u8, device: u8, function: u8, offset: u8) -> u8 {
    let pci_addr = match make_pci_address(bus, device, function, offset) {
        Some(a) => a,
        None => return 0xFF,
    };

    let _guard = PCI_LOCK.lock();
    unsafe {
        outl(PCI_CONFIG_ADDRESS, pci_addr);
        inb(PCI_CONFIG_DATA + ((offset & 0x03) as u16))
    }
}

/// Write a 32-bit dword to PCI configuration space at an aligned 4-byte offset.
pub fn write_config_u32(bus: u8, device: u8, function: u8, offset: u8, val: u32) {
    if (offset & 0x03) != 0 {
        return;
    }
    let pci_addr = match make_pci_address(bus, device, function, offset) {
        Some(a) => a,
        None => return,
    };

    let _guard = PCI_LOCK.lock();
    unsafe {
        outl(PCI_CONFIG_ADDRESS, pci_addr);
        outl(PCI_CONFIG_DATA, val);
    }
}

/// Write a 16-bit word to PCI configuration space at an aligned 2-byte offset.
pub fn write_config_u16(bus: u8, device: u8, function: u8, offset: u8, val: u16) {
    if (offset & 0x01) != 0 {
        return;
    }
    let pci_addr = match make_pci_address(bus, device, function, offset) {
        Some(a) => a,
        None => return,
    };

    let _guard = PCI_LOCK.lock();
    unsafe {
        outl(PCI_CONFIG_ADDRESS, pci_addr);
        outw(PCI_CONFIG_DATA + ((offset & 0x02) as u16), val);
    }
}

/// Write an 8-bit byte to PCI configuration space at any offset (0..255).
pub fn write_config_u8(bus: u8, device: u8, function: u8, offset: u8, val: u8) {
    let pci_addr = match make_pci_address(bus, device, function, offset) {
        Some(a) => a,
        None => return,
    };

    let _guard = PCI_LOCK.lock();
    unsafe {
        outl(PCI_CONFIG_ADDRESS, pci_addr);
        outb(PCI_CONFIG_DATA + ((offset & 0x03) as u16), val);
    }
}

// -----------------------------------------------------------------------------
// Data Structures for PCI Discovery & Resource Reporting
// -----------------------------------------------------------------------------

pub const MAX_PCI_FUNCTIONS: usize = 64;
pub const MAX_CAPABILITIES: usize = 16;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BarType {
    None,
    Io { port: u32 },
    Memory32 { address: u32, prefetchable: bool },
    Memory64 { address: u64, prefetchable: bool },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PciBar {
    pub bar_index: u8,
    pub bar_type: BarType,
    pub raw_low: u32,
    pub raw_high: u32,
}

impl PciBar {
    pub const fn empty(index: u8) -> Self {
        Self {
            bar_index: index,
            bar_type: BarType::None,
            raw_low: 0,
            raw_high: 0,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PciCapability {
    pub id: u8,
    pub offset: u8,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PciHeaderType {
    StandardDevice,
    PciToPciBridge,
    CardBusBridge,
    Unknown(u8),
}

impl From<u8> for PciHeaderType {
    fn from(raw: u8) -> Self {
        match raw & 0x7F {
            0x00 => PciHeaderType::StandardDevice,
            0x01 => PciHeaderType::PciToPciBridge,
            0x02 => PciHeaderType::CardBusBridge,
            other => PciHeaderType::Unknown(other),
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub struct PciDeviceFunction {
    pub bus: u8,
    pub device: u8,
    pub function: u8,
    pub vendor_id: u16,
    pub device_id: u16,
    pub command: u16,
    pub status: u16,
    pub revision_id: u8,
    pub prog_if: u8,
    pub subclass: u8,
    pub class_code: u8,
    pub header_type: PciHeaderType,
    pub multifunction: bool,
    pub bars: [PciBar; 6],
    pub bar_count: u8,
    pub capabilities: [PciCapability; MAX_CAPABILITIES],
    pub cap_count: u8,
    pub secondary_bus: Option<u8>,
    pub subordinate_bus: Option<u8>,
}

impl PciDeviceFunction {
    pub const fn empty() -> Self {
        Self {
            bus: 0,
            device: 0,
            function: 0,
            vendor_id: 0xFFFF,
            device_id: 0xFFFF,
            command: 0,
            status: 0,
            revision_id: 0,
            prog_if: 0,
            subclass: 0,
            class_code: 0,
            header_type: PciHeaderType::StandardDevice,
            multifunction: false,
            bars: [
                PciBar::empty(0),
                PciBar::empty(1),
                PciBar::empty(2),
                PciBar::empty(3),
                PciBar::empty(4),
                PciBar::empty(5),
            ],
            bar_count: 0,
            capabilities: [PciCapability { id: 0, offset: 0 }; MAX_CAPABILITIES],
            cap_count: 0,
            secondary_bus: None,
            subordinate_bus: None,
        }
    }

    pub fn class_name(&self) -> &'static str {
        match self.class_code {
            0x00 => "Unclassified / Legacy",
            0x01 => "Mass Storage Controller",
            0x02 => "Network Controller",
            0x03 => "Display Controller",
            0x04 => "Multimedia Device",
            0x05 => "Memory Controller",
            0x06 => "Bridge Device",
            0x07 => "Simple Communications Controller",
            0x08 => "Base System Peripheral",
            0x09 => "Input Device Controller",
            0x0A => "Docking Station",
            0x0B => "Processor",
            0x0C => "Serial Bus Controller",
            0x0D => "Wireless Controller",
            0x0E => "Intelligent Controller",
            0x0F => "Satellite Communications Controller",
            0x10 => "Encryption Controller",
            0x11 => "Signal Processing Controller",
            _ => "Unknown Class",
        }
    }
}

// -----------------------------------------------------------------------------
// Pure Parsing Helpers (Unit-testable without hardware access)
// -----------------------------------------------------------------------------

/// Parse a BAR from raw low and high dwords.
/// Returns (PciBar, consumed_two_slots).
pub fn parse_bar(index: u8, raw_low: u32, raw_high: u32) -> (PciBar, bool) {
    if raw_low == 0 || raw_low == 0xFFFF_FFFF {
        return (
            PciBar {
                bar_index: index,
                bar_type: BarType::None,
                raw_low,
                raw_high,
            },
            false,
        );
    }

    if (raw_low & 0x01) == 1 {
        // I/O space BAR
        let port = raw_low & !0x03;
        (
            PciBar {
                bar_index: index,
                bar_type: BarType::Io { port },
                raw_low,
                raw_high: 0,
            },
            false,
        )
    } else {
        // Memory space BAR
        let mem_type = (raw_low >> 1) & 0x03;
        let prefetchable = ((raw_low >> 3) & 0x01) == 1;

        match mem_type {
            0x00 => {
                // 32-bit memory BAR
                let address = raw_low & !0x0F;
                (
                    PciBar {
                        bar_index: index,
                        bar_type: BarType::Memory32 {
                            address,
                            prefetchable,
                        },
                        raw_low,
                        raw_high: 0,
                    },
                    false,
                )
            }
            0x02 => {
                // 64-bit memory BAR: combines low and high 32-bit registers
                let address = ((raw_high as u64) << 32) | ((raw_low & !0x0F) as u64);
                (
                    PciBar {
                        bar_index: index,
                        bar_type: BarType::Memory64 {
                            address,
                            prefetchable,
                        },
                        raw_low,
                        raw_high,
                    },
                    true,
                )
            }
            _ => {
                // Reserved or invalid encoding (e.g. 1MB below 1MB legacy)
                (
                    PciBar {
                        bar_index: index,
                        bar_type: BarType::None,
                        raw_low,
                        raw_high,
                    },
                    false,
                )
            }
        }
    }
}

// -----------------------------------------------------------------------------
// PCI Device Registry
// -----------------------------------------------------------------------------

pub struct PciRegistry {
    entries: [PciDeviceFunction; MAX_PCI_FUNCTIONS],
    count: usize,
    overflow_count: usize,
}

impl PciRegistry {
    pub const fn new() -> Self {
        Self {
            entries: [PciDeviceFunction::empty(); MAX_PCI_FUNCTIONS],
            count: 0,
            overflow_count: 0,
        }
    }

    pub fn add(&mut self, dev: PciDeviceFunction) -> bool {
        if self.count < MAX_PCI_FUNCTIONS {
            self.entries[self.count] = dev;
            self.count += 1;
            true
        } else {
            self.overflow_count += 1;
            false
        }
    }

    pub fn devices(&self) -> &[PciDeviceFunction] {
        &self.entries[..self.count]
    }

    pub fn count(&self) -> usize {
        self.count
    }

    pub fn overflow_count(&self) -> usize {
        self.overflow_count
    }

    pub fn find_by_vendor_device(&self, vendor_id: u16, device_id: u16) -> Option<&PciDeviceFunction> {
        self.devices()
            .iter()
            .find(|d| d.vendor_id == vendor_id && d.device_id == device_id)
    }

    pub fn find_by_class(&self, class_code: u8, subclass: u8) -> Option<&PciDeviceFunction> {
        self.devices()
            .iter()
            .find(|d| d.class_code == class_code && d.subclass == subclass)
    }
}

static PCI_REGISTRY: SpinLock<PciRegistry> = SpinLock::new(PciRegistry::new());

/// Retrieve a copy or closure-based view of discovered devices, or access count.
pub fn with_pci_registry<F, R>(f: F) -> R
where
    F: FnOnce(&PciRegistry) -> R,
{
    let guard = PCI_REGISTRY.lock();
    f(&guard)
}

/// Retrieve the count of discovered devices
pub fn get_device_count() -> usize {
    let guard = PCI_REGISTRY.lock();
    guard.count()
}

/// Retrieve the global registry's overflow count if more than MAX_PCI_FUNCTIONS were found.
pub fn get_overflow_count() -> usize {
    let guard = PCI_REGISTRY.lock();
    guard.overflow_count()
}

// -----------------------------------------------------------------------------
// Capability List Traversal
// -----------------------------------------------------------------------------

fn read_capabilities(
    bus: u8,
    dev: u8,
    func: u8,
    status: u16,
    cap_ptr_offset: u8,
    caps: &mut [PciCapability; MAX_CAPABILITIES],
) -> u8 {
    // Bit 4 of Status register: Capabilities List present
    if (status & (1 << 4)) == 0 {
        return 0;
    }

    let mut count = 0usize;
    let mut offset = read_config_u8(bus, dev, func, cap_ptr_offset) & !0x03;

    // Cycle detection & boundary checks
    let mut visited = [0u8; MAX_CAPABILITIES];

    while offset >= 0x40 && offset <= 0xFC && count < MAX_CAPABILITIES {
        // Check for cycle / repeat offset
        if visited[..count].contains(&offset) {
            crate::log_warn!(
                "[PCI] Cycle detected in capabilities list at offset 0x{:02x} for {:02x}:{:02x}.{:x}",
                offset,
                bus,
                dev,
                func
            );
            break;
        }
        visited[count] = offset;

        let cap_id = read_config_u8(bus, dev, func, offset);
        let next_offset = read_config_u8(bus, dev, func, offset + 1) & !0x03;

        caps[count] = PciCapability {
            id: cap_id,
            offset,
        };
        count += 1;

        if next_offset == 0 {
            break;
        }
        offset = next_offset;
    }

    count as u8
}

// -----------------------------------------------------------------------------
// Function & Device Inspection
// -----------------------------------------------------------------------------

fn inspect_function(bus: u8, dev: u8, func: u8) -> Option<PciDeviceFunction> {
    let vendor_id = read_config_u16(bus, dev, func, 0x00);
    // 0xFFFF indicates nonexistent function
    if vendor_id == 0xFFFF || vendor_id == 0x0000 {
        return None;
    }

    let device_id = read_config_u16(bus, dev, func, 0x02);
    let command = read_config_u16(bus, dev, func, 0x04);
    let status = read_config_u16(bus, dev, func, 0x06);

    let revision_id = read_config_u8(bus, dev, func, 0x08);
    let prog_if = read_config_u8(bus, dev, func, 0x09);
    let subclass = read_config_u8(bus, dev, func, 0x0A);
    let class_code = read_config_u8(bus, dev, func, 0x0B);

    let header_type_raw = read_config_u8(bus, dev, func, 0x0E);
    let multifunction = (header_type_raw & 0x80) != 0;
    let header_type = PciHeaderType::from(header_type_raw);

    let mut function_desc = PciDeviceFunction {
        bus,
        device: dev,
        function: func,
        vendor_id,
        device_id,
        command,
        status,
        revision_id,
        prog_if,
        subclass,
        class_code,
        header_type,
        multifunction,
        bars: [
            PciBar::empty(0),
            PciBar::empty(1),
            PciBar::empty(2),
            PciBar::empty(3),
            PciBar::empty(4),
            PciBar::empty(5),
        ],
        bar_count: 0,
        capabilities: [PciCapability { id: 0, offset: 0 }; MAX_CAPABILITIES],
        cap_count: 0,
        secondary_bus: None,
        subordinate_bus: None,
    };

    // Parse BARs according to header type without modifying hardware registers
    match header_type {
        PciHeaderType::StandardDevice => {
            // Type 0 has 6 BARs (offsets 0x10, 0x14, 0x18, 0x1C, 0x20, 0x24)
            let mut bar_idx = 0u8;
            let mut out_bar_idx = 0usize;
            while bar_idx < 6 {
                let offset = 0x10 + (bar_idx * 4);
                let raw_low = read_config_u32(bus, dev, func, offset);
                let raw_high = if bar_idx < 5 {
                    read_config_u32(bus, dev, func, offset + 4)
                } else {
                    0
                };

                let (bar, is_64) = parse_bar(bar_idx, raw_low, raw_high);
                if bar.bar_type != BarType::None {
                    function_desc.bars[out_bar_idx] = bar;
                    out_bar_idx += 1;
                }

                if is_64 {
                    bar_idx += 2; // 64-bit BAR consumes 2 slots
                } else {
                    bar_idx += 1;
                }
            }
            function_desc.bar_count = out_bar_idx as u8;

            // Type 0 capability pointer offset is 0x34
            function_desc.cap_count = read_capabilities(
                bus,
                dev,
                func,
                status,
                0x34,
                &mut function_desc.capabilities,
            );
        }
        PciHeaderType::PciToPciBridge => {
            // Type 1 has 2 BARs (offsets 0x10, 0x14)
            let mut bar_idx = 0u8;
            let mut out_bar_idx = 0usize;
            while bar_idx < 2 {
                let offset = 0x10 + (bar_idx * 4);
                let raw_low = read_config_u32(bus, dev, func, offset);
                let raw_high = if bar_idx < 1 {
                    read_config_u32(bus, dev, func, offset + 4)
                } else {
                    0
                };

                let (bar, is_64) = parse_bar(bar_idx, raw_low, raw_high);
                if bar.bar_type != BarType::None {
                    function_desc.bars[out_bar_idx] = bar;
                    out_bar_idx += 1;
                }

                if is_64 {
                    bar_idx += 2;
                } else {
                    bar_idx += 1;
                }
            }
            function_desc.bar_count = out_bar_idx as u8;

            // Bridge bus numbers (offset 0x18: primary [0..7], secondary [8..15], subordinate [16..23])
            let bus_info = read_config_u32(bus, dev, func, 0x18);
            let sec_bus = ((bus_info >> 8) & 0xFF) as u8;
            let sub_bus = ((bus_info >> 16) & 0xFF) as u8;
            function_desc.secondary_bus = Some(sec_bus);
            function_desc.subordinate_bus = Some(sub_bus);

            // Type 1 capability pointer offset is 0x34
            function_desc.cap_count = read_capabilities(
                bus,
                dev,
                func,
                status,
                0x34,
                &mut function_desc.capabilities,
            );
        }
        _ => {}
    }

    Some(function_desc)
}

// -----------------------------------------------------------------------------
// Subsystem Initialization & Enumeration
// -----------------------------------------------------------------------------

/// Enumerate PCI buses and populate the device registry.
/// We scan root buses (0..=7) which covers standard and bridge-accessible hierarchies on QEMU Q35,
/// tracking secondary buses of discovered bridges to ensure complete coverage.
pub fn init() {
    crate::log_info!("[PCI] Initializing PCI Subsystem (Mechanism #1: Ports 0xCF8/0xCFC)...");

    let mut new_registry = PciRegistry::new();

    // Bitmap of buses to scan (buses 0..=255)
    let mut scanned_buses = [false; 256];
    let mut buses_to_scan = [0u8; 256];
    let mut bus_queue_len = 0usize;

    // Initial root scan: Check bus 0
    buses_to_scan[bus_queue_len] = 0;
    bus_queue_len += 1;

    // Check host bridge at 00:00.0 for multifunction host bridge across multiple root buses
    let host_hdr = read_config_u8(0, 0, 0, 0x0E);
    if (host_hdr & 0x80) != 0 {
        for fn_idx in 1..8 {
            let v = read_config_u16(0, 0, fn_idx, 0x00);
            if v != 0xFFFF && v != 0x0000 {
                // If multifunction host bridge has extra functions, scan buses 1..=fn_idx
                if bus_queue_len < 256 {
                    buses_to_scan[bus_queue_len] = fn_idx;
                    bus_queue_len += 1;
                }
            }
        }
    }

    // Process bus queue
    let mut q_idx = 0usize;
    while q_idx < bus_queue_len {
        let bus = buses_to_scan[q_idx];
        q_idx += 1;

        if scanned_buses[bus as usize] {
            continue;
        }
        scanned_buses[bus as usize] = true;

        for dev in 0..32 {
            // Check function 0 first
            let fn0 = match inspect_function(bus, dev, 0) {
                Some(f) => f,
                None => continue,
            };

            let is_multi = fn0.multifunction;

            // Handle bridge discovery
            if let Some(sec) = fn0.secondary_bus {
                if sec > bus && !scanned_buses[sec as usize] && bus_queue_len < 256 {
                    buses_to_scan[bus_queue_len] = sec;
                    bus_queue_len += 1;
                }
            }

            new_registry.add(fn0);

            // If device is multifunction, inspect functions 1..7
            if is_multi {
                for func in 1..8 {
                    if let Some(fn_other) = inspect_function(bus, dev, func) {
                        if let Some(sec) = fn_other.secondary_bus {
                            if sec > bus && !scanned_buses[sec as usize] && bus_queue_len < 256 {
                                buses_to_scan[bus_queue_len] = sec;
                                bus_queue_len += 1;
                            }
                        }
                        new_registry.add(fn_other);
                    }
                }
            }
        }
    }

    let dev_count = new_registry.count();
    let overflow = new_registry.overflow_count();
    crate::log_info!("[PCI] Enumeration complete: Discovered {} function(s) (overflow: {})", dev_count, overflow);

    // Pretty-print discovered devices
    for dev in new_registry.devices() {
        crate::log_info!(
            "  [{:02x}:{:02x}.{:x}] {:04x}:{:04x} | Class: {:02x}:{:02x} (rev 0x{:02x}) -> {}",
            dev.bus,
            dev.device,
            dev.function,
            dev.vendor_id,
            dev.device_id,
            dev.class_code,
            dev.subclass,
            dev.revision_id,
            dev.class_name()
        );

        for bar in &dev.bars[..dev.bar_count as usize] {
            match bar.bar_type {
                BarType::Io { port } => {
                    crate::log_info!("    -> BAR{}: I/O Port 0x{:04x}", bar.bar_index, port);
                }
                BarType::Memory32 { address, prefetchable } => {
                    crate::log_info!(
                        "    -> BAR{}: 32-bit MMIO 0x{:08x} (prefetch: {})",
                        bar.bar_index,
                        address,
                        prefetchable
                    );
                }
                BarType::Memory64 { address, prefetchable } => {
                    crate::log_info!(
                        "    -> BAR{}: 64-bit MMIO 0x{:016x} (prefetch: {})",
                        bar.bar_index,
                        address,
                        prefetchable
                    );
                }
                BarType::None => {}
            }
        }

        if dev.cap_count > 0 {
            crate::log_info!("    -> Capabilities: {} detected (first ID: 0x{:02x} at 0x{:02x})", dev.cap_count, dev.capabilities[0].id, dev.capabilities[0].offset);
        }
    }

    // Atomically swap into global registry
    {
        let mut guard = PCI_REGISTRY.lock();
        *guard = new_registry;
    }
}
