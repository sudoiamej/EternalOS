use crate::partition::discover_partitions;
use crate::sync::SpinLock;
use crate::vfs::eternalfs::EternalFs;
use crate::vfs::virtio_adapter::VirtioBlockGlobal;
use crate::vfs::{DirEntry, FileSystem, Metadata, VfsError, MAX_PATH_LEN};

/// Represents an actively mounted filesystem.
pub struct MountedVolume {
    pub in_use: bool,
    pub device_id: [u8; 32],
    pub device_id_len: usize,
    pub mount_path: [u8; MAX_PATH_LEN],
    pub mount_path_len: usize,
    pub read_only: bool,
    pub fs: Option<EternalFs<VirtioBlockGlobal>>,
}

impl MountedVolume {
    pub const fn empty() -> Self {
        Self {
            in_use: false,
            device_id: [0; 32],
            device_id_len: 0,
            mount_path: [0; MAX_PATH_LEN],
            mount_path_len: 0,
            read_only: true,
            fs: None,
        }
    }

    pub fn mount_path_str(&self) -> &str {
        core::str::from_utf8(&self.mount_path[..self.mount_path_len]).unwrap_or("")
    }

    pub fn device_id_str(&self) -> &str {
        core::str::from_utf8(&self.device_id[..self.device_id_len]).unwrap_or("")
    }
}

pub const MAX_VOLUME_MOUNTS: usize = 4;
static VOLUME_MOUNTS: SpinLock<[MountedVolume; MAX_VOLUME_MOUNTS]> = SpinLock::new([
    MountedVolume::empty(),
    MountedVolume::empty(),
    MountedVolume::empty(),
    MountedVolume::empty(),
]);

/// Automatically detects partitions and mounts supported filesystems (e.g. VirtIO disk partition 0 to /disk).
pub fn auto_mount_storage() {
    let mut dev = VirtioBlockGlobal;
    let partitions = match discover_partitions(&mut dev) {
        Ok(t) => t,
        Err(_) => return,
    };

    if partitions.count == 0 {
        return;
    }

    // Try mounting partition 0
    let p0 = &partitions.partitions[0];
    if let Ok(efs) = EternalFs::mount(VirtioBlockGlobal, p0.start_lba, p0.sector_count) {
        mount_internal("/disk", "virtio0p1", true, efs);
    }
}

fn mount_internal(path: &str, dev_id: &str, read_only: bool, efs: EternalFs<VirtioBlockGlobal>) {
    let mut mounts = VOLUME_MOUNTS.lock();
    for slot in mounts.iter_mut() {
        if !slot.in_use {
            slot.in_use = true;
            slot.read_only = read_only;
            let p_bytes = path.as_bytes();
            slot.mount_path_len = p_bytes.len().min(MAX_PATH_LEN);
            slot.mount_path[..slot.mount_path_len].copy_from_slice(&p_bytes[..slot.mount_path_len]);

            let d_bytes = dev_id.as_bytes();
            slot.device_id_len = d_bytes.len().min(32);
            slot.device_id[..slot.device_id_len].copy_from_slice(&d_bytes[..slot.device_id_len]);

            slot.fs = Some(efs);
            crate::log_info!("[VFS] Mounted device '{}' at '{}' (read-only: {})", dev_id, path, read_only);
            break;
        }
    }
}

/// Find a mounted volume matching the path prefix.
/// Returns (volume_index, relative_subpath).
pub fn find_mount<'a>(path: &'a str) -> Option<(usize, &'a str)> {
    let mounts = VOLUME_MOUNTS.lock();
    for (idx, slot) in mounts.iter().enumerate() {
        if !slot.in_use {
            continue;
        }
        let mnt_str = slot.mount_path_str();
        if path == mnt_str {
            return Some((idx, "/"));
        }
        if path.starts_with(mnt_str) {
            let rest = &path[mnt_str.len()..];
            if rest.starts_with('/') {
                return Some((idx, rest));
            }
        }
    }
    None
}

/// Forward lookup in mounted volume
pub fn volume_lookup(vol_idx: usize, subpath: &str) -> Result<u64, VfsError> {
    let mounts = VOLUME_MOUNTS.lock();
    let vol = mounts.get(vol_idx).ok_or(VfsError::NotFound)?;
    let fs = vol.fs.as_ref().ok_or(VfsError::NotFound)?;
    if subpath == "/" || subpath.is_empty() {
        return Ok(fs.root_inode());
    }

    let mut curr = fs.root_inode();
    for part in subpath.split('/').filter(|s| !s.is_empty()) {
        curr = fs.lookup(curr, part)?;
    }
    Ok(curr)
}

/// Forward metadata query
pub fn volume_stat(vol_idx: usize, subpath: &str) -> Result<Metadata, VfsError> {
    let inode = volume_lookup(vol_idx, subpath)?;
    let mounts = VOLUME_MOUNTS.lock();
    let vol = mounts.get(vol_idx).ok_or(VfsError::NotFound)?;
    let fs = vol.fs.as_ref().ok_or(VfsError::NotFound)?;
    let mut meta = fs.get_metadata(inode)?;
    meta.read_only = vol.read_only;
    Ok(meta)
}

/// Forward readdir query
pub fn volume_readdir(vol_idx: usize, subpath: &str, offset: usize, out: &mut [DirEntry]) -> Result<usize, VfsError> {
    let inode = volume_lookup(vol_idx, subpath)?;
    let mounts = VOLUME_MOUNTS.lock();
    let vol = mounts.get(vol_idx).ok_or(VfsError::NotFound)?;
    let fs = vol.fs.as_ref().ok_or(VfsError::NotFound)?;
    fs.readdir(inode, offset, out)
}

/// Forward read query
pub fn volume_read(vol_idx: usize, inode: u64, offset: u64, buf: &mut [u8]) -> Result<usize, VfsError> {
    let mounts = VOLUME_MOUNTS.lock();
    let vol = mounts.get(vol_idx).ok_or(VfsError::NotFound)?;
    let fs = vol.fs.as_ref().ok_or(VfsError::NotFound)?;
    fs.read(inode, offset, buf)
}

/// Iterate through mounted volumes
pub fn for_each_mount<F: FnMut(&MountedVolume)>(mut f: F) {
    let mounts = VOLUME_MOUNTS.lock();
    for slot in mounts.iter() {
        if slot.in_use {
            f(slot);
        }
    }
}

/// Unmount a volume by mount path
pub fn unmount_volume(path: &str) -> Result<(), VfsError> {
    let mut mounts = VOLUME_MOUNTS.lock();
    for slot in mounts.iter_mut() {
        if slot.in_use && slot.mount_path_str() == path {
            *slot = MountedVolume::empty();
            crate::log_info!("[VFS] Unmounted volume at '{}'", path);
            return Ok(());
        }
    }
    Err(VfsError::NotFound)
}
