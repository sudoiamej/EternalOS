use crate::block::BlockDevice;
use crate::partition::crc32;
use crate::vfs::{
    DirEntry, FileSystem, InodeId, Metadata, NodeType, VfsError,
};

pub const ETERNALFS_MAGIC: &[u8; 8] = b"ETERNAL1";
pub const ETERNALFS_VERSION: u32 = 1;
pub const ETERNALFS_BLOCK_SIZE: usize = 1024;
pub const ETERNALFS_INODE_SIZE: usize = 64;
pub const ETERNALFS_DIRENT_SIZE: usize = 32;

pub const INODE_TYPE_FILE: u16 = 1;
pub const INODE_TYPE_DIR: u16 = 2;

/// Decoded Superblock representing an EternalFS volume.
#[derive(Debug, Clone, Copy)]
pub struct Superblock {
    pub version: u32,
    pub block_size: u32,
    pub total_blocks: u64,
    pub total_inodes: u64,
    pub inode_table_block: u64,
    pub data_block_start: u64,
    pub root_inode: u64,
    pub flags: u32,
}

impl Superblock {
    /// Decodes and validates superblock from 1024-byte block 1 slice.
    pub fn decode(buf: &[u8], partition_sectors: u64) -> Result<Self, VfsError> {
        if buf.len() < 1024 {
            return Err(VfsError::InvalidArgument);
        }

        // 1. Magic signature check
        if &buf[0..8] != ETERNALFS_MAGIC {
            return Err(VfsError::NotFound); // Not an EternalFS filesystem
        }

        // 2. CRC32 Checksum verification
        let expected_crc = u32::from_le_bytes(buf[0x3C..0x40].try_into().unwrap());
        let computed_crc = crc32(&buf[0x00..0x3C]);
        if expected_crc != computed_crc {
            return Err(VfsError::IoError);
        }

        // 3. Decode fields
        let version = u32::from_le_bytes(buf[0x08..0x0C].try_into().unwrap());
        if version != ETERNALFS_VERSION {
            return Err(VfsError::NotSupported);
        }

        let block_size = u32::from_le_bytes(buf[0x0C..0x10].try_into().unwrap());
        if block_size != ETERNALFS_BLOCK_SIZE as u32 {
            return Err(VfsError::NotSupported);
        }

        let total_blocks = u64::from_le_bytes(buf[0x10..0x18].try_into().unwrap());
        let total_inodes = u64::from_le_bytes(buf[0x18..0x20].try_into().unwrap());
        let inode_table_block = u64::from_le_bytes(buf[0x20..0x28].try_into().unwrap());
        let data_block_start = u64::from_le_bytes(buf[0x28..0x30].try_into().unwrap());
        let root_inode = u64::from_le_bytes(buf[0x30..0x38].try_into().unwrap());
        let flags = u32::from_le_bytes(buf[0x38..0x3C].try_into().unwrap());

        // 4. Validate geometry against partition capacity
        let total_fs_bytes = total_blocks
            .checked_mul(block_size as u64)
            .ok_or(VfsError::ArithmeticOverflow)?;
        let partition_bytes = partition_sectors
            .checked_mul(512)
            .ok_or(VfsError::ArithmeticOverflow)?;

        if total_fs_bytes > partition_bytes {
            return Err(VfsError::InvalidArgument);
        }

        if inode_table_block < 2 || data_block_start <= inode_table_block || data_block_start >= total_blocks {
            return Err(VfsError::InvalidArgument);
        }

        if root_inode == 0 || root_inode > total_inodes {
            return Err(VfsError::InvalidArgument);
        }

        Ok(Self {
            version,
            block_size,
            total_blocks,
            total_inodes,
            inode_table_block,
            data_block_start,
            root_inode,
            flags,
        })
    }
}

/// Decoded Inode representation.
#[derive(Debug, Clone, Copy)]
pub struct Inode {
    pub node_type: u16,
    pub flags: u16,
    pub file_size: u64,
    pub direct_blocks: [u32; 10],
    pub single_indirect: u32,
}

impl Inode {
    pub fn decode(buf: &[u8]) -> Result<Self, VfsError> {
        if buf.len() < 64 {
            return Err(VfsError::InvalidArgument);
        }

        let expected_crc = u32::from_le_bytes(buf[0x3C..0x40].try_into().unwrap());
        let computed_crc = crc32(&buf[0x00..0x3C]);
        if expected_crc != computed_crc {
            return Err(VfsError::IoError);
        }

        let node_type = u16::from_le_bytes(buf[0x00..0x02].try_into().unwrap());
        if node_type != INODE_TYPE_FILE && node_type != INODE_TYPE_DIR {
            return Err(VfsError::InvalidArgument);
        }

        let flags = u16::from_le_bytes(buf[0x02..0x04].try_into().unwrap());
        let file_size = u64::from_le_bytes(buf[0x08..0x10].try_into().unwrap());

        let mut direct_blocks = [0u32; 10];
        for i in 0..10 {
            let offset = 0x10 + (i * 4);
            direct_blocks[i] = u32::from_le_bytes(buf[offset..offset + 4].try_into().unwrap());
        }

        let single_indirect = u32::from_le_bytes(buf[0x38..0x3C].try_into().unwrap());

        Ok(Self {
            node_type,
            flags,
            file_size,
            direct_blocks,
            single_indirect,
        })
    }
}

/// Read-Only EternalFS Driver for an attached block device partition.
pub struct EternalFs<D: BlockDevice> {
    device: crate::sync::SpinLock<D>,
    partition_start_lba: u64,
    partition_sector_count: u64,
    pub superblock: Superblock,
}

impl<D: BlockDevice> EternalFs<D> {
    /// Mount an EternalFS filesystem from `device` bounded within `partition_start_lba..+partition_sector_count`.
    pub fn mount(
        mut device: D,
        partition_start_lba: u64,
        partition_sector_count: u64,
    ) -> Result<Self, VfsError> {
        if partition_sector_count < 4 {
            return Err(VfsError::InvalidArgument);
        }

        // Read Superblock from Block 1 (LBA offset 2..4 within partition)
        let mut sb_buf = [0u8; 1024];
        let sb_lba = partition_start_lba
            .checked_add(2)
            .ok_or(VfsError::ArithmeticOverflow)?;

        device
            .read_sectors(sb_lba, 2, &mut sb_buf)
            .map_err(|_| VfsError::IoError)?;

        let superblock = Superblock::decode(&sb_buf, partition_sector_count)?;

        Ok(Self {
            device: crate::sync::SpinLock::new(device),
            partition_start_lba,
            partition_sector_count,
            superblock,
        })
    }

    /// Read a 1024-byte filesystem block `fs_block_num` into `buf`.
    pub fn read_block(&self, fs_block_num: u64, buf: &mut [u8; 1024]) -> Result<(), VfsError> {
        if fs_block_num >= self.superblock.total_blocks {
            return Err(VfsError::InvalidArgument);
        }

        let lba_offset = fs_block_num
            .checked_mul(2)
            .ok_or(VfsError::ArithmeticOverflow)?;

        if lba_offset.checked_add(2).ok_or(VfsError::ArithmeticOverflow)? > self.partition_sector_count {
            return Err(VfsError::InvalidArgument);
        }

        let abs_lba = self
            .partition_start_lba
            .checked_add(lba_offset)
            .ok_or(VfsError::ArithmeticOverflow)?;

        let mut dev = self.device.lock();
        dev.read_sectors(abs_lba, 2, buf)
            .map_err(|_| VfsError::IoError)?;

        Ok(())
    }

    /// Read and decode Inode `inode_id` (1-indexed).
    pub fn read_inode(&self, inode_id: InodeId) -> Result<Inode, VfsError> {
        if inode_id == 0 || inode_id > self.superblock.total_inodes {
            return Err(VfsError::NotFound);
        }

        let index = inode_id - 1;
        let inodes_per_block = ETERNALFS_BLOCK_SIZE / ETERNALFS_INODE_SIZE; // 1024 / 64 = 16
        let block_offset = index / (inodes_per_block as u64);
        let index_within_block = (index % (inodes_per_block as u64)) as usize;

        let fs_block = self
            .superblock
            .inode_table_block
            .checked_add(block_offset)
            .ok_or(VfsError::ArithmeticOverflow)?;

        let mut block_buf = [0u8; 1024];
        self.read_block(fs_block, &mut block_buf)?;

        let offset = index_within_block * ETERNALFS_INODE_SIZE;
        Inode::decode(&block_buf[offset..offset + ETERNALFS_INODE_SIZE])
    }

    /// Resolve logical file block index to physical filesystem block index.
    pub fn resolve_block_pointer(&self, inode: &Inode, block_idx: usize) -> Result<Option<u64>, VfsError> {
        if block_idx < 10 {
            let direct = inode.direct_blocks[block_idx] as u64;
            if direct == 0 {
                Ok(None)
            } else {
                if direct < self.superblock.data_block_start || direct >= self.superblock.total_blocks {
                    return Err(VfsError::InvalidArgument);
                }
                Ok(Some(direct))
            }
        } else {
            let indirect_idx = block_idx - 10;
            if indirect_idx >= 256 || inode.single_indirect == 0 {
                return Ok(None);
            }

            let indirect_block = inode.single_indirect as u64;
            if indirect_block < self.superblock.data_block_start || indirect_block >= self.superblock.total_blocks {
                return Err(VfsError::InvalidArgument);
            }

            let mut ind_buf = [0u8; 1024];
            self.read_block(indirect_block, &mut ind_buf)?;

            let off = indirect_idx * 4;
            let target = u32::from_le_bytes(ind_buf[off..off + 4].try_into().unwrap()) as u64;
            if target == 0 {
                Ok(None)
            } else {
                if target < self.superblock.data_block_start || target >= self.superblock.total_blocks {
                    return Err(VfsError::InvalidArgument);
                }
                Ok(Some(target))
            }
        }
    }
}

impl<D: BlockDevice + Send + Sync> FileSystem for EternalFs<D> {
    fn root_inode(&self) -> InodeId {
        self.superblock.root_inode
    }

    fn lookup(&self, parent: InodeId, name: &str) -> Result<InodeId, VfsError> {
        if name == "." {
            return Ok(parent);
        }

        let parent_inode = self.read_inode(parent)?;

        if parent_inode.node_type != INODE_TYPE_DIR {
            return Err(VfsError::NotADirectory);
        }

        let num_entries = (parent_inode.file_size / (ETERNALFS_DIRENT_SIZE as u64)) as usize;
        let mut dirent_buf = [0u8; 1024];
        let mut curr_block_idx = usize::MAX;

        for i in 0..num_entries {
            let block_idx = (i * ETERNALFS_DIRENT_SIZE) / 1024;
            let off = (i * ETERNALFS_DIRENT_SIZE) % 1024;

            if block_idx != curr_block_idx {
                let phys_block = match self.resolve_block_pointer(&parent_inode, block_idx)? {
                    Some(b) => b,
                    None => break,
                };
                self.read_block(phys_block, &mut dirent_buf)?;
                curr_block_idx = block_idx;
            }

            let entry_slice = &dirent_buf[off..off + ETERNALFS_DIRENT_SIZE];
            let inode_num = u32::from_le_bytes(entry_slice[0..4].try_into().unwrap());
            if inode_num == 0 {
                continue;
            }

            let name_len = entry_slice[5] as usize;
            if name_len > 26 {
                continue;
            }

            if let Ok(entry_name) = core::str::from_utf8(&entry_slice[6..6 + name_len]) {
                if entry_name == name {
                    return Ok(inode_num as u64);
                }
            }
        }

        Err(VfsError::NotFound)
    }

    fn get_metadata(&self, inode: InodeId) -> Result<Metadata, VfsError> {
        let in_data = self.read_inode(inode)?;

        let node_type = match in_data.node_type {
            INODE_TYPE_FILE => NodeType::RegularFile,
            INODE_TYPE_DIR => NodeType::Directory,
            _ => return Err(VfsError::InvalidArgument),
        };

        Ok(Metadata {
            inode,
            node_type,
            size: in_data.file_size,
            read_only: true, // Read-Only filesystem
        })
    }

    fn create_file(&mut self, _parent: InodeId, _name: &str) -> Result<InodeId, VfsError> {
        Err(VfsError::PermissionDenied)
    }

    fn create_dir(&mut self, _parent: InodeId, _name: &str) -> Result<InodeId, VfsError> {
        Err(VfsError::PermissionDenied)
    }

    fn read(&self, inode: InodeId, offset: u64, buf: &mut [u8]) -> Result<usize, VfsError> {
        let in_data = self.read_inode(inode)?;

        if in_data.node_type != INODE_TYPE_FILE {
            return Err(VfsError::IsADirectory);
        }

        if offset >= in_data.file_size {
            return Ok(0); // EOF
        }

        let available = in_data.file_size - offset;
        let to_read = (buf.len() as u64).min(available) as usize;

        let mut read_so_far = 0usize;
        let mut curr_offset = offset;
        let mut block_buf = [0u8; 1024];

        while read_so_far < to_read {
            let block_idx = (curr_offset / 1024) as usize;
            let block_off = (curr_offset % 1024) as usize;
            let chunk = (1024 - block_off).min(to_read - read_so_far);

            match self.resolve_block_pointer(&in_data, block_idx)? {
                Some(phys_block) => {
                    self.read_block(phys_block, &mut block_buf)?;
                    buf[read_so_far..read_so_far + chunk]
                        .copy_from_slice(&block_buf[block_off..block_off + chunk]);
                }
                None => {
                    // Sparse hole / unallocated block -> Fill with 0
                    buf[read_so_far..read_so_far + chunk].fill(0);
                }
            }

            read_so_far += chunk;
            curr_offset += chunk as u64;
        }

        Ok(read_so_far)
    }

    fn write(&mut self, _inode: InodeId, _offset: u64, _buf: &[u8]) -> Result<usize, VfsError> {
        Err(VfsError::PermissionDenied)
    }

    fn truncate(&mut self, _inode: InodeId, _new_size: u64) -> Result<(), VfsError> {
        Err(VfsError::PermissionDenied)
    }

    fn readdir(&self, inode: InodeId, offset: usize, out: &mut [DirEntry]) -> Result<usize, VfsError> {
        let in_data = self.read_inode(inode)?;

        if in_data.node_type != INODE_TYPE_DIR {
            return Err(VfsError::NotADirectory);
        }

        let num_entries = (in_data.file_size / (ETERNALFS_DIRENT_SIZE as u64)) as usize;
        if offset >= num_entries || out.is_empty() {
            return Ok(0);
        }

        let mut copied = 0usize;
        let mut skipped = 0usize;
        let mut dirent_buf = [0u8; 1024];
        let mut curr_block_idx = usize::MAX;

        for i in 0..num_entries {
            let block_idx = (i * ETERNALFS_DIRENT_SIZE) / 1024;
            let off = (i * ETERNALFS_DIRENT_SIZE) % 1024;

            if block_idx != curr_block_idx {
                let phys_block = match self.resolve_block_pointer(&in_data, block_idx)? {
                    Some(b) => b,
                    None => break,
                };
                self.read_block(phys_block, &mut dirent_buf)?;
                curr_block_idx = block_idx;
            }

            let entry_slice = &dirent_buf[off..off + ETERNALFS_DIRENT_SIZE];
            let inode_num = u32::from_le_bytes(entry_slice[0..4].try_into().unwrap());
            if inode_num == 0 {
                continue;
            }

            if skipped < offset {
                skipped += 1;
                continue;
            }

            if copied >= out.len() {
                break;
            }

            let entry_type = match entry_slice[4] {
                1 => NodeType::RegularFile,
                2 => NodeType::Directory,
                _ => NodeType::RegularFile,
            };

            let name_len = (entry_slice[5] as usize).min(26);
            let mut entry = DirEntry::empty();
            entry.inode = inode_num as u64;
            entry.node_type = entry_type;
            entry.name_len = name_len;
            entry.name[..name_len].copy_from_slice(&entry_slice[6..6 + name_len]);

            out[copied] = entry;
            copied += 1;
        }

        Ok(copied)
    }
}
