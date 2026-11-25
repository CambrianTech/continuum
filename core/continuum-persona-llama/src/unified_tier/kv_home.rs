//! Where a mind's KV storage lives (UNIFIED-PAGING.md, slice 2). On a node whose device views host memory,
//! each mind's KV is a block of the block layer over her own mapped file: the stream views it when she is
//! bound, the OS pages it when she is parked, and no byte is copied. Elsewhere it is the fork's own memory,
//! moved by row copy until the discrete adapter (slice 4).

use std::path::PathBuf;

use continuum_blocks::{Block, Region, TierKind};
use continuum_persona_engine::persona::MindId;

/// Whole pages of every platform the block is mapped on: 16 KiB on Apple silicon, 64 KiB the Windows
/// allocation granularity.
const MAPPING_GRANULE: usize = 64 * 1024;

/// Where the tier keeps each mind's KV storage, chosen by the node.
#[derive(Debug)]
pub enum KvHome {
    /// Each mind's KV is a block over her own file in this instance's directory, under its lease (other copies
    /// of us on the node never map there). Whether the device can view it is
    /// the fork's to say (`storage_bytes` is 0 when it cannot); the tier asks once and keeps the answer.
    Mapped(continuum_blocks::Instance),
    /// The fork's own memory, moved by row copy (a context that is not the Core's shape; examples).
    Native,
    /// In the fork's own device memory (VRAM), viewed like a mapped block: a device that cannot wrap host memory.
    Device,
}

/// A mind's KV memory, held beside her fork blocks and dropped after them.
pub(super) enum KvMemory {
    Native,
    /// In the fork's own device memory (VRAM), viewed like a mapped block: a device that cannot wrap host memory.
    Device,
    Mapped { block: Option<Block<MindId>>, path: PathBuf },
}

impl KvMemory {
    /// Whether she has storage of her own the stream can view.
    pub(super) fn held(&self) -> bool {
        matches!(self, Self::Mapped { .. } | Self::Device)
    }

    /// A block over `path`, `bytes` rounded to whole pages, for `mind`.
    pub(super) fn mapped(mind: MindId, path: PathBuf, bytes: usize) -> std::io::Result<(Self, std::ptr::NonNull<u8>, usize)> {
        let len = bytes.div_ceil(MAPPING_GRANULE) * MAPPING_GRANULE;
        let region = Region::mapped(TierKind::Unified, &path, len).map_err(|refused| std::io::Error::other(refused.to_string()))?;
        let at = region.as_ptr();
        Ok((Self::Mapped { block: Some(Block::new(mind, region)), path }, at, len))
    }
}

impl Drop for KvMemory {
    // Her file holds only this run's KV, so it goes with her (a preserved mind's restore is her written
    // blocks, until her file is her restore: the next step of slice 2). The mapping is released first:
    // Windows refuses to delete a mapped file.
    fn drop(&mut self) {
        if let Self::Mapped { block, path } = self {
            drop(block.take());
            if let Err(why) = std::fs::remove_file(&*path) {
                continuum_base::probe!(class = "persona.kv_home.not_removed", path = %path.display(), why = %why, "a mind's KV file was not removed");
            }
        }
    }
}
