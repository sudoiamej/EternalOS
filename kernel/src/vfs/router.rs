use crate::sync::SpinLock;
use crate::vfs::path::{normalize_path, PathComponentIterator};
use crate::vfs::ramfs::RamFs;
use crate::vfs::{
    DirEntry, FileHandle, FileSystem, InodeId, Metadata, NodeType, OpenFlags, VfsError,
    MAX_MOUNTS, MAX_OPEN_FILES, MAX_PATH_LEN,
};

/// An open file table entry.
#[derive(Clone, Copy)]
pub struct OpenFileEntry {
    pub in_use: bool,
    pub generation: u32,
    pub inode: InodeId,
    pub offset: u64,
    pub flags: OpenFlags,
}

impl OpenFileEntry {
    pub const fn empty() -> Self {
        Self {
            in_use: false,
            generation: 0,
            inode: 0,
            offset: 0,
            flags: OpenFlags::READ_ONLY,
        }
    }
}

/// A mount point binding a path prefix to a filesystem instance.
#[derive(Clone, Copy)]
pub struct MountPoint {
    pub in_use: bool,
    pub path: [u8; MAX_PATH_LEN],
    pub path_len: usize,
}

impl MountPoint {
    pub const fn empty() -> Self {
        Self {
            in_use: false,
            path: [0; MAX_PATH_LEN],
            path_len: 0,
        }
    }

    pub fn path_as_str(&self) -> &str {
        core::str::from_utf8(&self.path[..self.path_len]).unwrap_or("")
    }
}

/// Global Virtual Filesystem Router and Mount Manager.
pub struct VfsRouter {
    root_fs: RamFs,
    mounts: [MountPoint; MAX_MOUNTS],
    open_files: [OpenFileEntry; MAX_OPEN_FILES],
}

impl VfsRouter {
    pub const fn new() -> Self {
        Self {
            root_fs: RamFs::new(),
            mounts: [MountPoint::empty(); MAX_MOUNTS],
            open_files: [OpenFileEntry::empty(); MAX_OPEN_FILES],
        }
    }

    /// Initialize the VFS with root RamFs mounted at `/`.
    pub fn init(&mut self) {
        self.root_fs.init();
        for m in self.mounts.iter_mut() {
            *m = MountPoint::empty();
        }
        for f in self.open_files.iter_mut() {
            *f = OpenFileEntry::empty();
        }

        // Mount root at "/"
        self.mounts[0] = MountPoint {
            in_use: true,
            path: [b'/'; MAX_PATH_LEN],
            path_len: 1,
        };
    }

    /// Mount check helper: check if prefix matches an active mount point.
    pub fn is_mounted(&self, path: &str) -> bool {
        self.mounts
            .iter()
            .any(|m| m.in_use && m.path_as_str() == path)
    }

    /// Resolve an absolute path into an InodeId on the root filesystem.
    pub fn resolve_path(&self, path: &str) -> Result<InodeId, VfsError> {
        let mut canon_buf = [0u8; MAX_PATH_LEN];
        let len = normalize_path(path, &mut canon_buf)?;
        let canon_str = core::str::from_utf8(&canon_buf[..len]).map_err(|_| VfsError::InvalidPath)?;

        if canon_str == "/" {
            return Ok(self.root_fs.root_inode());
        }

        let mut curr_inode = self.root_fs.root_inode();
        for comp in PathComponentIterator::new(canon_str) {
            curr_inode = self.root_fs.lookup(curr_inode, comp)?;
        }

        Ok(curr_inode)
    }

    /// Open a file at `path` with `flags`.
    pub fn open(&mut self, path: &str, flags: OpenFlags) -> Result<FileHandle, VfsError> {
        let mut canon_buf = [0u8; MAX_PATH_LEN];
        let len = normalize_path(path, &mut canon_buf)?;
        let canon_str = core::str::from_utf8(&canon_buf[..len]).map_err(|_| VfsError::InvalidPath)?;

        if canon_str == "/" {
            return Err(VfsError::IsADirectory);
        }

        // Resolve or create file
        let inode = match self.resolve_path(canon_str) {
            Ok(in_id) => {
                let meta = self.root_fs.get_metadata(in_id)?;
                if meta.node_type == NodeType::Directory {
                    return Err(VfsError::IsADirectory);
                }
                if flags.truncate {
                    self.root_fs.truncate(in_id, 0)?;
                }
                in_id
            }
            Err(VfsError::NotFound) => {
                if !flags.create {
                    return Err(VfsError::NotFound);
                }

                // Split into parent directory path and file basename
                let last_slash = canon_str.rfind('/').ok_or(VfsError::InvalidPath)?;
                let parent_path = if last_slash == 0 { "/" } else { &canon_str[..last_slash] };
                let filename = &canon_str[last_slash + 1..];

                let parent_inode = self.resolve_path(parent_path)?;
                self.root_fs.create_file(parent_inode, filename)?
            }
            Err(e) => return Err(e),
        };

        // Allocate slot in open file table
        for (fd, slot) in self.open_files.iter_mut().enumerate() {
            if !slot.in_use {
                slot.in_use = true;
                slot.generation = slot.generation.wrapping_add(1);
                slot.inode = inode;
                slot.offset = if flags.append {
                    self.root_fs.get_metadata(inode)?.size
                } else {
                    0
                };
                slot.flags = flags;

                return Ok(FileHandle {
                    fd,
                    generation: slot.generation,
                });
            }
        }

        Err(VfsError::TableOverflow)
    }

    /// Validate and return mutable handle entry.
    fn get_open_file(&self, handle: FileHandle) -> Result<&OpenFileEntry, VfsError> {
        if handle.fd >= MAX_OPEN_FILES {
            return Err(VfsError::InvalidHandle);
        }
        let entry = &self.open_files[handle.fd];
        if !entry.in_use || entry.generation != handle.generation {
            return Err(VfsError::InvalidHandle);
        }
        Ok(entry)
    }

    fn get_open_file_mut(&mut self, handle: FileHandle) -> Result<&mut OpenFileEntry, VfsError> {
        if handle.fd >= MAX_OPEN_FILES {
            return Err(VfsError::InvalidHandle);
        }
        let entry = &mut self.open_files[handle.fd];
        if !entry.in_use || entry.generation != handle.generation {
            return Err(VfsError::InvalidHandle);
        }
        Ok(entry)
    }

    /// Read data through open file handle.
    pub fn read(&mut self, handle: FileHandle, buf: &mut [u8]) -> Result<usize, VfsError> {
        let entry = self.get_open_file(handle)?;
        if !entry.flags.read {
            return Err(VfsError::PermissionDenied);
        }
        let inode = entry.inode;
        let offset = entry.offset;

        let bytes_read = self.root_fs.read(inode, offset, buf)?;
        let entry_mut = self.get_open_file_mut(handle)?;
        entry_mut.offset = entry_mut
            .offset
            .checked_add(bytes_read as u64)
            .ok_or(VfsError::ArithmeticOverflow)?;

        Ok(bytes_read)
    }

    /// Write data through open file handle.
    pub fn write(&mut self, handle: FileHandle, buf: &[u8]) -> Result<usize, VfsError> {
        let entry = self.get_open_file(handle)?;
        if !entry.flags.write {
            return Err(VfsError::PermissionDenied);
        }
        let inode = entry.inode;
        let offset = entry.offset;

        let bytes_written = self.root_fs.write(inode, offset, buf)?;
        let entry_mut = self.get_open_file_mut(handle)?;
        entry_mut.offset = entry_mut
            .offset
            .checked_add(bytes_written as u64)
            .ok_or(VfsError::ArithmeticOverflow)?;

        Ok(bytes_written)
    }

    /// Close open file handle.
    pub fn close(&mut self, handle: FileHandle) -> Result<(), VfsError> {
        let entry = self.get_open_file_mut(handle)?;
        entry.in_use = false;
        Ok(())
    }

    /// Create a directory at `path`.
    pub fn mkdir(&mut self, path: &str) -> Result<(), VfsError> {
        let mut canon_buf = [0u8; MAX_PATH_LEN];
        let len = normalize_path(path, &mut canon_buf)?;
        let canon_str = core::str::from_utf8(&canon_buf[..len]).map_err(|_| VfsError::InvalidPath)?;

        if canon_str == "/" {
            return Err(VfsError::AlreadyExists);
        }

        let last_slash = canon_str.rfind('/').ok_or(VfsError::InvalidPath)?;
        let parent_path = if last_slash == 0 { "/" } else { &canon_str[..last_slash] };
        let dirname = &canon_str[last_slash + 1..];

        let parent_inode = self.resolve_path(parent_path)?;
        self.root_fs.create_dir(parent_inode, dirname)?;
        Ok(())
    }

    /// Enumerate directory entries at `path`.
    pub fn readdir(&self, path: &str, offset: usize, out: &mut [DirEntry]) -> Result<usize, VfsError> {
        let inode = self.resolve_path(path)?;
        let meta = self.root_fs.get_metadata(inode)?;
        if meta.node_type != NodeType::Directory {
            return Err(VfsError::NotADirectory);
        }
        self.root_fs.readdir(inode, offset, out)
    }

    /// Retrieve metadata for `path`.
    pub fn stat(&self, path: &str) -> Result<Metadata, VfsError> {
        let inode = self.resolve_path(path)?;
        self.root_fs.get_metadata(inode)
    }

    /// Unlink (remove) a file or empty directory at `path`.
    pub fn unlink(&mut self, path: &str) -> Result<(), VfsError> {
        let mut canon_buf = [0u8; MAX_PATH_LEN];
        let len = normalize_path(path, &mut canon_buf)?;
        let canon_str = core::str::from_utf8(&canon_buf[..len]).map_err(|_| VfsError::InvalidPath)?;

        if canon_str == "/" {
            return Err(VfsError::InvalidArgument);
        }

        let last_slash = canon_str.rfind('/').ok_or(VfsError::InvalidPath)?;
        let parent_path = if last_slash == 0 { "/" } else { &canon_str[..last_slash] };
        let basename = &canon_str[last_slash + 1..];

        let parent_inode = self.resolve_path(parent_path)?;
        self.root_fs.unlink(parent_inode, basename)
    }
}

static VFS_ROUTER: SpinLock<VfsRouter> = SpinLock::new(VfsRouter::new());

/// Global VFS Initialization
pub fn init() {
    let mut vfs = VFS_ROUTER.lock();
    vfs.init();
    crate::log_info!("[VFS] Virtual File System initialized with root RamFs at '/'.");
}

/// Access global VFS safely under lock.
pub fn with_vfs<F, R>(f: F) -> R
where
    F: FnOnce(&mut VfsRouter) -> R,
{
    let mut guard = VFS_ROUTER.lock();
    f(&mut guard)
}
