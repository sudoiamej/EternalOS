use crate::vfs::path::normalize_path;
use crate::vfs::{
    with_vfs, DirEntry, FileSystem, NodeType, OpenFlags, RamFs, VfsError, MAX_PATH_LEN,
};
use crate::log_info;

pub fn run_phase14b_vfs_tests() {
    log_info!("====================================================");
    log_info!("   [TEST SUITE] Phase 14B: Virtual Filesystem (VFS) ");
    log_info!("====================================================");

    test_path_normalization();
    test_ramfs_direct_operations();
    test_vfs_file_lifecycle();
    test_vfs_directories_and_enumeration();
    test_vfs_handles_and_safety();

    log_info!(">>> ALL PHASE 14B VFS TESTS PASSED SUCCESSFULLY!");
    log_info!("====================================================");
}

/// Test 1: Path normalization, `.` and `..`, multiple slashes, and boundary checks.
fn test_path_normalization() {
    log_info!(">>> [TEST 1/5] Testing Path Normalization & Canonicalization...");

    let mut out = [0u8; MAX_PATH_LEN];

    // Case 1: Simple absolute path
    let len = normalize_path("/a/b/c", &mut out).expect("Normalize /a/b/c failed");
    assert_eq!(&out[..len], b"/a/b/c");

    // Case 2: Redundant slashes
    let len = normalize_path("///var///log///syslog", &mut out).expect("Normalize slashes failed");
    assert_eq!(&out[..len], b"/var/log/syslog");

    // Case 3: `.` current dir resolution
    let len = normalize_path("/usr/./bin/././rustc", &mut out).expect("Normalize . failed");
    assert_eq!(&out[..len], b"/usr/bin/rustc");

    // Case 4: `..` parent dir resolution
    let len = normalize_path("/home/user/../guest/docs", &mut out).expect("Normalize .. failed");
    assert_eq!(&out[..len], b"/home/guest/docs");

    // Case 5: `..` traversal above root clamped at root
    let len = normalize_path("/../../../../etc/passwd", &mut out).expect("Normalize above root failed");
    assert_eq!(&out[..len], b"/etc/passwd");

    // Case 6: Trailing slash removal
    let len = normalize_path("/home/eternalos/", &mut out).expect("Normalize trailing slash failed");
    assert_eq!(&out[..len], b"/home/eternalos");

    // Case 7: Root path
    let len = normalize_path("/", &mut out).expect("Normalize root failed");
    assert_eq!(&out[..len], b"/");
    let len = normalize_path("////", &mut out).expect("Normalize multiple root slashes failed");
    assert_eq!(&out[..len], b"/");

    // Case 8: Relative path rejection
    assert_eq!(normalize_path("relative/path", &mut out), Err(VfsError::InvalidPath));
    assert_eq!(normalize_path("", &mut out), Err(VfsError::InvalidPath));

    log_info!("    [PASS] Path canonicalization and boundary guards verified.");
}

static mut TEST_RAMFS: RamFs = RamFs::new();

/// Test 2: Direct RamFs isolated unit tests.
fn test_ramfs_direct_operations() {
    log_info!(">>> [TEST 2/5] Testing Isolated RamFs Backend Operations...");

    let fs = unsafe { &mut *(&raw mut TEST_RAMFS) };
    fs.init();
    let root = fs.root_inode();

    // 1. Verify root directory metadata
    let root_meta = fs.get_metadata(root).expect("Failed to get root metadata");
    assert_eq!(root_meta.node_type, NodeType::Directory);

    // 2. Create directory and child file
    let docs = fs.create_dir(root, "docs").expect("Failed to create docs dir");
    let file = fs.create_file(docs, "readme.txt").expect("Failed to create readme.txt");

    // 3. Duplicate name rejection
    assert_eq!(fs.create_file(docs, "readme.txt"), Err(VfsError::AlreadyExists));

    // 4. Write data to file
    let msg = b"Welcome to EternalOS VFS!";
    let written = fs.write(file, 0, msg).expect("Write failed");
    assert_eq!(written, msg.len());

    // 5. Read back data
    let mut read_buf = [0u8; 64];
    let read_bytes = fs.read(file, 0, &mut read_buf).expect("Read failed");
    assert_eq!(read_bytes, msg.len());
    assert_eq!(&read_buf[..read_bytes], msg);

    // 6. Truncation test
    fs.truncate(file, 7).expect("Truncation failed");
    let trunc_meta = fs.get_metadata(file).expect("Metadata after truncate failed");
    assert_eq!(trunc_meta.size, 7);
    let mut trunc_buf = [0u8; 16];
    let n = fs.read(file, 0, &mut trunc_buf).expect("Read after truncate failed");
    assert_eq!(n, 7);
    assert_eq!(&trunc_buf[..7], b"Welcome");

    // 7. Readdir enumeration
    let mut entries = [DirEntry::empty(); 8];
    let count = fs.readdir(docs, 0, &mut entries).expect("Readdir failed");
    assert_eq!(count, 1);
    assert_eq!(entries[0].name_as_str(), "readme.txt");

    log_info!("    [PASS] Isolated RamFs file/dir operations and truncation verified.");
}

/// Test 3: High-level VFS file creation, read/write, seek offset, and stat.
fn test_vfs_file_lifecycle() {
    log_info!(">>> [TEST 3/5] Testing VFS File Lifecycle & Read/Write API...");

    with_vfs(|vfs| {
        // 1. Create directory structure /system/cfg
        vfs.mkdir("/system").expect("mkdir /system failed");
        vfs.mkdir("/system/cfg").expect("mkdir /system/cfg failed");

        // 2. Open non-existent file without create flag -> NotFound
        assert_eq!(
            vfs.open("/system/cfg/kernel.conf", OpenFlags::READ_ONLY),
            Err(VfsError::NotFound)
        );

        // 3. Create and write file
        let handle = vfs
            .open("/system/cfg/kernel.conf", OpenFlags::CREATE_OR_WRITE)
            .expect("Create kernel.conf failed");

        let config_data = b"version=0.1.0\narch=x86_64\npreempt=true\n";
        let written = vfs.write(handle, config_data).expect("VFS write failed");
        assert_eq!(written, config_data.len());
        vfs.close(handle).expect("VFS close failed");

        // 4. Query metadata with stat
        let meta = vfs.stat("/system/cfg/kernel.conf").expect("stat failed");
        assert_eq!(meta.node_type, NodeType::RegularFile);
        assert_eq!(meta.size, config_data.len() as u64);

        // 5. Open for reading and verify contents
        let r_handle = vfs
            .open("/system/cfg/kernel.conf", OpenFlags::READ_ONLY)
            .expect("Open for read failed");
        let mut read_buf = [0u8; 128];
        let bytes_read = vfs.read(r_handle, &mut read_buf).expect("VFS read failed");
        assert_eq!(bytes_read, config_data.len());
        assert_eq!(&read_buf[..bytes_read], config_data);

        // Second read at EOF returns 0
        let eof = vfs.read(r_handle, &mut read_buf).expect("Read at EOF failed");
        assert_eq!(eof, 0);

        vfs.close(r_handle).expect("Close read handle failed");
    });

    log_info!("    [PASS] VFS file open, write, stat, sequential read, and EOF confirmed.");
}

/// Test 4: Nested directories, path traversal, and readdir enumeration.
fn test_vfs_directories_and_enumeration() {
    log_info!(">>> [TEST 4/5] Testing Nested Directories & Enumeration...");

    with_vfs(|vfs| {
        vfs.mkdir("/data").expect("mkdir /data failed");
        let h1 = vfs.open("/data/file1.bin", OpenFlags::CREATE_OR_WRITE).unwrap();
        vfs.close(h1).unwrap();
        let h2 = vfs.open("/data/file2.bin", OpenFlags::CREATE_OR_WRITE).unwrap();
        vfs.close(h2).unwrap();
        vfs.mkdir("/data/subdir").expect("mkdir /data/subdir failed");

        // Enumerate entries in /data
        let mut entries = [DirEntry::empty(); 8];
        let count = vfs.readdir("/data", 0, &mut entries).expect("readdir /data failed");
        assert_eq!(count, 3);

        // Verify lookup across `..` and `.` components
        let stat_dot = vfs.stat("/data/./file1.bin").expect("stat with . failed");
        assert_eq!(stat_dot.node_type, NodeType::RegularFile);

        let stat_dotdot = vfs.stat("/data/subdir/../file2.bin").expect("stat with .. failed");
        assert_eq!(stat_dotdot.node_type, NodeType::RegularFile);

        // Traversing through a file as a directory must be rejected
        assert_eq!(
            vfs.stat("/data/file1.bin/child"),
            Err(VfsError::NotADirectory)
        );
    });

    log_info!("    [PASS] Directory hierarchy, relative navigation, and readdir verified.");
}

/// Test 5: Handle safety, stale handle rejection, and bounds guards.
fn test_vfs_handles_and_safety() {
    log_info!(">>> [TEST 5/5] Testing Handle Safety, Stale Detection & Error Handling...");

    with_vfs(|vfs| {
        let h = vfs.open("/test_handle.txt", OpenFlags::CREATE_OR_WRITE).unwrap();
        vfs.close(h).unwrap();

        // 1. Reading from closed handle must be rejected with InvalidHandle
        let mut buf = [0u8; 16];
        assert_eq!(vfs.read(h, &mut buf), Err(VfsError::InvalidHandle));

        // 2. Writing to closed handle must be rejected
        assert_eq!(vfs.write(h, b"fail"), Err(VfsError::InvalidHandle));

        // 3. Closing already closed handle must be rejected
        assert_eq!(vfs.close(h), Err(VfsError::InvalidHandle));

        // 4. Stale handle generation test: open new file in slot, old handle with older generation rejected
        let h_new = vfs.open("/test_handle2.txt", OpenFlags::CREATE_OR_WRITE).unwrap();
        if h_new.fd == h.fd {
            assert_ne!(h_new.generation, h.generation);
            assert_eq!(vfs.read(h, &mut buf), Err(VfsError::InvalidHandle));
        }
        vfs.close(h_new).unwrap();

        // 5. Mount checks: "/" is mounted
        assert!(vfs.is_mounted("/"));
        assert!(!vfs.is_mounted("/nonexistent"));
    });

    log_info!("    [PASS] Handle generations, stale handle rejection, and mount checks verified.");
}
