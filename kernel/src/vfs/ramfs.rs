use crate::vfs::{
    DirEntry, FileSystem, InodeId, Metadata, NodeType, VfsError, MAX_NAME_LEN,
};

pub const RAMFS_MAX_NODES: usize = 32;
pub const RAMFS_FILE_MAX_SIZE: usize = 512;
pub const RAMFS_MAX_CHILDREN: usize = 16;

#[derive(Clone, Copy)]
pub struct RamChild {
    pub inode: InodeId,
    pub name: [u8; MAX_NAME_LEN],
    pub name_len: usize,
}

#[derive(Clone, Copy)]
pub enum RamNodeKind {
    File {
        data: [u8; RAMFS_FILE_MAX_SIZE],
        size: usize,
    },
    Directory {
        children: [Option<RamChild>; RAMFS_MAX_CHILDREN],
        child_count: usize,
    },
}

#[derive(Clone, Copy)]
pub struct RamNode {
    pub in_use: bool,
    pub inode: InodeId,
    pub parent_inode: InodeId,
    pub kind: RamNodeKind,
}

impl RamNode {
    pub const fn empty() -> Self {
        Self {
            in_use: false,
            inode: 0,
            parent_inode: 0,
            kind: RamNodeKind::Directory {
                children: [None; RAMFS_MAX_CHILDREN],
                child_count: 0,
            },
        }
    }
}

/// In-Memory RamFS implementation.
/// Backed by a statically bounded pool of nodes.
pub struct RamFs {
    nodes: [RamNode; RAMFS_MAX_NODES],
    next_inode: u64,
}

impl RamFs {
    pub const fn new() -> Self {
        Self {
            nodes: [RamNode::empty(); RAMFS_MAX_NODES],
            next_inode: 1, // Root is inode 1
        }
    }

    /// Initialize the root directory of the RAM filesystem.
    pub fn init(&mut self) {
        for node in self.nodes.iter_mut() {
            *node = RamNode::empty();
        }
        self.next_inode = 1;

        // Allocate root inode 1
        self.nodes[0] = RamNode {
            in_use: true,
            inode: 1,
            parent_inode: 1, // Root's parent is root
            kind: RamNodeKind::Directory {
                children: [None; RAMFS_MAX_CHILDREN],
                child_count: 0,
            },
        };
        self.next_inode = 2;
    }

    fn find_node(&self, inode: InodeId) -> Option<&RamNode> {
        self.nodes.iter().find(|n| n.in_use && n.inode == inode)
    }

    fn find_node_mut(&mut self, inode: InodeId) -> Option<&mut RamNode> {
        self.nodes.iter_mut().find(|n| n.in_use && n.inode == inode)
    }

    fn alloc_node(&mut self, parent: InodeId, kind: RamNodeKind) -> Result<InodeId, VfsError> {
        let slot = self
            .nodes
            .iter_mut()
            .find(|n| !n.in_use)
            .ok_or(VfsError::NoSpace)?;

        let inode = self.next_inode;
        self.next_inode += 1;

        *slot = RamNode {
            in_use: true,
            inode,
            parent_inode: parent,
            kind,
        };

        Ok(inode)
    }
}

impl FileSystem for RamFs {
    fn root_inode(&self) -> InodeId {
        1
    }

    fn lookup(&self, parent: InodeId, name: &str) -> Result<InodeId, VfsError> {
        if name == "." {
            return Ok(parent);
        }

        let p_node = self.find_node(parent).ok_or(VfsError::NotFound)?;
        if name == ".." {
            return Ok(p_node.parent_inode);
        }

        match &p_node.kind {
            RamNodeKind::Directory { children, .. } => {
                let name_bytes = name.as_bytes();
                for child_opt in children.iter().flatten() {
                    if child_opt.name_len == name_bytes.len()
                        && &child_opt.name[..child_opt.name_len] == name_bytes
                    {
                        return Ok(child_opt.inode);
                    }
                }
                Err(VfsError::NotFound)
            }
            RamNodeKind::File { .. } => Err(VfsError::NotADirectory),
        }
    }

    fn get_metadata(&self, inode: InodeId) -> Result<Metadata, VfsError> {
        let node = self.find_node(inode).ok_or(VfsError::NotFound)?;
        match &node.kind {
            RamNodeKind::File { size, .. } => Ok(Metadata {
                inode,
                node_type: NodeType::RegularFile,
                size: *size as u64,
                read_only: false,
            }),
            RamNodeKind::Directory { .. } => Ok(Metadata {
                inode,
                node_type: NodeType::Directory,
                size: 0,
                read_only: false,
            }),
        }
    }

    fn create_file(&mut self, parent: InodeId, name: &str) -> Result<InodeId, VfsError> {
        if name.is_empty() || name.len() > MAX_NAME_LEN {
            return Err(VfsError::InvalidArgument);
        }

        // Verify parent is a directory and doesn't already contain name
        if self.lookup(parent, name).is_ok() {
            return Err(VfsError::AlreadyExists);
        }

        let p_node = self.find_node(parent).ok_or(VfsError::NotFound)?;
        match &p_node.kind {
            RamNodeKind::Directory { child_count, .. } => {
                if *child_count >= RAMFS_MAX_CHILDREN {
                    return Err(VfsError::NoSpace);
                }
            }
            RamNodeKind::File { .. } => return Err(VfsError::NotADirectory),
        }

        // Allocate new file node
        let new_inode = self.alloc_node(
            parent,
            RamNodeKind::File {
                data: [0u8; RAMFS_FILE_MAX_SIZE],
                size: 0,
            },
        )?;

        // Attach child to parent directory
        let mut child_record = RamChild {
            inode: new_inode,
            name: [0u8; MAX_NAME_LEN],
            name_len: name.len(),
        };
        child_record.name[..name.len()].copy_from_slice(name.as_bytes());

        let p_mut = self.find_node_mut(parent).unwrap();
        if let RamNodeKind::Directory {
            children,
            child_count,
        } = &mut p_mut.kind
        {
            for slot in children.iter_mut() {
                if slot.is_none() {
                    *slot = Some(child_record);
                    *child_count += 1;
                    break;
                }
            }
        }

        Ok(new_inode)
    }

    fn create_dir(&mut self, parent: InodeId, name: &str) -> Result<InodeId, VfsError> {
        if name.is_empty() || name.len() > MAX_NAME_LEN {
            return Err(VfsError::InvalidArgument);
        }

        if self.lookup(parent, name).is_ok() {
            return Err(VfsError::AlreadyExists);
        }

        let p_node = self.find_node(parent).ok_or(VfsError::NotFound)?;
        match &p_node.kind {
            RamNodeKind::Directory { child_count, .. } => {
                if *child_count >= RAMFS_MAX_CHILDREN {
                    return Err(VfsError::NoSpace);
                }
            }
            RamNodeKind::File { .. } => return Err(VfsError::NotADirectory),
        }

        // Allocate new dir node
        let new_inode = self.alloc_node(
            parent,
            RamNodeKind::Directory {
                children: [None; RAMFS_MAX_CHILDREN],
                child_count: 0,
            },
        )?;

        let mut child_record = RamChild {
            inode: new_inode,
            name: [0u8; MAX_NAME_LEN],
            name_len: name.len(),
        };
        child_record.name[..name.len()].copy_from_slice(name.as_bytes());

        let p_mut = self.find_node_mut(parent).unwrap();
        if let RamNodeKind::Directory {
            children,
            child_count,
        } = &mut p_mut.kind
        {
            for slot in children.iter_mut() {
                if slot.is_none() {
                    *slot = Some(child_record);
                    *child_count += 1;
                    break;
                }
            }
        }

        Ok(new_inode)
    }

    fn read(&self, inode: InodeId, offset: u64, buf: &mut [u8]) -> Result<usize, VfsError> {
        let node = self.find_node(inode).ok_or(VfsError::NotFound)?;
        match &node.kind {
            RamNodeKind::File { data, size } => {
                let offset = offset as usize;
                if offset >= *size {
                    return Ok(0); // EOF
                }
                let available = *size - offset;
                let to_read = buf.len().min(available);
                buf[..to_read].copy_from_slice(&data[offset..offset + to_read]);
                Ok(to_read)
            }
            RamNodeKind::Directory { .. } => Err(VfsError::IsADirectory),
        }
    }

    fn write(&mut self, inode: InodeId, offset: u64, buf: &[u8]) -> Result<usize, VfsError> {
        let node = self.find_node_mut(inode).ok_or(VfsError::NotFound)?;
        match &mut node.kind {
            RamNodeKind::File { data, size } => {
                let offset = offset as usize;
                if offset > RAMFS_FILE_MAX_SIZE || offset > *size {
                    return Err(VfsError::InvalidArgument);
                }
                let remaining_capacity = RAMFS_FILE_MAX_SIZE - offset;
                let to_write = buf.len().min(remaining_capacity);
                if to_write == 0 && !buf.is_empty() {
                    return Err(VfsError::NoSpace);
                }

                data[offset..offset + to_write].copy_from_slice(&buf[..to_write]);
                let new_size = (*size).max(offset + to_write);
                *size = new_size;
                Ok(to_write)
            }
            RamNodeKind::Directory { .. } => Err(VfsError::IsADirectory),
        }
    }

    fn truncate(&mut self, inode: InodeId, new_size: u64) -> Result<(), VfsError> {
        let node = self.find_node_mut(inode).ok_or(VfsError::NotFound)?;
        match &mut node.kind {
            RamNodeKind::File { data, size } => {
                let target = new_size as usize;
                if target > RAMFS_FILE_MAX_SIZE {
                    return Err(VfsError::NoSpace);
                }
                if target < *size {
                    data[target..*size].fill(0);
                }
                *size = target;
                Ok(())
            }
            RamNodeKind::Directory { .. } => Err(VfsError::IsADirectory),
        }
    }

    fn readdir(&self, inode: InodeId, offset: usize, out: &mut [DirEntry]) -> Result<usize, VfsError> {
        let node = self.find_node(inode).ok_or(VfsError::NotFound)?;
        match &node.kind {
            RamNodeKind::Directory { children, child_count } => {
                if offset >= *child_count || out.is_empty() {
                    return Ok(0);
                }

                let mut copied = 0usize;
                let mut skipped = 0usize;

                for child_opt in children.iter().flatten() {
                    if skipped < offset {
                        skipped += 1;
                        continue;
                    }
                    if copied >= out.len() {
                        break;
                    }

                    // Look up child node type
                    let child_node = self.find_node(child_opt.inode);
                    let ntype = match child_node {
                        Some(n) => match n.kind {
                            RamNodeKind::File { .. } => NodeType::RegularFile,
                            RamNodeKind::Directory { .. } => NodeType::Directory,
                        },
                        None => NodeType::RegularFile,
                    };

                    let mut entry = DirEntry::empty();
                    entry.inode = child_opt.inode;
                    entry.node_type = ntype;
                    entry.name_len = child_opt.name_len;
                    entry.name[..child_opt.name_len].copy_from_slice(&child_opt.name[..child_opt.name_len]);

                    out[copied] = entry;
                    copied += 1;
                }

                Ok(copied)
            }
            RamNodeKind::File { .. } => Err(VfsError::NotADirectory),
        }
    }

    fn unlink(&mut self, parent: InodeId, name: &str) -> Result<(), VfsError> {
        if name.is_empty() || name == "." || name == ".." {
            return Err(VfsError::InvalidArgument);
        }

        // Find child inode under parent
        let child_inode = self.lookup(parent, name)?;

        // If the target is a directory, verify it is empty
        let child_node = self.find_node(child_inode).ok_or(VfsError::NotFound)?;
        if let RamNodeKind::Directory { child_count, .. } = &child_node.kind {
            if *child_count > 0 {
                return Err(VfsError::DirectoryNotEmpty);
            }
        }

        // Deallocate the child node
        for slot in self.nodes.iter_mut() {
            if slot.in_use && slot.inode == child_inode {
                *slot = RamNode::empty();
                break;
            }
        }

        // Remove the entry from the parent directory
        let p_mut = self.find_node_mut(parent).ok_or(VfsError::NotFound)?;
        if let RamNodeKind::Directory { children, child_count } = &mut p_mut.kind {
            let name_bytes = name.as_bytes();
            for slot in children.iter_mut() {
                if let Some(child) = slot {
                    if child.name_len == name_bytes.len() && &child.name[..child.name_len] == name_bytes {
                        *slot = None;
                        *child_count = child_count.saturating_sub(1);
                        return Ok(());
                    }
                }
            }
        }

        Err(VfsError::NotFound)
    }
}

