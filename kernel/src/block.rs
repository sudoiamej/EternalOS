use core::fmt;

/// Standard logical sector size (512 bytes for standard disks).
pub const DEFAULT_SECTOR_SIZE: usize = 512;

/// Detailed errors for block device operations.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BlockError {
    /// Device is not initialized or not responding.
    NotReady,
    /// Requested sector range exceeds device capacity.
    OutOfBounds,
    /// Provided buffer length is not aligned to the device logical sector size.
    MisalignedBuffer,
    /// Buffer is too small to receive or provide requested sector count.
    BufferTooSmall,
    /// Zero sectors requested.
    ZeroSectorsRequested,
    /// Arithmetic overflow in sector address or byte offset computation.
    ArithmeticOverflow,
    /// Controller reported hardware error during command execution.
    HardwareError(u32),
    /// Device timed out waiting for ready, busy clear, or command completion.
    Timeout,
    /// Device is read-only (for write attempts).
    ReadOnly,
    /// Memory mapping or physical frame allocation failure for DMA buffers.
    MemoryError,
    /// Feature or command unsupported by the device.
    Unsupported,
}

impl fmt::Display for BlockError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            BlockError::NotReady => write!(f, "Block device is not ready"),
            BlockError::OutOfBounds => write!(f, "Sector range is out of device bounds"),
            BlockError::MisalignedBuffer => write!(f, "Buffer length is not aligned to sector size"),
            BlockError::BufferTooSmall => write!(f, "Buffer is too small for requested sector count"),
            BlockError::ZeroSectorsRequested => write!(f, "Zero sectors requested"),
            BlockError::ArithmeticOverflow => write!(f, "Arithmetic overflow in sector calculation"),
            BlockError::HardwareError(code) => write!(f, "Hardware error from controller: 0x{:x}", code),
            BlockError::Timeout => write!(f, "Block device operation timed out"),
            BlockError::ReadOnly => write!(f, "Block device is read-only"),
            BlockError::MemoryError => write!(f, "DMA or memory buffer allocation failure"),
            BlockError::Unsupported => write!(f, "Operation unsupported by device"),
        }
    }
}

/// Static description and properties of a block storage device.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BlockDeviceInfo {
    /// Human-readable device name / model (e.g., "VirtIO Block Device", "AHCI Port 0").
    pub name: [u8; 32],
    pub name_len: usize,
    /// Total number of logical sectors on the device.
    pub total_sectors: u64,
    /// Size of each logical sector in bytes (e.g., 512, 4096).
    pub sector_size: usize,
    /// Whether the device supports sector writes.
    pub read_only: bool,
}

impl BlockDeviceInfo {
    pub const fn empty() -> Self {
        Self {
            name: [0; 32],
            name_len: 0,
            total_sectors: 0,
            sector_size: 512,
            read_only: false,
        }
    }

    pub fn new(name_str: &str, total_sectors: u64, sector_size: usize, read_only: bool) -> Self {
        let mut name = [0u8; 32];
        let bytes = name_str.as_bytes();
        let len = bytes.len().min(32);
        let mut i = 0;
        while i < len {
            name[i] = bytes[i];
            i += 1;
        }
        Self {
            name,
            name_len: len,
            total_sectors,
            sector_size,
            read_only,
        }
    }

    pub fn name_as_str(&self) -> &str {
        core::str::from_utf8(&self.name[..self.name_len]).unwrap_or("Unknown Block Device")
    }

    /// Total capacity in bytes. Returns None on arithmetic overflow.
    pub fn capacity_bytes(&self) -> Option<u64> {
        self.total_sectors.checked_mul(self.sector_size as u64)
    }
}

/// Hardware-independent trait for block storage devices.
pub trait BlockDevice {
    /// Return static device metadata and capacity information.
    fn info(&self) -> BlockDeviceInfo;

    /// Read `sector_count` consecutive sectors starting at `start_sector` into `buf`.
    /// `buf` must have at least `sector_count * sector_size` bytes.
    fn read_sectors(&mut self, start_sector: u64, sector_count: u32, buf: &mut [u8]) -> Result<(), BlockError>;

    /// Write `sector_count` consecutive sectors starting at `start_sector` from `buf`.
    /// `buf` must have at least `sector_count * sector_size` bytes.
    fn write_sectors(&mut self, start_sector: u64, sector_count: u32, buf: &[u8]) -> Result<(), BlockError>;

    /// Synchronize or flush volatile device caches to persistent media.
    fn flush(&mut self) -> Result<(), BlockError> {
        Ok(())
    }
}

/// Validate sector range and buffer bounds for a block operation.
/// Ensures:
/// 1. `sector_count > 0`
/// 2. `start_sector + sector_count <= total_sectors` without overflow
/// 3. `sector_count * sector_size <= buf_len` without overflow
pub fn validate_request(
    start_sector: u64,
    sector_count: u32,
    buf_len: usize,
    total_sectors: u64,
    sector_size: usize,
) -> Result<(), BlockError> {
    if sector_count == 0 {
        return Err(BlockError::ZeroSectorsRequested);
    }
    if sector_size == 0 {
        return Err(BlockError::Unsupported);
    }

    // Check end sector bounds without overflow
    let end_sector = start_sector
        .checked_add(sector_count as u64)
        .ok_or(BlockError::ArithmeticOverflow)?;

    if end_sector > total_sectors {
        return Err(BlockError::OutOfBounds);
    }

    // Check required buffer length
    let required_bytes = (sector_count as usize)
        .checked_mul(sector_size)
        .ok_or(BlockError::ArithmeticOverflow)?;

    if buf_len < required_bytes {
        return Err(BlockError::BufferTooSmall);
    }

    Ok(())
}
