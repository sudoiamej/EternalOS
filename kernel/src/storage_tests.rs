use crate::block::{BlockDevice, BlockDeviceInfo, BlockError, validate_request};
use crate::virtio_blk;
use crate::{log_info, log_warn};

/// Synthetic in-memory RAM block device for deterministic offline unit testing.
pub struct RamBlockDevice {
    info: BlockDeviceInfo,
    storage: &'static mut [u8],
}

impl RamBlockDevice {
    pub fn new(storage: &'static mut [u8], sector_size: usize, read_only: bool) -> Self {
        let total_sectors = (storage.len() / sector_size) as u64;
        let info = BlockDeviceInfo::new("Synthetic RAM Disk", total_sectors, sector_size, read_only);
        Self { info, storage }
    }
}

impl BlockDevice for RamBlockDevice {
    fn info(&self) -> BlockDeviceInfo {
        self.info
    }

    fn read_sectors(&mut self, start_sector: u64, sector_count: u32, buf: &mut [u8]) -> Result<(), BlockError> {
        validate_request(start_sector, sector_count, buf.len(), self.info.total_sectors, self.info.sector_size)?;

        let byte_offset = (start_sector as usize) * self.info.sector_size;
        let byte_count = (sector_count as usize) * self.info.sector_size;

        buf[..byte_count].copy_from_slice(&self.storage[byte_offset..byte_offset + byte_count]);
        Ok(())
    }

    fn write_sectors(&mut self, start_sector: u64, sector_count: u32, buf: &[u8]) -> Result<(), BlockError> {
        validate_request(start_sector, sector_count, buf.len(), self.info.total_sectors, self.info.sector_size)?;

        if self.info.read_only {
            return Err(BlockError::ReadOnly);
        }

        let byte_offset = (start_sector as usize) * self.info.sector_size;
        let byte_count = (sector_count as usize) * self.info.sector_size;

        self.storage[byte_offset..byte_offset + byte_count].copy_from_slice(&buf[..byte_count]);
        Ok(())
    }
}

// Statically allocated RAM disk storage (32 sectors * 512 = 16 KiB)
static mut RAM_STORAGE: [u8; 16384] = [0; 16384];
static mut TEST_IO_BUFFER: [u8; 4096] = [0; 4096];
static mut TEST_VERIFY_BUFFER: [u8; 4096] = [0; 4096];

pub fn run_phase13b_storage_tests() {
    log_info!("====================================================");
    log_info!("   [TEST SUITE] Phase 13B: Block Storage & VirtIO   ");
    log_info!("====================================================");

    test_request_validation();
    test_ramdisk_roundtrip();
    test_ramdisk_multi_sector();
    test_ramdisk_readonly();
    test_hardware_virtio_io();

    log_info!(">>> ALL PHASE 13B STORAGE TESTS PASSED SUCCESSFULLY!");
    log_info!("====================================================");
}

/// Test 1: Bounds, overflow, and argument validation.
fn test_request_validation() {
    log_info!(">>> [TEST 1/5] Validating Sector Ranges & Arithmetic...");

    // Case 1: Zero sector request
    assert_eq!(
        validate_request(0, 0, 512, 100, 512),
        Err(BlockError::ZeroSectorsRequested),
        "Zero sector request should be rejected"
    );

    // Case 2: Out of bounds (exceeding total capacity)
    assert_eq!(
        validate_request(95, 10, 5120, 100, 512),
        Err(BlockError::OutOfBounds),
        "Request extending past total sectors must be rejected"
    );

    // Case 3: Buffer too small
    assert_eq!(
        validate_request(0, 2, 512, 100, 512),
        Err(BlockError::BufferTooSmall),
        "Buffer smaller than sector_count * sector_size must be rejected"
    );

    // Case 4: Arithmetic overflow in sector addition
    assert_eq!(
        validate_request(u64::MAX - 2, 5, 5120, 1000, 512),
        Err(BlockError::ArithmeticOverflow),
        "Sector arithmetic overflow must be caught"
    );

    // Case 5: Valid request
    assert!(
        validate_request(0, 1, 512, 100, 512).is_ok(),
        "Valid request should pass validation"
    );

    log_info!("    [PASS] Request validation & arithmetic guards verified.");
}

/// Test 2: RAM disk single sector write-then-read roundtrip.
fn test_ramdisk_roundtrip() {
    log_info!(">>> [TEST 2/5] RAM Disk Single Sector Roundtrip...");

    let ram_slice = &raw mut RAM_STORAGE;
    let mut dev = RamBlockDevice::new(unsafe { &mut *ram_slice }, 512, false);

    let write_buf = unsafe { &mut *(&raw mut TEST_IO_BUFFER) };
    let read_buf = unsafe { &mut *(&raw mut TEST_VERIFY_BUFFER) };

    // Fill with known test pattern
    for (i, byte) in write_buf[..512].iter_mut().enumerate() {
        *byte = (i ^ 0xA5) as u8;
    }

    // Write to sector 3
    dev.write_sectors(3, 1, &write_buf[..512]).expect("RAM disk write failed");

    // Read back from sector 3
    dev.read_sectors(3, 1, &mut read_buf[..512]).expect("RAM disk read failed");

    assert_eq!(&write_buf[..512], &read_buf[..512], "Data read back must match data written");
    log_info!("    [PASS] Single sector read-after-write data integrity matched.");
}

/// Test 3: Multi-sector read/write integrity.
fn test_ramdisk_multi_sector() {
    log_info!(">>> [TEST 3/5] RAM Disk Multi-Sector (4 Sectors) Roundtrip...");

    let ram_slice = &raw mut RAM_STORAGE;
    let mut dev = RamBlockDevice::new(unsafe { &mut *ram_slice }, 512, false);

    let write_buf = unsafe { &mut *(&raw mut TEST_IO_BUFFER) };
    let read_buf = unsafe { &mut *(&raw mut TEST_VERIFY_BUFFER) };

    let total_bytes = 4 * 512;
    for (i, byte) in write_buf[..total_bytes].iter_mut().enumerate() {
        *byte = ((i * 7) & 0xFF) as u8;
    }

    // Write 4 sectors starting at sector 8
    dev.write_sectors(8, 4, &write_buf[..total_bytes]).expect("Multi-sector write failed");

    // Read back 4 sectors
    dev.read_sectors(8, 4, &mut read_buf[..total_bytes]).expect("Multi-sector read failed");

    assert_eq!(&write_buf[..total_bytes], &read_buf[..total_bytes], "Multi-sector data corrupted");
    log_info!("    [PASS] Multi-sector roundtrip data integrity verified.");
}

/// Test 4: Read-only protection enforcement.
fn test_ramdisk_readonly() {
    log_info!(">>> [TEST 4/5] Enforcing Read-Only Device Policy...");

    let ram_slice = &raw mut RAM_STORAGE;
    let mut dev = RamBlockDevice::new(unsafe { &mut *ram_slice }, 512, true);

    let buf = [0x55u8; 512];
    let res = dev.write_sectors(0, 1, &buf);

    assert_eq!(res, Err(BlockError::ReadOnly), "Writing to read-only disk must be rejected");
    log_info!("    [PASS] Read-only enforcement verified.");
}

/// Test 5: Real VirtIO hardware sector read & write on attached QEMU test disk.
fn test_hardware_virtio_io() {
    log_info!(">>> [TEST 5/5] Real Hardware VirtIO Block Device I/O...");

    let io_tested = virtio_blk::with_block_device(|dev| {
        let info = dev.info();
        log_info!("    Hardware device: {} (Capacity: {} sectors, Sector Size: {} bytes)",
            info.name_as_str(), info.total_sectors, info.sector_size);

        assert!(info.total_sectors > 0, "VirtIO Block reported zero capacity");
        assert_eq!(info.sector_size, 512, "Expected 512-byte logical sectors");

        let write_buf = unsafe { &mut *(&raw mut TEST_IO_BUFFER) };
        let read_buf = unsafe { &mut *(&raw mut TEST_VERIFY_BUFFER) };

        // Test sector 16 (isolated from boot sectors)
        let test_sector = 16u64;

        // Fill test pattern
        for (i, byte) in write_buf[..512].iter_mut().enumerate() {
            *byte = (0xBE ^ (i as u8)) ^ 0xEF;
        }

        // Perform hardware DMA write
        log_info!("    Issuing DMA write command to sector {}...", test_sector);
        dev.write_sectors(test_sector, 1, &write_buf[..512])
            .expect("VirtIO hardware write failed");

        // Clear read buffer
        read_buf[..512].fill(0);

        // Perform hardware DMA read
        log_info!("    Issuing DMA read command from sector {}...", test_sector);
        dev.read_sectors(test_sector, 1, &mut read_buf[..512])
            .expect("VirtIO hardware read failed");

        // Verify data integrity
        assert_eq!(&write_buf[..512], &read_buf[..512], "VirtIO read-after-write payload mismatch!");
        log_info!("    [PASS] VirtIO hardware read-after-write verification matched!");

        // Test Multi-sector Hardware I/O (2 sectors = 1024 bytes)
        log_info!("    Issuing multi-sector (2 sectors) DMA write to sector 20...",);
        for (i, byte) in write_buf[..1024].iter_mut().enumerate() {
            *byte = ((i + 42) & 0xFF) as u8;
        }
        dev.write_sectors(20, 2, &write_buf[..1024])
            .expect("VirtIO multi-sector write failed");

        read_buf[..1024].fill(0);
        dev.read_sectors(20, 2, &mut read_buf[..1024])
            .expect("VirtIO multi-sector read failed");

        assert_eq!(&write_buf[..1024], &read_buf[..1024], "VirtIO multi-sector payload mismatch!");
        log_info!("    [PASS] VirtIO multi-sector hardware DMA verified!");
        true
    });

    if io_tested == Some(true) {
        log_info!("    [PASS] Full VirtIO hardware block test succeeded.");
    } else {
        log_warn!("    [SKIP] VirtIO hardware device not present on PCI bus; skipped hardware I/O.");
    }
}
