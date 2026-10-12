use crate::console::console_write;
use crate::shell_parser::ParsedArgs;

/// Standardized Eternal Control Suite (ECS) Process Exit Codes
pub mod exit_code {
    pub const SUCCESS: i32 = 0;              // Command completed successfully
    pub const OPERATIONAL_FAILURE: i32 = 1;  // Runtime or execution failure
    pub const INVALID_USAGE: i32 = 2;        // Invalid syntax, unknown flag, missing argument
    pub const UNSUPPORTED: i32 = 3;          // Feature not supported on current platform
    pub const RESOURCE_UNAVAILABLE: i32 = 4; // Required hardware or prerequisite missing
    pub const VALIDATION_FAILURE: i32 = 5;   // Integrity check or assertion failed
}

/// Structured Output Format
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OutputFormat {
    Text,
    Json,
}

/// Standardized CLI Argument Options supported across ECS administration commands
#[derive(Debug, Clone, Copy)]
pub struct CommonOptions {
    pub help: bool,
    pub version: bool,
    pub verbose: bool,
    pub quiet: bool,
    pub format: OutputFormat,
    pub dry_run: bool,
}

impl CommonOptions {
    pub const fn default() -> Self {
        Self {
            help: false,
            version: false,
            verbose: false,
            quiet: false,
            format: OutputFormat::Text,
            dry_run: false,
        }
    }

    /// Parse common flags starting from index 1 in `ParsedArgs`.
    /// Returns the parsed options and the index of the first positional argument.
    pub fn parse(args: &ParsedArgs) -> (Self, usize) {
        let mut opts = Self::default();
        let mut positional_idx = args.len();

        let mut i = 1;
        while i < args.len() {
            let arg = args.get(i).unwrap_or("");
            if arg == "-h" || arg == "--help" {
                opts.help = true;
            } else if arg == "-V" || arg == "--version" {
                opts.version = true;
            } else if arg == "-v" || arg == "--verbose" {
                opts.verbose = true;
            } else if arg == "-q" || arg == "--quiet" {
                opts.quiet = true;
            } else if arg == "--dry-run" {
                opts.dry_run = true;
            } else if arg == "--json" {
                opts.format = OutputFormat::Json;
            } else if arg == "--format" {
                if i + 1 < args.len() {
                    let fmt_str = args.get(i + 1).unwrap_or("");
                    if fmt_str == "json" {
                        opts.format = OutputFormat::Json;
                    } else {
                        opts.format = OutputFormat::Text;
                    }
                    i += 1;
                }
            } else if arg.starts_with("--format=") {
                let fmt_val = &arg[9..];
                if fmt_val == "json" {
                    opts.format = OutputFormat::Json;
                } else {
                    opts.format = OutputFormat::Text;
                }
            } else if !arg.starts_with('-') && positional_idx == args.len() {
                positional_idx = i;
            }
            i += 1;
        }

        (opts, positional_idx)
    }
}

/// Hardware and Kernel Subsystem Capability Status
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CapabilityStatus {
    Supported,         // Architecture/firmware supports it
    Available,         // Initialized and actively operating
    UnavailablePrereq, // Driver/hardware absent or uninitialized
    Unsupported,       // Hardware or architecture does not support it
    NotTested,         // Not yet tested in active session
    Failed,            // Self-test or probe failed
}

impl CapabilityStatus {
    pub fn as_str(&self) -> &'static str {
        match self {
            CapabilityStatus::Supported => "SUPPORTED",
            CapabilityStatus::Available => "AVAILABLE",
            CapabilityStatus::UnavailablePrereq => "UNAVAILABLE_PREREQ",
            CapabilityStatus::Unsupported => "UNSUPPORTED",
            CapabilityStatus::NotTested => "NOT_TESTED",
            CapabilityStatus::Failed => "FAILED",
        }
    }
}

/// Capability Record
#[derive(Debug, Clone, Copy)]
pub struct CapabilityRecord {
    pub name: &'static str,
    pub category: &'static str,
    pub status: CapabilityStatus,
    pub details: &'static str,
}

/// Query system capabilities dynamically based on live kernel state
pub fn get_system_capabilities() -> [CapabilityRecord; 16] {
    let has_net = crate::net::with_network_stack(|stack| stack.device.is_some());
    let has_disk = crate::virtio_blk::with_block_device(|_dev| true).is_some();
    let has_pci = crate::pci::with_pci_registry(|reg| reg.count() > 0);
    let has_usb = crate::usb::device::with_usb_registry(|reg| reg.count > 0);

    [
        CapabilityRecord {
            name: "cpu.x86_64",
            category: "Processor",
            status: CapabilityStatus::Available,
            details: "64-bit Long Mode, SSE/SSE2, CPUID active",
        },
        CapabilityRecord {
            name: "mem.paging",
            category: "Memory",
            status: CapabilityStatus::Available,
            details: "4-Level Paging (PML4) with higher-half kernel map",
        },
        CapabilityRecord {
            name: "mem.heap",
            category: "Memory",
            status: CapabilityStatus::Available,
            details: "Kernel Linked-List Allocator (1 MiB dynamic pool)",
        },
        CapabilityRecord {
            name: "kernel.sched",
            category: "Execution",
            status: CapabilityStatus::Available,
            details: "Round-Robin Task Scheduler with PIT 100Hz preemption",
        },
        CapabilityRecord {
            name: "kernel.user_proc",
            category: "Execution",
            status: CapabilityStatus::Available,
            details: "Ring 3 (CPL3) User Address Spaces with ELF64 loader",
        },
        CapabilityRecord {
            name: "bus.pci",
            category: "Hardware",
            status: if has_pci { CapabilityStatus::Available } else { CapabilityStatus::UnavailablePrereq },
            details: "PCI Configuration Space Mechanism #1 enumeration",
        },
        CapabilityRecord {
            name: "storage.virtio",
            category: "Storage",
            status: if has_disk { CapabilityStatus::Available } else { CapabilityStatus::UnavailablePrereq },
            details: "VirtIO-Block PCI driver with DMA descriptor queues",
        },
        CapabilityRecord {
            name: "fs.ramfs",
            category: "Filesystem",
            status: CapabilityStatus::Available,
            details: "In-memory dynamic RamFs mounted at '/' (read-write)",
        },
        CapabilityRecord {
            name: "fs.eternalfs",
            category: "Filesystem",
            status: if has_disk { CapabilityStatus::Available } else { CapabilityStatus::UnavailablePrereq },
            details: "Read-only EternalFS partition verification on /disk",
        },
        CapabilityRecord {
            name: "display.fb",
            category: "Graphics",
            status: CapabilityStatus::Available,
            details: "Limine Linear Framebuffer (32-bpp BGRA) with console",
        },
        CapabilityRecord {
            name: "input.ps2",
            category: "Input",
            status: CapabilityStatus::Available,
            details: "PS/2 Keyboard & Mouse drivers with IRQ1/IRQ12 buffering",
        },
        CapabilityRecord {
            name: "net.e1000",
            category: "Network",
            status: if has_net { CapabilityStatus::Available } else { CapabilityStatus::UnavailablePrereq },
            details: "Intel 82540EM Gigabit NIC with Ring-DMA RX/TX",
        },
        CapabilityRecord {
            name: "net.tcp_ip",
            category: "Network",
            status: if has_net { CapabilityStatus::Available } else { CapabilityStatus::UnavailablePrereq },
            details: "Real Ethernet, ARP, IPv4, ICMP, UDP, TCP state machine",
        },
        CapabilityRecord {
            name: "bus.usb",
            category: "Hardware",
            status: if has_usb { CapabilityStatus::Available } else { CapabilityStatus::UnavailablePrereq },
            details: "UHCI Host Controller discovery and device enumeration",
        },
        CapabilityRecord {
            name: "clock.rtc",
            category: "System",
            status: CapabilityStatus::Available,
            details: "Motorola MC146818 CMOS Real-Time Clock with BCD decoding",
        },
        CapabilityRecord {
            name: "srv.manager",
            category: "Services",
            status: CapabilityStatus::Available,
            details: "Native EternalOS Service Manager (eservice)",
        },
    ]
}

// -----------------------------------------------------------------------------
// JSON Formatting Helpers (Zero Allocation, Bounded)
// -----------------------------------------------------------------------------

pub struct JsonPrinter;

impl JsonPrinter {
    pub fn begin_object() {
        console_write("{\r\n");
    }

    pub fn end_object() {
        console_write("}\r\n");
    }

    pub fn field_str(key: &str, val: &str, is_last: bool) {
        console_write("  \"");
        console_write(key);
        console_write("\": \"");
        // Escape special chars
        for b in val.bytes() {
            match b {
                b'"' => console_write("\\\""),
                b'\\' => console_write("\\\\"),
                b'\n' => console_write("\\n"),
                b'\r' => console_write("\\r"),
                b'\t' => console_write("\\t"),
                _ => {
                    let arr = [b];
                    if let Ok(s) = core::str::from_utf8(&arr) {
                        console_write(s);
                    }
                }
            }
        }
        console_write("\"");
        if !is_last {
            console_write(",");
        }
        console_write("\r\n");
    }

    pub fn field_u64(key: &str, val: u64, is_last: bool) {
        console_write("  \"");
        console_write(key);
        console_write("\": ");
        let mut b = [0u8; 32];
        console_write(crate::shell::format_u64(val, &mut b));
        if !is_last {
            console_write(",");
        }
        console_write("\r\n");
    }

    pub fn field_i64(key: &str, val: i64, is_last: bool) {
        console_write("  \"");
        console_write(key);
        console_write("\": ");
        let mut b = [0u8; 32];
        if val < 0 {
            console_write("-");
            console_write(crate::shell::format_u64((-val) as u64, &mut b));
        } else {
            console_write(crate::shell::format_u64(val as u64, &mut b));
        }
        if !is_last {
            console_write(",");
        }
        console_write("\r\n");
    }

    pub fn field_bool(key: &str, val: bool, is_last: bool) {
        console_write("  \"");
        console_write(key);
        console_write("\": ");
        console_write(if val { "true" } else { "false" });
        if !is_last {
            console_write(",");
        }
        console_write("\r\n");
    }
}
