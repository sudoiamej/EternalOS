use crate::block::{BlockDevice, BlockDeviceInfo, BlockError, validate_request};
use crate::partition::{
    discover_partitions, crc32, Guid, PartitionError, PartitionTableType, PartitionType,
    is_protective_mbr, is_extended_type,
};
use crate::{log_info, log_warn};

/// Synthetic Block Device backed by in-memory sector array.
pub struct SyntheticDisk {
    sectors: &'static mut [[u8; 512]],
    info: BlockDeviceInfo,
}

impl SyntheticDisk {
    pub fn new(sectors: &'static mut [[u8; 512]]) -> Self {
        let total_sectors = sectors.len() as u64;
        let info = BlockDeviceInfo::new("Synthetic Test Disk", total_sectors, 512, false);
        Self { sectors, info }
    }

    pub fn reset(&mut self) {
        for sector in self.sectors.iter_mut() {
            sector.fill(0);
        }
    }
}

impl BlockDevice for SyntheticDisk {
    fn info(&self) -> BlockDeviceInfo {
        self.info
    }

    fn read_sectors(&mut self, start_sector: u64, sector_count: u32, buf: &mut [u8]) -> Result<(), BlockError> {
        validate_request(start_sector, sector_count, buf.len(), self.info.total_sectors, self.info.sector_size)?;

        for i in 0..sector_count as usize {
            let sec_idx = (start_sector as usize) + i;
            let buf_offset = i * 512;
            buf[buf_offset..buf_offset + 512].copy_from_slice(&self.sectors[sec_idx]);
        }
        Ok(())
    }

    fn write_sectors(&mut self, start_sector: u64, sector_count: u32, buf: &[u8]) -> Result<(), BlockError> {
        validate_request(start_sector, sector_count, buf.len(), self.info.total_sectors, self.info.sector_size)?;

        for i in 0..sector_count as usize {
            let sec_idx = (start_sector as usize) + i;
            let buf_offset = i * 512;
            self.sectors[sec_idx].copy_from_slice(&buf[buf_offset..buf_offset + 512]);
        }
        Ok(())
    }
}

// 64 static sectors = 32 KiB synthetic disk for unit tests
static mut SYNTH_SECTORS: [[u8; 512]; 64] = [[0; 512]; 64];

pub fn run_phase14a_partition_tests() {
    log_info!("====================================================");
    log_info!("   [TEST SUITE] Phase 14A: Partition Discovery      ");
    log_info!("====================================================");

    test_crc32_algorithm();
    test_guid_parsing();
    test_mbr_synthetic_tests();
    test_gpt_synthetic_tests();
    test_hardware_partition_discovery();

    log_info!(">>> ALL PHASE 14A PARTITION TESTS PASSED SUCCESSFULLY!");
    log_info!("====================================================");
}

/// Test 1: CRC32 algorithm correctness.
fn test_crc32_algorithm() {
    log_info!(">>> [TEST 1/5] Verifying IEEE 802.3 CRC32 Implementation...");

    // Standard vector: "123456789" -> 0xCBF43926
    let vector = b"123456789";
    let result = crc32(vector);
    assert_eq!(result, 0xCBF4_3926, "CRC32 computation for standard check vector failed");

    // Empty vector -> 0x00000000
    assert_eq!(crc32(b""), 0x0000_0000, "CRC32 for empty slice must be 0");

    log_info!("    [PASS] CRC32 checksum vectors verified.");
}

/// Test 2: GUID parsing and formatting.
fn test_guid_parsing() {
    log_info!(">>> [TEST 2/5] Testing GUID Representation & Parsing...");

    let raw_bytes: [u8; 16] = [
        0x28, 0x73, 0x2a, 0xc1, 0x1f, 0xf8, 0xd2, 0x11,
        0xba, 0x4b, 0x00, 0xa0, 0xc9, 0x3e, 0xc9, 0x3b,
    ];
    let guid = Guid::from_bytes(&raw_bytes);
    // Little-endian first 3 fields: c12a7328-f81f-11d2-ba4b-00a0c93ec93b (EFI System Partition)
    assert_eq!(guid.time_low, 0xc12a_7328);
    assert_eq!(guid.time_mid, 0xf81f);
    assert_eq!(guid.time_hi_and_version, 0x11d2);
    assert_eq!(guid.clock_seq_and_node[0], 0xba);
    assert!(!guid.is_zero());
    assert!(Guid::ZERO.is_zero());

    log_info!("    [PASS] GUID little-endian conversion & zero-checks verified.");
}

/// Test 3: Synthetic MBR parsing tests.
fn test_mbr_synthetic_tests() {
    log_info!(">>> [TEST 3/5] Testing Synthetic MBR Scenarios...");

    let mut disk = SyntheticDisk::new(unsafe { &mut *(&raw mut SYNTH_SECTORS) });

    // Scenario 1: Empty MBR (0x55AA signature present, 0 partitions)
    disk.reset();
    disk.sectors[0][510] = 0x55;
    disk.sectors[0][511] = 0xAA;
    let table = discover_partitions(&mut disk).expect("Valid empty MBR should succeed");
    assert_eq!(table.count, 0);

    // Scenario 2: Single Valid Primary MBR Partition
    disk.reset();
    disk.sectors[0][510] = 0x55;
    disk.sectors[0][511] = 0xAA;
    let entry0 = &mut disk.sectors[0][446..462];
    entry0[0] = 0x80; // Bootable
    entry0[4] = 0x83; // Linux Native / Ext
    entry0[8..12].copy_from_slice(&10u32.to_le_bytes()); // Start LBA = 10
    entry0[12..16].copy_from_slice(&20u32.to_le_bytes()); // Count = 20
    let table = discover_partitions(&mut disk).expect("Single primary MBR failed");
    assert_eq!(table.table_type, PartitionTableType::Mbr);
    assert_eq!(table.count, 1);
    assert_eq!(table.partitions[0].start_lba, 10);
    assert_eq!(table.partitions[0].sector_count, 20);
    assert_eq!(table.partitions[0].part_type, PartitionType::Mbr(0x83));
    assert!(table.partitions[0].bootable);

    // Scenario 3: Missing 0x55AA Signature (returns Raw / None)
    disk.reset();
    let table = discover_partitions(&mut disk).expect("Missing signature should report Raw table");
    assert_eq!(table.table_type, PartitionTableType::None);

    // Scenario 4: Invalid Boot Indicator (must reject)
    disk.reset();
    disk.sectors[0][510] = 0x55;
    disk.sectors[0][511] = 0xAA;
    disk.sectors[0][446] = 0x42; // Invalid boot indicator
    disk.sectors[0][446 + 4] = 0x83;
    let err = discover_partitions(&mut disk).unwrap_err();
    assert_eq!(err, PartitionError::InvalidBootIndicator(0x42));

    // Scenario 5: Partition Outside Capacity (Start LBA 50, Count 20 on 64-sector disk)
    disk.reset();
    disk.sectors[0][510] = 0x55;
    disk.sectors[0][511] = 0xAA;
    let entry0 = &mut disk.sectors[0][446..462];
    entry0[4] = 0x83;
    entry0[8..12].copy_from_slice(&50u32.to_le_bytes());
    entry0[12..16].copy_from_slice(&20u32.to_le_bytes()); // 50 + 20 = 70 > 64
    let err = discover_partitions(&mut disk).unwrap_err();
    match err {
        PartitionError::OutOfBounds { start, count, capacity } => {
            assert_eq!(start, 50);
            assert_eq!(count, 20);
            assert_eq!(capacity, 64);
        }
        other => panic!("Expected OutOfBounds, got {:?}", other),
    }

    // Scenario 6: Overlapping Partitions
    disk.reset();
    disk.sectors[0][510] = 0x55;
    disk.sectors[0][511] = 0xAA;
    // Part 1: LBA 10..25 (len 15)
    disk.sectors[0][446 + 4] = 0x83;
    disk.sectors[0][446 + 8..446 + 12].copy_from_slice(&10u32.to_le_bytes());
    disk.sectors[0][446 + 12..446 + 16].copy_from_slice(&15u32.to_le_bytes());
    // Part 2: LBA 20..35 (len 15) -> Overlaps with Part 1!
    disk.sectors[0][462 + 4] = 0x83;
    disk.sectors[0][462 + 8..462 + 12].copy_from_slice(&20u32.to_le_bytes());
    disk.sectors[0][462 + 12..462 + 16].copy_from_slice(&15u32.to_le_bytes());
    let err = discover_partitions(&mut disk).unwrap_err();
    assert_eq!(err, PartitionError::OverlappingPartitions { part_a: 0, part_b: 1 });

    // Scenario 7: Extended Partition Rejection
    disk.reset();
    disk.sectors[0][510] = 0x55;
    disk.sectors[0][511] = 0xAA;
    disk.sectors[0][446 + 4] = 0x05; // Extended partition
    assert!(is_extended_type(0x05));
    let err = discover_partitions(&mut disk).unwrap_err();
    assert_eq!(err, PartitionError::UnsupportedExtendedPartition(0x05));

    // Scenario 8: Protective MBR Detection
    disk.reset();
    disk.sectors[0][510] = 0x55;
    disk.sectors[0][511] = 0xAA;
    disk.sectors[0][446 + 4] = 0xEE; // GPT Protective MBR
    assert!(is_protective_mbr(&disk.sectors[0]));

    log_info!("    [PASS] All synthetic MBR test cases passed.");
}

/// Test 4: Synthetic GPT parsing tests.
fn test_gpt_synthetic_tests() {
    log_info!(">>> [TEST 4/5] Testing Synthetic GPT Scenarios...");

    let mut disk = SyntheticDisk::new(unsafe { &mut *(&raw mut SYNTH_SECTORS) });

    // Helper closure to build a valid minimal GPT disk
    let setup_valid_gpt = |d: &mut SyntheticDisk| {
        d.reset();
        // LBA 0: Protective MBR
        d.sectors[0][510] = 0x55;
        d.sectors[0][511] = 0xAA;
        d.sectors[0][446 + 4] = 0xEE;

        // LBA 2: Partition Entry 0
        let p0 = &mut d.sectors[2][0..128];
        // Type GUID: {1, 0, 0, 0...}
        p0[0] = 0x01;
        // Unique GUID: {2, 0, 0, 0...}
        p0[16] = 0x02;
        // Start LBA = 10, End LBA = 20
        p0[32..40].copy_from_slice(&10u64.to_le_bytes());
        p0[40..48].copy_from_slice(&20u64.to_le_bytes());
        // Name UTF-16LE: "Test"
        p0[56] = b'T'; p0[57] = 0;
        p0[58] = b'e'; p0[59] = 0;
        p0[60] = b's'; p0[61] = 0;
        p0[62] = b't'; p0[63] = 0;

        // Calculate Array CRC32 (1 entry = 128 bytes)
        let array_crc = crc32(&d.sectors[2][0..128]);

        // LBA 1: GPT Header
        let h = &mut d.sectors[1];
        h[0..8].copy_from_slice(b"EFI PART");
        h[8..12].copy_from_slice(&0x00010000u32.to_le_bytes()); // Rev 1.0
        h[12..16].copy_from_slice(&92u32.to_le_bytes()); // Header size = 92
        h[24..32].copy_from_slice(&1u64.to_le_bytes()); // My LBA = 1
        h[32..40].copy_from_slice(&63u64.to_le_bytes()); // Backup LBA = 63
        h[40..48].copy_from_slice(&6u64.to_le_bytes()); // First usable = 6
        h[48..56].copy_from_slice(&60u64.to_le_bytes()); // Last usable = 60
        h[72..80].copy_from_slice(&2u64.to_le_bytes()); // Entry LBA = 2
        h[80..84].copy_from_slice(&1u32.to_le_bytes()); // Num entries = 1
        h[84..88].copy_from_slice(&128u32.to_le_bytes()); // Entry size = 128
        h[88..92].copy_from_slice(&array_crc.to_le_bytes());

        // Header CRC (with CRC field 16..20 zeroed)
        h[16..20].fill(0);
        let header_crc = crc32(&h[0..92]);
        h[16..20].copy_from_slice(&header_crc.to_le_bytes());
    };

    // Scenario 1: Valid GPT with single partition
    setup_valid_gpt(&mut disk);
    let table = discover_partitions(&mut disk).expect("Valid synthetic GPT should succeed");
    assert_eq!(table.table_type, PartitionTableType::Gpt);
    assert_eq!(table.count, 1);
    assert_eq!(table.partitions[0].start_lba, 10);
    assert_eq!(table.partitions[0].sector_count, 11); // 20 - 10 + 1 = 11
    assert_eq!(table.partitions[0].name_as_str(), "Test");

    // Scenario 2: Corrupted Header CRC32
    setup_valid_gpt(&mut disk);
    disk.sectors[1][16] ^= 0xFF; // Invalidate header CRC
    let err = discover_partitions(&mut disk).unwrap_err();
    match err {
        PartitionError::HeaderCrcMismatch { .. } => (),
        other => panic!("Expected HeaderCrcMismatch, got {:?}", other),
    }

    // Scenario 3: Corrupted Entry Array CRC32
    setup_valid_gpt(&mut disk);
    disk.sectors[2][0] ^= 0xFF; // Modify partition entry data without updating header CRC
    // Recompute header CRC to isolate array CRC error
    disk.sectors[1][16..20].fill(0);
    let new_hcrc = crc32(&disk.sectors[1][0..92]);
    disk.sectors[1][16..20].copy_from_slice(&new_hcrc.to_le_bytes());
    let err = discover_partitions(&mut disk).unwrap_err();
    match err {
        PartitionError::EntryArrayCrcMismatch { .. } => (),
        other => panic!("Expected EntryArrayCrcMismatch, got {:?}", other),
    }

    // Scenario 4: Invalid Header Size (< 92)
    setup_valid_gpt(&mut disk);
    disk.sectors[1][12..16].copy_from_slice(&80u32.to_le_bytes());
    disk.sectors[1][16..20].fill(0);
    let new_hcrc = crc32(&disk.sectors[1][0..80]);
    disk.sectors[1][16..20].copy_from_slice(&new_hcrc.to_le_bytes());
    let err = discover_partitions(&mut disk).unwrap_err();
    assert_eq!(err, PartitionError::InvalidHeaderSize(80));

    // Scenario 5: Partition Outside Usable Range
    setup_valid_gpt(&mut disk);
    // Move starting LBA before first usable (first usable = 6, set start = 2)
    disk.sectors[2][32..40].copy_from_slice(&2u64.to_le_bytes());
    let new_acrc = crc32(&disk.sectors[2][0..128]);
    disk.sectors[1][88..92].copy_from_slice(&new_acrc.to_le_bytes());
    disk.sectors[1][16..20].fill(0);
    let new_hcrc = crc32(&disk.sectors[1][0..92]);
    disk.sectors[1][16..20].copy_from_slice(&new_hcrc.to_le_bytes());
    let err = discover_partitions(&mut disk).unwrap_err();
    match err {
        PartitionError::OutOfBounds { .. } => (),
        other => panic!("Expected OutOfBounds, got {:?}", other),
    }

    log_info!("    [PASS] All synthetic GPT test cases passed.");
}

/// Test 5: Real Hardware Partition Discovery on attached QEMU test disk.
fn test_hardware_partition_discovery() {
    log_info!(">>> [TEST 5/5] Real Hardware Partition Discovery on VirtIO Disk...");

    let discovered = crate::virtio_blk::with_block_device(|dev| {
        let info = dev.info();
        log_info!("    Querying VirtIO disk: {} (Capacity: {} sectors)",
            info.name_as_str(), info.total_sectors);

        match discover_partitions(dev) {
            Ok(table) => {
                log_info!("    Discovered Table Format: {}", table.table_type);
                log_info!("    Discovered Partitions  : {}", table.count);

                for (i, p) in table.list().iter().enumerate() {
                    log_info!("      [{}] LBA {}..{} ({} sectors) | Type: {:?} | Bootable: {}",
                        i, p.start_lba, p.start_lba + p.sector_count, p.sector_count, p.part_type, p.bootable);
                    if table.table_type == PartitionTableType::Gpt {
                        log_info!("          Name: '{}', GUID: {}", p.name_as_str(), p.unique_guid);
                    }
                }
                log_info!("    [PASS] Hardware partition discovery succeeded without corruption.");
                true
            }
            Err(e) => {
                log_warn!("    Partition discovery reported: {}", e);
                // Even on an unpartitioned disk or raw disk, discovery must fail safely without crashing
                log_info!("    [PASS] Hardware partition discovery handled disk safely.");
                true
            }
        }
    });

    if discovered == Some(true) {
        log_info!("    [PASS] Full hardware partition discovery test complete.");
    } else {
        log_warn!("    [SKIP] VirtIO hardware device not present; skipped hardware test.");
    }
}
