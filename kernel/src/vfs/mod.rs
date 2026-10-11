use core::fmt;

pub mod path;
pub mod ramfs;
pub mod router;

pub use router::{init, with_vfs, VfsRouter};
pub use path::normalize_path;
pub use ramfs::RamFs;

/// Maximum length of a single path component or file name.
pub const MAX_NAME_LEN: usize = 64;

/// Maximum length of an absolute or normalized path.
pub const MAX_PATH_LEN: usize = 256;

/// Maximum open file handles in the system.
pub const MAX_OPEN_FILES: usize = 32;

/// Maximum concurrent mount points.
pub const MAX_MOUNTS: usize = 8;

/// Inode or object identifier in the VFS.
pub type InodeId = u64;

/// Unified VFS error codes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VfsError {
    /// Target file or directory was not found.
    NotFound,
    /// An entity with the same name already exists.
    AlreadyExists,
    /// Operation expected a directory but found a regular file.
    NotADirectory,
    /// Operation expected a regular file but found a directory.
    IsADirectory,
    /// Operation invalid for this node type or state.
    InvalidArgument,
    /// Directory is not empty.
    DirectoryNotEmpty,
    /// Permission denied or write attempt on read-only mount/file.
    PermissionDenied,
    /// No available file descriptor or handle in the table.
    TableOverflow,
    /// Device or filesystem out of storage space / memory capacity.
    NoSpace,
    /// Provided path is empty, invalid, or exceeds `MAX_PATH_LEN`.
    InvalidPath,
    /// File handle is stale, closed, or invalid.
    InvalidHandle,
    /// Operation not supported by filesystem implementation.
    NotSupported,
    /// Mount point not found or duplicate mount point.
    MountError,
    /// Internal I/O or storage backend failure.
    IoError,
    /// Arithmetic overflow in offset or size calculation.
    ArithmeticOverflow,
}

impl fmt::Display for VfsError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            VfsError::NotFound => write!(f, "File or directory not found"),
            VfsError::AlreadyExists => write!(f, "File or directory already exists"),
            VfsError::NotADirectory => write!(f, "Not a directory"),
            VfsError::IsADirectory => write!(f, "Is a directory"),
            VfsError::InvalidArgument => write!(f, "Invalid argument"),
            VfsError::DirectoryNotEmpty => write!(f, "Directory is not empty"),
            VfsError::PermissionDenied => write!(f, "Permission denied"),
            VfsError::TableOverflow => write!(f, "VFS handle or mount table overflow"),
            VfsError::NoSpace => write!(f, "Filesystem out of space or capacity"),
            VfsError::InvalidPath => write!(f, "Invalid or oversized path"),
            VfsError::InvalidHandle => write!(f, "Invalid or stale file handle"),
            VfsError::NotSupported => write!(f, "Operation not supported"),
            VfsError::MountError => write!(f, "Mount table error"),
            VfsError::IoError => write!(f, "I/O backend failure"),
            VfsError::ArithmeticOverflow => write!(f, "Arithmetic overflow in file calculation"),
        }
    }
}

/// Node types supported by the VFS.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NodeType {
    RegularFile,
    Directory,
}

/// Metadata describing a filesystem node.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Metadata {
    pub inode: InodeId,
    pub node_type: NodeType,
    pub size: u64,
    pub read_only: bool,
}

/// Directory entry for enumeration.
#[derive(Debug, Clone, Copy)]
pub struct DirEntry {
    pub inode: InodeId,
    pub node_type: NodeType,
    pub name: [u8; MAX_NAME_LEN],
    pub name_len: usize,
}

impl DirEntry {
    pub const fn empty() -> Self {
        Self {
            inode: 0,
            node_type: NodeType::RegularFile,
            name: [0; MAX_NAME_LEN],
            name_len: 0,
        }
    }

    pub fn new(inode: InodeId, node_type: NodeType, name_str: &str) -> Self {
        let mut name = [0u8; MAX_NAME_LEN];
        let bytes = name_str.as_bytes();
        let len = bytes.len().min(MAX_NAME_LEN);
        let mut i = 0;
        while i < len {
            name[i] = bytes[i];
            i += 1;
        }
        Self {
            inode,
            node_type,
            name,
            name_len: len,
        }
    }

    pub fn name_as_str(&self) -> &str {
        core::str::from_utf8(&self.name[..self.name_len]).unwrap_or("")
    }
}

/// File open mode / access flags.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct OpenFlags {
    pub read: bool,
    pub write: bool,
    pub create: bool,
    pub truncate: bool,
    pub append: bool,
}

impl OpenFlags {
    pub const READ_ONLY: Self = Self {
        read: true,
        write: false,
        create: false,
        truncate: false,
        append: false,
    };

    pub const READ_WRITE: Self = Self {
        read: true,
        write: true,
        create: false,
        truncate: false,
        append: false,
    };

    pub const CREATE_OR_WRITE: Self = Self {
        read: true,
        write: true,
        create: true,
        truncate: false,
        append: false,
    };
}

/// Safe token representing an open file handle.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FileHandle {
    pub fd: usize,
    pub generation: u32,
}

/// Trait representing an abstract filesystem instance.
pub trait FileSystem: Send + Sync {
    /// Return the root inode ID of this filesystem.
    fn root_inode(&self) -> InodeId;

    /// Look up a child node by name inside parent directory `parent`.
    fn lookup(&self, parent: InodeId, name: &str) -> Result<InodeId, VfsError>;

    /// Retrieve metadata for `inode`.
    fn get_metadata(&self, inode: InodeId) -> Result<Metadata, VfsError>;

    /// Create a regular file under directory `parent`.
    fn create_file(&mut self, parent: InodeId, name: &str) -> Result<InodeId, VfsError>;

    /// Create a subdirectory under directory `parent`.
    fn create_dir(&mut self, parent: InodeId, name: &str) -> Result<InodeId, VfsError>;

    /// Read data from `inode` starting at `offset`.
    fn read(&self, inode: InodeId, offset: u64, buf: &mut [u8]) -> Result<usize, VfsError>;

    /// Write data to `inode` starting at `offset`.
    fn write(&mut self, inode: InodeId, offset: u64, buf: &[u8]) -> Result<usize, VfsError>;

    /// Truncate file `inode` to `new_size` bytes.
    fn truncate(&mut self, inode: InodeId, new_size: u64) -> Result<(), VfsError>;

    /// Enumerate directory entries of `inode`. Returns number of entries written into `out`.
    fn readdir(&self, inode: InodeId, offset: usize, out: &mut [DirEntry]) -> Result<usize, VfsError>;
}
