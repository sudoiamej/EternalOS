use limine::memory_map::EntryType;
use limine::request::MemoryMapRequest;

#[used]
#[unsafe(link_section = ".requests")]
static MEMORY_MAP_REQUEST: MemoryMapRequest = MemoryMapRequest::new();

pub fn parse_memory_map() {
    crate::log_info!("Inspecting physical memory map via Limine protocol...");

    let Some(response) = MEMORY_MAP_REQUEST.get_response() else {
        crate::log_warn!("No Memory Map response provided by Limine bootloader.");
        return;
    };

    let entries = response.entries();
    let total_count = entries.len();
    crate::log_info!("Memory map entries enumerated: {}", total_count);

    let mut usable_bytes: u64 = 0;
    let mut reserved_bytes: u64 = 0;
    let mut reclaimable_bytes: u64 = 0;
    let mut other_bytes: u64 = 0;

    for (idx, entry) in entries.iter().enumerate() {
        let base = entry.base;
        let length = entry.length;
        let entry_type = entry.entry_type;

        // Overflow validation: verify base + length does not overflow u64
        let end_address = match base.checked_add(length) {
            Some(end) => end,
            None => {
                crate::log_error!(
                    "[{:02}] OVERFLOW DETECTED: Base={:#018x} + Length={:#018x}",
                    idx,
                    base,
                    length
                );
                continue;
            }
        };

        let type_str = match entry_type {
            EntryType::USABLE => {
                usable_bytes = usable_bytes.saturating_add(length);
                "USABLE"
            }
            EntryType::RESERVED => {
                reserved_bytes = reserved_bytes.saturating_add(length);
                "RESERVED"
            }
            EntryType::ACPI_RECLAIMABLE => {
                reclaimable_bytes = reclaimable_bytes.saturating_add(length);
                "ACPI_RECLAIMABLE"
            }
            EntryType::ACPI_NVS => {
                reserved_bytes = reserved_bytes.saturating_add(length);
                "ACPI_NVS"
            }
            EntryType::BAD_MEMORY => {
                other_bytes = other_bytes.saturating_add(length);
                "BAD_MEMORY"
            }
            EntryType::BOOTLOADER_RECLAIMABLE => {
                reclaimable_bytes = reclaimable_bytes.saturating_add(length);
                "BOOTLOADER_RECLAIMABLE"
            }
            EntryType::EXECUTABLE_AND_MODULES => {
                reserved_bytes = reserved_bytes.saturating_add(length);
                "EXECUTABLE_AND_MODULES"
            }
            EntryType::FRAMEBUFFER => {
                reserved_bytes = reserved_bytes.saturating_add(length);
                "FRAMEBUFFER"
            }
            _ => {
                other_bytes = other_bytes.saturating_add(length);
                "UNKNOWN"
            }
        };

        crate::log_debug!(
            "[{:02}] {:#018x} - {:#018x} ({:>10} KiB) -> {}",
            idx,
            base,
            end_address,
            length / 1024,
            type_str
        );
    }

    crate::log_info!(
        "Memory Summary: Usable: {} MiB | Reserved: {} MiB | Reclaimable: {} MiB",
        usable_bytes / (1024 * 1024),
        reserved_bytes / (1024 * 1024),
        reclaimable_bytes / (1024 * 1024)
    );
}
