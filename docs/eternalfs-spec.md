# EternalFS Specification (v1.0)

## 1. Overview & Principles

EternalFS is a compact, robust, allocation-conscious on-disk filesystem designed specifically for the `no_std` x86-64 EternalOS operating system kernel.

### Design Principles
- **Strict Byte-Level Decoding:** No native C/Rust struct direct memory casting. All fields are explicitly decoded with checked slice offsets and documented little-endian byte ordering.
- **Defensive Boundary Validation:** Every block number, inode index, and byte offset is checked against filesystem size and partition boundaries before issuing reads.
- **Fixed-Size Metadata:** Fixed 64-byte inode records and fixed 32-byte directory entries for predictable arithmetic and bounds checking.
- **Bounded Storage Footprint:** Sized for 512-byte logical sectors and 1024-byte filesystem blocks.

---

## 2. Geometry & Disk Layout

An EternalFS volume is composed of contiguous 1024-byte logical blocks (2 x 512-byte sectors per block):

```text
+-----------------------+-----------------------+-----------------------+-----------------------+
| Block 0               | Block 1               | Blocks 2..N           | Blocks N+1..Total     |
| Boot Record / Reserved| Superblock (1024 B)   | Inode Table (64B/node)| Data Blocks (1024 B)  |
+-----------------------+-----------------------+-----------------------+-----------------------+
```

- **Block 0 (Offset 0..1023):** Reserved for boot records / partition table headers.
- **Block 1 (Offset 1024..2047):** Superblock (contains magic, version, geometry, and region pointers).
- **Block 2..`data_block_start - 1`:** Inode Table (containing `total_inodes` records of 64 bytes each).
- **Block `data_block_start`..`total_blocks - 1`:** Data block region.

---

## 3. Superblock Layout (Block 1, 1024 bytes)

Located at filesystem byte offset `1024` (LBA 2).

| Offset (Bytes) | Field Name | Width | Type | Description |
|----------------|------------|-------|------|-------------|
| `0x00..0x08`   | `magic`    | 8 B   | ASCII| Must be ASCII `ETERNAL1` (`[0x45, 0x54, 0x45, 0x52, 0x4E, 0x41, 0x4C, 0x31]`) |
| `0x08..0x0C`   | `version`  | 4 B   | u32-le | Format version (currently `1`) |
| `0x0C..0x10`   | `block_size` | 4 B | u32-le | Filesystem block size in bytes (must be `1024`) |
| `0x10..0x18`   | `total_blocks` | 8 B | u64-le | Total count of 1024-byte blocks in this volume |
| `0x18..0x20`   | `total_inodes` | 8 B | u64-le | Total count of 64-byte inode records |
| `0x20..0x28`   | `inode_table_block` | 8 B | u64-le | Block number where Inode Table starts (typically `2`) |
| `0x28..0x30`   | `data_block_start` | 8 B | u64-le | Block number where data blocks start |
| `0x30..0x38`   | `root_inode` | 8 B | u64-le | Root directory inode ID (typically `1`) |
| `0x38..0x3C`   | `flags`    | 4 B   | u32-le | Feature flags (bit 0 = ReadOnly) |
| `0x3C..0x40`   | `checksum` | 4 B   | u32-le | IEEE 802.3 CRC32 of bytes `0x00..0x3C` |
| `0x40..0x400`  | `reserved` | 960 B | u8[960]| Zero-padded |

---

## 4. Inode Layout (64 bytes per Inode)

Inodes are 1-indexed (Inode 1 is Root). Inode index `i` is stored at byte offset:
$$\text{offset} = (\text{inode\_table\_block} \times \text{block\_size}) + ((i - 1) \times 64)$$

| Offset (Bytes) | Field Name | Width | Type | Description |
|----------------|------------|-------|------|-------------|
| `0x00..0x02`   | `node_type`| 2 B   | u16-le | Node type: `1` = Regular File, `2` = Directory |
| `0x02..0x04`   | `flags`    | 2 B   | u16-le | Permissions / attributes (`0` = Default) |
| `0x04..0x08`   | `reserved` | 4 B   | u32-le | Reserved (0) |
| `0x08..0x10`   | `file_size`| 8 B   | u64-le | File size in bytes (for directories, equals `entry_count * 32`) |
| `0x10..0x38`   | `direct_blocks` | 40 B | u32-le[10] | 10 Direct block pointers (`0` = unallocated block) |
| `0x38..0x3C`   | `single_indirect` | 4 B | u32-le | Single indirect block pointer (`0` = none) |
| `0x3C..0x40`   | `checksum` | 4 B   | u32-le | IEEE 802.3 CRC32 of bytes `0x00..0x3C` |

- **Capacity per Inode with Direct Blocks:** $10 \times 1024 = 10,240$ bytes (~10 KiB).
- **With Single Indirect Block:** $+ (256 \times 1024) = 262,144$ bytes (256 KiB).

---

## 5. Directory Entry Layout (32 bytes per Entry)

A directory is stored as standard data blocks containing contiguous 32-byte records:

| Offset (Bytes) | Field Name | Width | Type | Description |
|----------------|------------|-------|------|-------------|
| `0x00..0x04`   | `inode`    | 4 B   | u32-le | Referenced Inode ID (`0` = empty/deleted entry) |
| `0x04..0x05`   | `entry_type` | 1 B | u8 | Object type: `1` = Regular File, `2` = Directory |
| `0x05..0x06`   | `name_len` | 1 B   | u8 | Length of filename in bytes (1..26) |
| `0x06..0x20`   | `name`     | 26 B  | ASCII/UTF-8 | Null-padded filename string |

Directory blocks cannot contain `/` or null bytes within filenames.

---

## 6. Defensive Invariants & Safety

1. **Volume Boundaries:** $\text{total\_blocks} \times \text{block\_size} \le \text{partition\_sector\_count} \times 512$.
2. **Block Pointers:** Every direct or indirect block pointer $B$ must satisfy $\text{data\_block\_start} \le B < \text{total\_blocks}$.
3. **Inode Ranges:** Inode IDs must satisfy $1 \le \text{inode\_id} \le \text{total\_inodes}$.
4. **File Size Consistency:** Direct blocks required = $\lceil \text{file\_size} / 1024 \rceil$. Any file declaring size without sufficient direct blocks is rejected.
5. **No Mutation on Read-Only Mounts:** All `create`, `write`, `truncate`, `delete` operations return `VfsError::PermissionDenied`.
