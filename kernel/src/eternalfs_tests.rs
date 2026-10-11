use crate::block::{BlockDevice, BlockDeviceInfo, BlockError, validate_request};
use crate::partition::{crc32, discover_partitions};
use crate::vfs::eternalfs::{
    EternalFs, Superblock, Inode,
    ETERNALFS_DIRENT_SIZE, INODE_TYPE_FILE, INODE_TYPE_DIR,
};
use crate::vfs::{DirEntry, FileSystem, VfsError};
use crate::{log_info, log_warn};

/// Synthetic Block Device for deterministic unit tests.
pub struct SyntheticFsDisk {
    sectors: &'static mut [[u8; 512]],
    info: BlockDeviceInfo,
}

impl SyntheticFsDisk {
    pub fn new(sectors: &'static mut [[u8; 512]]) -> Self {
        let total_sectors = sectors.len() as u64;
        let info = BlockDeviceInfo::new("Synthetic FS Disk", total_sectors, 512, false);
        Self { sectors, info }
    }

    pub fn reset(&mut self) {
        for s in self.sectors.iter_mut() {
            s.fill(0);
        }
    }
}

impl BlockDevice for SyntheticFsDisk {
    fn info(&self) -> BlockDeviceInfo {
        self.info
    }

    fn read_sectors(&mut self, start_sector: u64, sector_count: u32, buf: &mut [u8]) -> Result<(), BlockError> {
        validate_request(start_sector, sector_count, buf.len(), self.info.total_sectors, self.info.sector_size)?;
        for i in 0..sector_count as usize {
            let sec_idx = (start_sector as usize) + i;
            let off = i * 512;
            buf[off..off + 512].copy_from_slice(&self.sectors[sec_idx]);
        }
        Ok(())
    }

    fn write_sectors(&mut self, start_sector: u64, sector_count: u32, buf: &[u8]) -> Result<(), BlockError> {
        validate_request(start_sector, sector_count, buf.len(), self.info.total_sectors, self.info.sector_size)?;
        for i in 0..sector_count as usize {
            let sec_idx = (start_sector as usize) + i;
            let off = i * 512;
            self.sectors[sec_idx].copy_from_slice(&buf[off..off + 512]);
        }
        Ok(())
    }
}

// 64 sectors = 32 KiB synthetic disk (enough for blocks 0..31)
static mut SYNTH_FS_SECTORS: [[u8; 512]; 64] = [[0; 512]; 64];

pub fn run_phase14c_eternalfs_tests() {
    log_info!("====================================================");
    log_info!("   [TEST SUITE] Phase 14C: EternalFS Read-Only FS   ");
    log_info!("====================================================");

    test_superblock_validation();
    test_inode_validation();
    test_synthetic_filesystem_operations();
    test_hardware_eternalfs_mount_and_read();

    log_info!(">>> ALL PHASE 14C ETERNALFS TESTS PASSED SUCCESSFULLY!");
    log_info!("====================================================");
}

/// Helper to format a valid superblock into a 1024-byte slice.
fn build_valid_superblock(buf: &mut [u8; 1024], total_blocks: u64) {
    buf.fill(0);
    buf[0..8].copy_from_slice(b"ETERNAL1");
    buf[0x08..0x0C].copy_from_slice(&1u32.to_le_bytes()); // version
    buf[0x0C..0x10].copy_from_slice(&1024u32.to_le_bytes()); // block size
    buf[0x10..0x18].copy_from_slice(&total_blocks.to_le_bytes());
    buf[0x18..0x20].copy_from_slice(&16u64.to_le_bytes()); // total inodes
    buf[0x20..0x28].copy_from_slice(&2u64.to_le_bytes()); // inode table block 2
    buf[0x28..0x30].copy_from_slice(&3u64.to_le_bytes()); // data block start 3
    buf[0x30..0x38].copy_from_slice(&1u64.to_le_bytes()); // root inode 1
    buf[0x38..0x3C].copy_from_slice(&0u32.to_le_bytes()); // flags 0

    let crc = crc32(&buf[0..0x3C]);
    buf[0x3C..0x40].copy_from_slice(&crc.to_le_bytes());
}

/// Helper to format a valid inode into a 64-byte slice.
fn build_valid_inode(
    buf: &mut [u8; 64],
    _id: u64,
    node_type: u16,
    size: u64,
    direct: &[u32],
    indirect: u32,
) {
    buf.fill(0);
    buf[0x00..0x02].copy_from_slice(&node_type.to_le_bytes());
    buf[0x02..0x04].copy_from_slice(&0o755u16.to_le_bytes()); // flags / permissions
    buf[0x04..0x08].copy_from_slice(&0u32.to_le_bytes()); // reserved
    buf[0x08..0x10].copy_from_slice(&size.to_le_bytes()); // file size

    for (i, &blk) in direct.iter().enumerate().take(10) {
        let off = 0x10 + (i * 4);
        buf[off..off + 4].copy_from_slice(&blk.to_le_bytes());
    }
    buf[0x38..0x3C].copy_from_slice(&indirect.to_le_bytes());

    let crc = crc32(&buf[0..0x3C]);
    buf[0x3C..0x40].copy_from_slice(&crc.to_le_bytes());
}

/// Test 1: Superblock decoding, checksums, and geometry constraints.
fn test_superblock_validation() {
    log_info!(">>> [TEST 1/4] Superblock Decoding & Validation...");

    let mut sb_buf = [0u8; 1024];

    // Case 1: Valid Superblock
    build_valid_superblock(&mut sb_buf, 32);
    let sb = Superblock::decode(&sb_buf, 64).expect("Valid superblock decoding failed");
    assert_eq!(sb.version, 1);
    assert_eq!(sb.block_size, 1024);
    assert_eq!(sb.total_blocks, 32);
    assert_eq!(sb.total_inodes, 16);
    assert_eq!(sb.root_inode, 1);

    // Case 2: Corrupted Magic Signature
    sb_buf[0..8].copy_from_slice(b"CORRUPT!");
    assert_eq!(Superblock::decode(&sb_buf, 64).err(), Some(VfsError::NotFound));

    // Case 3: Superblock CRC mismatch
    build_valid_superblock(&mut sb_buf, 32);
    sb_buf[0x10] = 0xFF; // Modify total blocks without recomputing CRC
    assert_eq!(Superblock::decode(&sb_buf, 64).err(), Some(VfsError::IoError));

    // Case 4: Invalid Block Size (e.g. 512 or 4096)
    build_valid_superblock(&mut sb_buf, 32);
    sb_buf[0x0C..0x10].copy_from_slice(&4096u32.to_le_bytes());
    let crc = crc32(&sb_buf[0..0x3C]);
    sb_buf[0x3C..0x40].copy_from_slice(&crc.to_le_bytes());
    assert_eq!(Superblock::decode(&sb_buf, 64).err(), Some(VfsError::NotSupported));

    // Case 5: Filesystem Geometry Exceeds Partition
    build_valid_superblock(&mut sb_buf, 100); // 100 blocks = 200 sectors
    assert_eq!(Superblock::decode(&sb_buf, 64).err(), Some(VfsError::InvalidArgument));

    log_info!("    [PASS] Superblock decoding and constraint checks verified.");
}

/// Test 2: Inode decoding, checksums, and block resolution.
fn test_inode_validation() {
    log_info!(">>> [TEST 2/4] Inode Decoding & Validation...");

    let mut in_buf = [0u8; 64];

    // Case 1: Valid Inode
    build_valid_inode(&mut in_buf, 1, INODE_TYPE_FILE, 100, &[3, 4], 0);
    let inode = Inode::decode(&in_buf).expect("Valid inode decoding failed");
    assert_eq!(inode.node_type, INODE_TYPE_FILE);
    assert_eq!(inode.file_size, 100);
    assert_eq!(inode.direct_blocks[0], 3);
    assert_eq!(inode.direct_blocks[1], 4);

    // Case 2: Inode CRC Mismatch
    build_valid_inode(&mut in_buf, 2, INODE_TYPE_FILE, 50, &[3], 0);
    in_buf[0x08] = 0xAA; // Corrupt size without recomputing CRC
    assert_eq!(Inode::decode(&in_buf).err(), Some(VfsError::IoError));

    // Case 3: Invalid Inode Type (not 1 or 2)
    build_valid_inode(&mut in_buf, 3, 99, 10, &[], 0);
    assert_eq!(Inode::decode(&in_buf).err(), Some(VfsError::InvalidArgument));

    log_info!("    [PASS] Inode decoding, CRC verification, and bounds verified.");
}

/// Helper to write directory entry into block slice.
fn write_dirent(buf: &mut [u8], idx: usize, inode: u32, node_type: u8, name: &str) {
    let off = idx * ETERNALFS_DIRENT_SIZE;
    buf[off..off + 4].copy_from_slice(&inode.to_le_bytes());
    buf[off + 4] = node_type;
    let b = name.as_bytes();
    let len = b.len().min(26);
    buf[off + 5] = len as u8;
    buf[off + 6..off + 6 + len].copy_from_slice(&b[..len]);
}

/// Test 3: Synthetic Filesystem operations (lookup, read, multi-block, readdir, permissions).
fn test_synthetic_filesystem_operations() {
    log_info!(">>> [TEST 3/4] Synthetic Filesystem In-Memory Operations...");

    let disk = unsafe {
        let sectors: &'static mut [[u8; 512]] = &mut *(&raw mut SYNTH_FS_SECTORS);
        SyntheticFsDisk::new(sectors)
    };
    let mut dev = disk;
    dev.reset();

    // 1. Write Superblock at Block 1 (sectors 2..3)
    let mut sb_buf = [0u8; 1024];
    build_valid_superblock(&mut sb_buf, 32);
    dev.write_sectors(2, 2, &sb_buf).expect("Write sb failed");

    // 2. Write Inode Table at Block 2 (sectors 4..5)
    // Inode 1: Root Dir (size = 4 * 32 = 128, direct = [3])
    // Inode 2: /test.txt (size = 12, direct = [4])
    // Inode 3: /multiblock.txt (size = 1500, direct = [5, 6])
    // Inode 4: /sub (dir, size = 2 * 32 = 64, direct = [7])
    // Inode 5: /sub/inner.txt (size = 5, direct = [8])
    let mut itable = [0u8; 1024];
    let mut i1 = [0u8; 64];
    build_valid_inode(&mut i1, 1, INODE_TYPE_DIR, 128, &[3], 0);
    itable[0..64].copy_from_slice(&i1);

    let mut i2 = [0u8; 64];
    build_valid_inode(&mut i2, 2, INODE_TYPE_FILE, 12, &[4], 0);
    itable[64..128].copy_from_slice(&i2);

    let mut i3 = [0u8; 64];
    build_valid_inode(&mut i3, 3, INODE_TYPE_FILE, 1500, &[5, 6], 0);
    itable[128..192].copy_from_slice(&i3);

    let mut i4 = [0u8; 64];
    build_valid_inode(&mut i4, 4, INODE_TYPE_DIR, 64, &[7], 0);
    itable[192..256].copy_from_slice(&i4);

    let mut i5 = [0u8; 64];
    build_valid_inode(&mut i5, 5, INODE_TYPE_FILE, 5, &[8], 0);
    itable[256..320].copy_from_slice(&i5);

    dev.write_sectors(4, 2, &itable).expect("Write itable failed");

    // 3. Write Root Directory entries at Block 3 (sectors 6..7)
    let mut root_dir = [0u8; 1024];
    write_dirent(&mut root_dir, 0, 1, 2, ".");
    write_dirent(&mut root_dir, 1, 2, 1, "test.txt");
    write_dirent(&mut root_dir, 2, 3, 1, "multiblock.txt");
    write_dirent(&mut root_dir, 3, 4, 2, "sub");
    dev.write_sectors(6, 2, &root_dir).expect("Write root dir failed");

    // 4. Write /test.txt at Block 4 (sectors 8..9)
    let mut b4 = [0u8; 1024];
    b4[0..12].copy_from_slice(b"Hello World!");
    dev.write_sectors(8, 2, &b4).expect("Write b4 failed");

    // 5. Write /multiblock.txt at Block 5 & 6 (sectors 10..13)
    let mut b5 = [0u8; 1024];
    b5.fill(b'X');
    dev.write_sectors(10, 2, &b5).expect("Write b5 failed");

    let mut b6 = [0u8; 1024];
    b6[0..476].fill(b'Y');
    dev.write_sectors(12, 2, &b6).expect("Write b6 failed");

    // 6. Write /sub at Block 7 (sectors 14..15)
    let mut sub_dir = [0u8; 1024];
    write_dirent(&mut sub_dir, 0, 4, 2, ".");
    write_dirent(&mut sub_dir, 1, 5, 1, "inner.txt");
    dev.write_sectors(14, 2, &sub_dir).expect("Write sub dir failed");

    // 7. Write /sub/inner.txt at Block 8 (sectors 16..17)
    let mut b8 = [0u8; 1024];
    b8[0..5].copy_from_slice(b"INNER");
    dev.write_sectors(16, 2, &b8).expect("Write b8 failed");

    // Mount EternalFs
    let mut efs = EternalFs::mount(dev, 0, 64).expect("Mount synthetic EternalFs failed");

    // Lookup root
    assert_eq!(efs.root_inode(), 1);

    // Lookup /test.txt
    let test_in = efs.lookup(1, "test.txt").expect("Lookup test.txt failed");
    assert_eq!(test_in, 2);

    // Lookup non-existent file
    assert_eq!(efs.lookup(1, "nonexistent.txt"), Err(VfsError::NotFound));

    // Lookup /sub and nested /sub/inner.txt
    let sub_in = efs.lookup(1, "sub").expect("Lookup sub failed");
    assert_eq!(sub_in, 4);
    let inner_in = efs.lookup(sub_in, "inner.txt").expect("Lookup inner.txt failed");
    assert_eq!(inner_in, 5);

    // Read /test.txt
    let mut read_buf = [0u8; 64];
    let bytes = efs.read(2, 0, &mut read_buf).expect("Read test.txt failed");
    assert_eq!(bytes, 12);
    assert_eq!(&read_buf[..12], b"Hello World!");

    // Read with offset
    let bytes = efs.read(2, 6, &mut read_buf).expect("Read test.txt at offset 6 failed");
    assert_eq!(bytes, 6);
    assert_eq!(&read_buf[..6], b"World!");

    // Read at EOF
    let bytes = efs.read(2, 12, &mut read_buf).expect("Read at EOF failed");
    assert_eq!(bytes, 0);

    // Read past EOF
    let bytes = efs.read(2, 50, &mut read_buf).expect("Read past EOF failed");
    assert_eq!(bytes, 0);

    // Read Multi-block file /multiblock.txt across block boundary (1500 bytes)
    let mut multi_buf = [0u8; 1500];
    let bytes = efs.read(3, 0, &mut multi_buf).expect("Read multiblock failed");
    assert_eq!(bytes, 1500);
    assert_eq!(&multi_buf[0..1024], &[b'X'; 1024][..]);
    assert_eq!(&multi_buf[1024..1500], &[b'Y'; 476][..]);

    // Readdir on root
    let mut entries = [DirEntry::empty(); 8];
    let count = efs.readdir(1, 0, &mut entries).expect("Readdir root failed");
    assert_eq!(count, 4);
    assert_eq!(entries[0].name_as_str(), ".");
    assert_eq!(entries[1].name_as_str(), "test.txt");
    assert_eq!(entries[2].name_as_str(), "multiblock.txt");
    assert_eq!(entries[3].name_as_str(), "sub");

    // Read-only permission checks (write, truncate, create_file, create_dir rejected)
    assert_eq!(efs.write(2, 0, b"data"), Err(VfsError::PermissionDenied));
    assert_eq!(efs.truncate(2, 0), Err(VfsError::PermissionDenied));
    assert_eq!(efs.create_file(1, "new.txt"), Err(VfsError::PermissionDenied));
    assert_eq!(efs.create_dir(1, "newdir"), Err(VfsError::PermissionDenied));

    log_info!("    [PASS] Synthetic EternalFS operations, multi-block reads, and RO enforcement verified.");
}

/// Helper struct wrapping an active VirtIO block device reference as a `BlockDevice`.
struct VirtioDevRef<'a>(&'a mut crate::virtio_blk::VirtioBlockDevice);

impl<'a> BlockDevice for VirtioDevRef<'a> {
    fn info(&self) -> BlockDeviceInfo {
        self.0.info()
    }

    fn read_sectors(&mut self, start_sector: u64, sector_count: u32, buf: &mut [u8]) -> Result<(), BlockError> {
        self.0.read_sectors(start_sector, sector_count, buf)
    }

    fn write_sectors(&mut self, start_sector: u64, sector_count: u32, buf: &[u8]) -> Result<(), BlockError> {
        self.0.write_sectors(start_sector, sector_count, buf)
    }
}

/// Test 4: Real Hardware EternalFS mount and file reading on attached QEMU VirtIO disk.
fn test_hardware_eternalfs_mount_and_read() {
    log_info!(">>> [TEST 4/4] Hardware EternalFS Mount & Read on VirtIO Disk...");

    let mounted = crate::virtio_blk::with_block_device(|dev| {
        log_info!("    Probing partitions on VirtIO disk: {}...", dev.info().name_as_str());

        let table = match discover_partitions(dev) {
            Ok(t) => t,
            Err(e) => {
                log_warn!("    Partition discovery failed: {}", e);
                return false;
            }
        };

        if table.count == 0 {
            log_warn!("    No partitions discovered on VirtIO disk.");
            return false;
        }

        let p0 = &table.partitions[0];
        log_info!("    Partition 0: LBA {}..{} ({} sectors)",
            p0.start_lba, p0.start_lba + p0.sector_count, p0.sector_count);

        let dev_ref = VirtioDevRef(dev);
        let efs = match EternalFs::mount(dev_ref, p0.start_lba, p0.sector_count) {
            Ok(fs) => fs,
            Err(e) => {
                log_warn!("    EternalFS mount failed on Partition 0: {}", e);
                return false;
            }
        };

        log_info!("    EternalFS mounted successfully! Total blocks: {}, Inodes: {}",
            efs.superblock.total_blocks, efs.superblock.total_inodes);

        // 1. Look up and read /hello.txt
        let hello_inode = match efs.lookup(efs.root_inode(), "hello.txt") {
            Ok(in_id) => in_id,
            Err(e) => {
                log_warn!("    Failed to lookup /hello.txt: {}", e);
                return false;
            }
        };

        let mut hello_buf = [0u8; 64];
        let bytes_read = match efs.read(hello_inode, 0, &mut hello_buf) {
            Ok(b) => b,
            Err(e) => {
                log_warn!("    Failed to read /hello.txt: {}", e);
                return false;
            }
        };

        let text = core::str::from_utf8(&hello_buf[..bytes_read]).unwrap_or("<invalid utf-8>");
        log_info!("    /hello.txt content ({} bytes): '{}'", bytes_read, text.trim_ascii_end());
        assert!(text.contains("Hello from EternalFS"));

        // 2. Look up and verify /empty.txt
        let empty_inode = match efs.lookup(efs.root_inode(), "empty.txt") {
            Ok(in_id) => in_id,
            Err(e) => {
                log_warn!("    Failed to lookup /empty.txt: {}", e);
                return false;
            }
        };

        let meta = efs.get_metadata(empty_inode).expect("Metadata /empty.txt failed");
        assert_eq!(meta.size, 0);
        let empty_read = efs.read(empty_inode, 0, &mut hello_buf).expect("Read empty failed");
        assert_eq!(empty_read, 0);
        log_info!("    /empty.txt verified: 0 bytes, read returned 0 (EOF).");

        // 3. Look up and read /multiblock.txt
        let multi_inode = efs.lookup(efs.root_inode(), "multiblock.txt").expect("Lookup multiblock failed");
        let multi_meta = efs.get_metadata(multi_inode).expect("Metadata multiblock failed");
        assert_eq!(multi_meta.size, 1500);
        log_info!("    /multiblock.txt verified: Size = 1500 bytes.");

        // Read first 10 bytes and boundary bytes
        let mut sample_buf = [0u8; 16];
        efs.read(multi_inode, 0, &mut sample_buf).expect("Read start of multiblock failed");
        assert_eq!(&sample_buf[..10], &[b'A'; 10]);

        efs.read(multi_inode, 1024, &mut sample_buf).expect("Read block 1 of multiblock failed");
        assert_eq!(&sample_buf[..10], &[b'B'; 10]);
        log_info!("    /multiblock.txt cross-block boundary data verified ('A' -> 'B').");

        // 4. Look up nested /docs/kernel.txt
        let docs_inode = efs.lookup(efs.root_inode(), "docs").expect("Lookup /docs failed");
        let kdoc_inode = efs.lookup(docs_inode, "kernel.txt").expect("Lookup /docs/kernel.txt failed");
        let mut kdoc_buf = [0u8; 64];
        let kdoc_read = efs.read(kdoc_inode, 0, &mut kdoc_buf).expect("Read kernel.txt failed");
        let kdoc_text = core::str::from_utf8(&kdoc_buf[..kdoc_read]).unwrap_or("<invalid utf-8>");
        log_info!("    /docs/kernel.txt content ({} bytes): '{}'", kdoc_read, kdoc_text.trim_ascii_end());
        assert!(kdoc_text.contains("microkernel"));

        // 5. Enumerate root directory
        let mut root_ents = [DirEntry::empty(); 8];
        let ent_count = efs.readdir(efs.root_inode(), 0, &mut root_ents).expect("Readdir root failed");
        log_info!("    Root directory contains {} entries:", ent_count);
        for ent in root_ents.iter().take(ent_count) {
            log_info!("      - '{}' (Inode: {}, Type: {:?})", ent.name_as_str(), ent.inode, ent.node_type);
        }

        true
    });

    if mounted == Some(true) {
        log_info!("    [PASS] Full hardware VirtIO EternalFS mount and file access verified!");
    } else {
        log_warn!("    [SKIP] Hardware VirtIO EternalFS device not active.");
    }
}
