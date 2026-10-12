use crate::block::{BlockDevice, BlockDeviceInfo, BlockError, validate_request};

/// Zero-size type implementing `BlockDevice` that routes to global VirtIO block operations.
#[derive(Clone, Copy)]
pub struct VirtioBlockGlobal;

impl BlockDevice for VirtioBlockGlobal {
    fn info(&self) -> BlockDeviceInfo {
        crate::virtio_blk::with_block_device(|dev| dev.info())
            .unwrap_or_else(|| BlockDeviceInfo::new("virtio_blk", 0, 512, true))
    }

    fn read_sectors(&mut self, start_sector: u64, sector_count: u32, buf: &mut [u8]) -> Result<(), BlockError> {
        validate_request(start_sector, sector_count, buf.len(), self.info().total_sectors, self.info().sector_size)?;
        crate::virtio_blk::read_sectors_global(start_sector, sector_count, buf)
    }

    fn write_sectors(&mut self, start_sector: u64, sector_count: u32, buf: &[u8]) -> Result<(), BlockError> {
        validate_request(start_sector, sector_count, buf.len(), self.info().total_sectors, self.info().sector_size)?;
        crate::virtio_blk::write_sectors_global(start_sector, sector_count, buf)
    }
}
