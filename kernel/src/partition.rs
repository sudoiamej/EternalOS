use crate::block::{BlockDevice, BlockError};
use core::fmt;

/// Maximum number of partitions supported in the static registry.
pub const MAX_PARTITIONS: usize = 16;

/// Standard sector size for MBR and GPT header structures.
pub const MBR_GPT_SECTOR_SIZE: usize = 512;

/// Formats of partition table recognized by EternalOS.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PartitionTableType {
    /// Legacy Master Boot Record.
    Mbr,
    /// GUID Partition Table.
    Gpt,
    /// Disk is unpartitioned / raw device.
    None,
}

impl fmt::Display for PartitionTableType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            PartitionTableType::Mbr => write!(f, "MBR"),
            PartitionTableType::Gpt => write!(f, "GPT"),
            PartitionTableType::None => write!(f, "None (Raw)"),
        }
    }
}

/// A 128-bit Globally Unique Identifier (UUID / GUID) according to RFC 4122 / UEFI spec.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Guid {
    pub time_low: u32,
    pub time_mid: u16,
    pub time_hi_and_version: u16,
    pub clock_seq_and_node: [u8; 8],
}

impl Guid {
    pub const ZERO: Self = Self {
        time_low: 0,
        time_mid: 0,
        time_hi_and_version: 0,
        clock_seq_and_node: [0; 8],
    };

    /// Parse a 16-byte mixed-endian GUID from raw slice (little-endian for first 3 fields).
    pub fn from_bytes(bytes: &[u8; 16]) -> Self {
        let time_low = u32::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]);
        let time_mid = u16::from_le_bytes([bytes[4], bytes[5]]);
        let time_hi_and_version = u16::from_le_bytes([bytes[6], bytes[7]]);
        let mut clock_seq_and_node = [0u8; 8];
        clock_seq_and_node.copy_from_slice(&bytes[8..16]);

        Self {
            time_low,
            time_mid,
            time_hi_and_version,
            clock_seq_and_node,
        }
    }

    pub fn is_zero(&self) -> bool {
        self.time_low == 0
            && self.time_mid == 0
            && self.time_hi_and_version == 0
            && self.clock_seq_and_node == [0; 8]
    }
}

impl fmt::Display for Guid {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "{:08x}-{:04x}-{:04x}-{:02x}{:02x}-{:02x}{:02x}{:02x}{:02x}{:02x}{:02x}",
            self.time_low,
            self.time_mid,
            self.time_hi_and_version,
            self.clock_seq_and_node[0],
            self.clock_seq_and_node[1],
            self.clock_seq_and_node[2],
            self.clock_seq_and_node[3],
            self.clock_seq_and_node[4],
            self.clock_seq_and_node[5],
            self.clock_seq_and_node[6],
            self.clock_seq_and_node[7]
        )
    }
}

/// Normalized type representation across MBR and GPT.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PartitionType {
    /// MBR 1-byte system ID.
    Mbr(u8),
    /// GPT 128-bit Type GUID.
    Gpt(Guid),
}

/// Normalized description of a single discovered partition.
#[derive(Debug, Clone, Copy)]
pub struct Partition {
    /// 0-based partition index on the device.
    pub index: usize,
    /// Partition table format (MBR or GPT).
    pub table_type: PartitionTableType,
    /// Starting Logical Block Address.
    pub start_lba: u64,
    /// Number of logical sectors.
    pub sector_count: u64,
    /// Partition type (MBR byte or GPT Type GUID).
    pub part_type: PartitionType,
    /// Unique partition GUID (GPT only).
    pub unique_guid: Guid,
    /// Partition attribute flags (bitmask, GPT only).
    pub flags: u64,
    /// Partition name as UTF-8 string bytes (up to 36 chars from UTF-16LE GPT name).
    pub name: [u8; 36],
    pub name_len: usize,
    /// Whether this partition is marked bootable / active.
    pub bootable: bool,
}

impl Partition {
    pub const fn empty() -> Self {
        Self {
            index: 0,
            table_type: PartitionTableType::None,
            start_lba: 0,
            sector_count: 0,
            part_type: PartitionType::Mbr(0),
            unique_guid: Guid::ZERO,
            flags: 0,
            name: [0; 36],
            name_len: 0,
            bootable: false,
        }
    }

    /// Calculate the exclusive end LBA (`start_lba + sector_count`).
    /// Returns None on integer overflow.
    pub fn end_lba(&self) -> Option<u64> {
        self.start_lba.checked_add(self.sector_count)
    }

    pub fn name_as_str(&self) -> &str {
        core::str::from_utf8(&self.name[..self.name_len]).unwrap_or("")
    }
}

/// Granular errors during partition table parsing.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PartitionError {
    /// Underlying block device returned an I/O or buffer error.
    BlockIo(BlockError),
    /// Device sector size is incompatible with standard 512-byte partition parsing.
    UnsupportedSectorSize(usize),
    /// Device capacity is too small to contain a partition table.
    DeviceTooSmall,
    /// MBR signature (0x55AA) is missing.
    InvalidMbrSignature,
    /// MBR boot indicator is invalid (must be 0x00 or 0x80).
    InvalidBootIndicator(u8),
    /// GPT header signature ("EFI PART") is missing or invalid.
    InvalidGptSignature,
    /// GPT header size is invalid (< 92 bytes or > sector size).
    InvalidHeaderSize(u32),
    /// GPT header CRC32 check failed.
    HeaderCrcMismatch { computed: u32, expected: u32 },
    /// GPT partition entry array CRC32 check failed.
    EntryArrayCrcMismatch { computed: u32, expected: u32 },
    /// GPT current LBA or backup LBA does not match expected disk locations.
    InvalidHeaderLba,
    /// GPT usable LBA range is inverted or outside device capacity.
    InvalidUsableLbaRange,
    /// GPT partition entry size or entry count is invalid or unsupported.
    InvalidEntryGeometry,
    /// Partition boundaries exceed disk capacity or usable LBA range.
    OutOfBounds { start: u64, count: u64, capacity: u64 },
    /// Overlapping partitions detected on the same disk.
    OverlappingPartitions { part_a: usize, part_b: usize },
    /// Duplicate unique partition GUID detected in GPT table.
    DuplicateUniqueGuid,
    /// Arithmetic overflow in LBA or byte calculation.
    ArithmeticOverflow,
    /// Extended/Logical partition (EBR) detected; unbounded traversal is unsupported.
    UnsupportedExtendedPartition(u8),
    /// Registry capacity exhausted (maximum partitions exceeded).
    RegistryOverflow,
}

impl From<BlockError> for PartitionError {
    fn from(err: BlockError) -> Self {
        PartitionError::BlockIo(err)
    }
}

impl fmt::Display for PartitionError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            PartitionError::BlockIo(e) => write!(f, "Block device I/O error: {}", e),
            PartitionError::UnsupportedSectorSize(sz) => write!(f, "Unsupported sector size: {} (requires 512)", sz),
            PartitionError::DeviceTooSmall => write!(f, "Device capacity too small for partition table"),
            PartitionError::InvalidMbrSignature => write!(f, "Invalid MBR boot signature (expected 0x55AA)"),
            PartitionError::InvalidBootIndicator(b) => write!(f, "Invalid MBR boot indicator: 0x{:02x}", b),
            PartitionError::InvalidGptSignature => write!(f, "Invalid GPT signature (expected 'EFI PART')"),
            PartitionError::InvalidHeaderSize(sz) => write!(f, "Invalid GPT header size: {}", sz),
            PartitionError::HeaderCrcMismatch { computed, expected } => {
                write!(f, "GPT header CRC mismatch: computed 0x{:08x}, expected 0x{:08x}", computed, expected)
            }
            PartitionError::EntryArrayCrcMismatch { computed, expected } => {
                write!(f, "GPT partition entry array CRC mismatch: computed 0x{:08x}, expected 0x{:08x}", computed, expected)
            }
            PartitionError::InvalidHeaderLba => write!(f, "GPT header LBA invalid or mismatched"),
            PartitionError::InvalidUsableLbaRange => write!(f, "GPT usable LBA range invalid"),
            PartitionError::InvalidEntryGeometry => write!(f, "GPT entry geometry invalid"),
            PartitionError::OutOfBounds { start, count, capacity } => {
                write!(f, "Partition [LBA {}..{}] exceeds capacity {}", start, start.saturating_add(*count), capacity)
            }
            PartitionError::OverlappingPartitions { part_a, part_b } => {
                write!(f, "Partitions {} and {} overlap", part_a, part_b)
            }
            PartitionError::DuplicateUniqueGuid => write!(f, "Duplicate GPT unique partition GUID"),
            PartitionError::ArithmeticOverflow => write!(f, "Arithmetic overflow in LBA computation"),
            PartitionError::UnsupportedExtendedPartition(t) => {
                write!(f, "Extended MBR partition type 0x{:02x} detected (unsupported)", t)
            }
            PartitionError::RegistryOverflow => write!(f, "Partition registry capacity exceeded"),
        }
    }
}

/// Bounded static registry of discovered partitions on a device.
#[derive(Debug, Clone, Copy)]
pub struct PartitionTable {
    pub table_type: PartitionTableType,
    pub partitions: [Partition; MAX_PARTITIONS],
    pub count: usize,
    pub disk_guid: Guid,
}

impl PartitionTable {
    pub const fn empty() -> Self {
        Self {
            table_type: PartitionTableType::None,
            partitions: [Partition::empty(); MAX_PARTITIONS],
            count: 0,
            disk_guid: Guid::ZERO,
        }
    }

    pub fn add(&mut self, partition: Partition) -> Result<(), PartitionError> {
        if self.count >= MAX_PARTITIONS {
            return Err(PartitionError::RegistryOverflow);
        }
        self.partitions[self.count] = partition;
        self.count += 1;
        Ok(())
    }

    pub fn list(&self) -> &[Partition] {
        &self.partitions[..self.count]
    }
}

// -----------------------------------------------------------------------------
// Pure CRC32 Implementation (IEEE 802.3 standard polynomial 0xEDB88320)
// -----------------------------------------------------------------------------

/// Computes the standard IEEE 802.3 32-bit CRC checksum.
pub fn crc32(data: &[u8]) -> u32 {
    let mut crc: u32 = 0xFFFF_FFFF;
    for &byte in data {
        crc ^= byte as u32;
        for _ in 0..8 {
            let mask = (crc & 1).wrapping_neg();
            crc = (crc >> 1) ^ (0xEDB8_8320 & mask);
        }
    }
    !crc
}

// -----------------------------------------------------------------------------
// MBR Parsing Implementation
// -----------------------------------------------------------------------------

const MBR_SIGNATURE: u16 = 0xAA55; // Little-endian 0x55, 0xAA
const GPT_PROTECTIVE_TYPE: u8 = 0xEE;

/// Check if an MBR partition table is a protective MBR for GPT.
/// A protective MBR has one entry with type 0xEE spanning the disk.
pub fn is_protective_mbr(sector0: &[u8; 512]) -> bool {
    let sig = u16::from_le_bytes([sector0[510], sector0[511]]);
    if sig != MBR_SIGNATURE {
        return false;
    }

    // Check entry 0
    let type0 = sector0[446 + 4];
    if type0 == GPT_PROTECTIVE_TYPE {
        return true;
    }

    false
}

/// Check if an MBR partition type represents an extended partition.
pub fn is_extended_type(part_type: u8) -> bool {
    matches!(part_type, 0x05 | 0x0F | 0x85)
}

/// Parse Master Boot Record primary partitions from sector 0.
pub fn parse_mbr(
    sector0: &[u8; 512],
    device_capacity: u64,
    table: &mut PartitionTable,
) -> Result<(), PartitionError> {
    let sig = u16::from_le_bytes([sector0[510], sector0[511]]);
    if sig != MBR_SIGNATURE {
        return Err(PartitionError::InvalidMbrSignature);
    }

    if is_protective_mbr(sector0) {
        // Protective MBR indicates GPT should be used
        return Ok(());
    }

    table.table_type = PartitionTableType::Mbr;

    for i in 0..4 {
        let offset = 446 + (i * 16);
        let boot_indicator = sector0[offset];
        if boot_indicator != 0x00 && boot_indicator != 0x80 {
            return Err(PartitionError::InvalidBootIndicator(boot_indicator));
        }

        let part_type = sector0[offset + 4];
        if part_type == 0 {
            // Unused / empty partition slot
            continue;
        }

        if is_extended_type(part_type) {
            return Err(PartitionError::UnsupportedExtendedPartition(part_type));
        }

        let start_lba = u32::from_le_bytes([
            sector0[offset + 8],
            sector0[offset + 9],
            sector0[offset + 10],
            sector0[offset + 11],
        ]) as u64;

        let sector_count = u32::from_le_bytes([
            sector0[offset + 12],
            sector0[offset + 13],
            sector0[offset + 14],
            sector0[offset + 15],
        ]) as u64;

        if sector_count == 0 {
            continue;
        }

        let end_lba = start_lba
            .checked_add(sector_count)
            .ok_or(PartitionError::ArithmeticOverflow)?;

        if end_lba > device_capacity {
            return Err(PartitionError::OutOfBounds {
                start: start_lba,
                count: sector_count,
                capacity: device_capacity,
            });
        }

        let partition = Partition {
            index: i,
            table_type: PartitionTableType::Mbr,
            start_lba,
            sector_count,
            part_type: PartitionType::Mbr(part_type),
            unique_guid: Guid::ZERO,
            flags: 0,
            name: [0; 36],
            name_len: 0,
            bootable: boot_indicator == 0x80,
        };

        table.add(partition)?;
    }

    // Validate non-overlapping partitions
    validate_no_overlap(table.list())?;

    Ok(())
}

// -----------------------------------------------------------------------------
// GPT Parsing Implementation
// -----------------------------------------------------------------------------

const GPT_SIGNATURE: &[u8; 8] = b"EFI PART";

/// Decode UTF-16LE partition name safely into UTF-8 buffer.
pub fn decode_utf16le_name(utf16_bytes: &[u8], out: &mut [u8; 36]) -> usize {
    let mut out_idx = 0;
    let num_words = utf16_bytes.len() / 2;

    for i in 0..num_words {
        let word = u16::from_le_bytes([utf16_bytes[i * 2], utf16_bytes[i * 2 + 1]]);
        if word == 0 {
            break; // Null terminator
        }

        if word < 0x80 {
            // ASCII character
            if out_idx < 36 {
                out[out_idx] = word as u8;
                out_idx += 1;
            }
        } else {
            // For non-ASCII or invalid surrogates, use '?' replacement
            if out_idx < 36 {
                out[out_idx] = b'?';
                out_idx += 1;
            }
        }
    }
    out_idx
}

/// Parse GUID Partition Table from device.
pub fn parse_gpt<D: BlockDevice + ?Sized>(
    device: &mut D,
    device_capacity: u64,
    table: &mut PartitionTable,
) -> Result<(), PartitionError> {
    let mut lba1_buf = [0u8; 512];
    device.read_sectors(1, 1, &mut lba1_buf)?;

    // 1. Signature check
    if &lba1_buf[0..8] != GPT_SIGNATURE {
        return Err(PartitionError::InvalidGptSignature);
    }

    let header_size = u32::from_le_bytes([lba1_buf[12], lba1_buf[13], lba1_buf[14], lba1_buf[15]]);
    if header_size < 92 || header_size > 512 {
        return Err(PartitionError::InvalidHeaderSize(header_size));
    }

    let expected_header_crc = u32::from_le_bytes([lba1_buf[16], lba1_buf[17], lba1_buf[18], lba1_buf[19]]);

    // Compute Header CRC32 with CRC field zeroed
    let mut crc_header_buf = [0u8; 512];
    crc_header_buf[..header_size as usize].copy_from_slice(&lba1_buf[..header_size as usize]);
    crc_header_buf[16..20].fill(0);
    let computed_header_crc = crc32(&crc_header_buf[..header_size as usize]);

    if computed_header_crc != expected_header_crc {
        return Err(PartitionError::HeaderCrcMismatch {
            computed: computed_header_crc,
            expected: expected_header_crc,
        });
    }

    // Parse Fields
    let my_lba = u64::from_le_bytes(lba1_buf[24..32].try_into().unwrap());
    let alternate_lba = u64::from_le_bytes(lba1_buf[32..40].try_into().unwrap());
    let first_usable = u64::from_le_bytes(lba1_buf[40..48].try_into().unwrap());
    let last_usable = u64::from_le_bytes(lba1_buf[48..56].try_into().unwrap());

    if my_lba != 1 || alternate_lba >= device_capacity {
        return Err(PartitionError::InvalidHeaderLba);
    }

    if first_usable >= last_usable || last_usable >= device_capacity {
        return Err(PartitionError::InvalidUsableLbaRange);
    }

    let disk_guid = Guid::from_bytes((&lba1_buf[56..72]).try_into().unwrap());
    table.disk_guid = disk_guid;
    table.table_type = PartitionTableType::Gpt;

    let part_entry_lba = u64::from_le_bytes(lba1_buf[72..80].try_into().unwrap());
    let num_entries = u32::from_le_bytes(lba1_buf[80..84].try_into().unwrap());
    let entry_size = u32::from_le_bytes(lba1_buf[84..88].try_into().unwrap());
    let expected_array_crc = u32::from_le_bytes(lba1_buf[88..92].try_into().unwrap());

    if entry_size < 128 || (entry_size % 8) != 0 || num_entries == 0 || num_entries > 128 {
        return Err(PartitionError::InvalidEntryGeometry);
    }

    let total_array_bytes = (num_entries as usize)
        .checked_mul(entry_size as usize)
        .ok_or(PartitionError::ArithmeticOverflow)?;

    let array_sectors = (total_array_bytes.div_ceil(512)) as u32;

    // Read partition entry array
    // Allocate a temporary stack buffer sized up to 32 sectors (16 KiB = 128 entries * 128 bytes)
    // To protect stack, we limit entry parsing to max 128 entries
    let mut entry_buf = [0u8; 16384];
    if total_array_bytes > entry_buf.len() {
        return Err(PartitionError::InvalidEntryGeometry);
    }

    device.read_sectors(part_entry_lba, array_sectors, &mut entry_buf[..array_sectors as usize * 512])?;

    // Validate Partition Entry Array CRC32
    let computed_array_crc = crc32(&entry_buf[..total_array_bytes]);
    if computed_array_crc != expected_array_crc {
        return Err(PartitionError::EntryArrayCrcMismatch {
            computed: computed_array_crc,
            expected: expected_array_crc,
        });
    }

    // Parse Individual Partition Entries
    for i in 0..num_entries as usize {
        let offset = i * (entry_size as usize);
        let entry_slice = &entry_buf[offset..offset + entry_size as usize];

        let type_guid = Guid::from_bytes((&entry_slice[0..16]).try_into().unwrap());
        if type_guid.is_zero() {
            // Unused partition entry
            continue;
        }

        let unique_guid = Guid::from_bytes((&entry_slice[16..32]).try_into().unwrap());
        let starting_lba = u64::from_le_bytes(entry_slice[32..40].try_into().unwrap());
        let ending_lba = u64::from_le_bytes(entry_slice[40..48].try_into().unwrap());
        let attributes = u64::from_le_bytes(entry_slice[48..56].try_into().unwrap());

        if ending_lba < starting_lba {
            return Err(PartitionError::ArithmeticOverflow);
        }

        let sector_count = ending_lba
            .checked_sub(starting_lba)
            .and_then(|c| c.checked_add(1))
            .ok_or(PartitionError::ArithmeticOverflow)?;

        if starting_lba < first_usable || ending_lba > last_usable {
            return Err(PartitionError::OutOfBounds {
                start: starting_lba,
                count: sector_count,
                capacity: device_capacity,
            });
        }

        let mut name_buf = [0u8; 36];
        let name_len = decode_utf16le_name(&entry_slice[56..entry_slice.len().min(128)], &mut name_buf);

        let partition = Partition {
            index: i,
            table_type: PartitionTableType::Gpt,
            start_lba: starting_lba,
            sector_count,
            part_type: PartitionType::Gpt(type_guid),
            unique_guid,
            flags: attributes,
            name: name_buf,
            name_len,
            bootable: (attributes & (1 << 2)) != 0, // Legacy BIOS bootable bit
        };

        table.add(partition)?;
    }

    // Validate duplicate unique GUIDs
    validate_unique_guids(table.list())?;

    // Validate non-overlapping partitions
    validate_no_overlap(table.list())?;

    Ok(())
}

/// Ensure no two partitions overlap on the disk.
fn validate_no_overlap(partitions: &[Partition]) -> Result<(), PartitionError> {
    for i in 0..partitions.len() {
        let a_start = partitions[i].start_lba;
        let a_end = partitions[i]
            .end_lba()
            .ok_or(PartitionError::ArithmeticOverflow)?;

        for j in (i + 1)..partitions.len() {
            let b_start = partitions[j].start_lba;
            let b_end = partitions[j]
                .end_lba()
                .ok_or(PartitionError::ArithmeticOverflow)?;

            // Overlap condition: max(start) < min(end)
            if a_start.max(b_start) < a_end.min(b_end) {
                return Err(PartitionError::OverlappingPartitions {
                    part_a: partitions[i].index,
                    part_b: partitions[j].index,
                });
            }
        }
    }
    Ok(())
}

/// Ensure all unique partition GUIDs are distinct.
fn validate_unique_guids(partitions: &[Partition]) -> Result<(), PartitionError> {
    for i in 0..partitions.len() {
        if partitions[i].unique_guid.is_zero() {
            continue;
        }
        for j in (i + 1)..partitions.len() {
            if partitions[i].unique_guid == partitions[j].unique_guid {
                return Err(PartitionError::DuplicateUniqueGuid);
            }
        }
    }
    Ok(())
}

// -----------------------------------------------------------------------------
// Universal Partition Discovery Entrypoint
// -----------------------------------------------------------------------------

/// Discovers partitions on a block device by inspecting MBR and GPT structures.
/// Operates strictly in a READ-ONLY manner.
pub fn discover_partitions<D: BlockDevice + ?Sized>(
    device: &mut D,
) -> Result<PartitionTable, PartitionError> {
    let info = device.info();

    if info.sector_size != MBR_GPT_SECTOR_SIZE {
        return Err(PartitionError::UnsupportedSectorSize(info.sector_size));
    }

    if info.total_sectors < 34 {
        // Standard GPT requires at least LBA 0 (PMBR), LBA 1 (GPT Header), LBA 2..33 (Entries)
        return Err(PartitionError::DeviceTooSmall);
    }

    let mut sector0 = [0u8; 512];
    device.read_sectors(0, 1, &mut sector0)?;

    let mut table = PartitionTable::empty();

    if is_protective_mbr(&sector0) {
        // Protective MBR detected -> Attempt GPT parsing
        match parse_gpt(device, info.total_sectors, &mut table) {
            Ok(()) => return Ok(table),
            Err(e) => {
                // Return error if GPT parsing failed on protective MBR disk
                return Err(e);
            }
        }
    }

    // Try standard MBR parsing
    match parse_mbr(&sector0, info.total_sectors, &mut table) {
        Ok(()) => {
            if table.count > 0 {
                return Ok(table);
            }
        }
        Err(e) => {
            // If MBR signature is missing, check if raw unpartitioned disk
            if e == PartitionError::InvalidMbrSignature {
                table.table_type = PartitionTableType::None;
                return Ok(table);
            }
            return Err(e);
        }
    }

    // If no partitions found in MBR, also check if GPT header is present at LBA 1
    let mut lba1 = [0u8; 512];
    if device.read_sectors(1, 1, &mut lba1).is_ok() && &lba1[0..8] == GPT_SIGNATURE {
        parse_gpt(device, info.total_sectors, &mut table)?;
        return Ok(table);
    }

    Ok(table)
}
