//! Where a mind's KV storage lives (UNIFIED-PAGING.md, slice 2). On a node whose device views host memory,
//! each mind's KV is a block of the block layer over her own mapped file: the stream views it when she is
//! bound, the OS pages it when she is parked, and no byte is copied. Elsewhere it is the fork's own memory,
//! moved by row copy until the discrete adapter (slice 4).

use std::io;
use std::path::{Path, PathBuf};
use std::ptr::NonNull;

use continuum_blocks::{Backing, Block, Region, TierKind};
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
    /// Her mapped file. `owned` while it is this core's (in its instance directory: it goes with her); not while a
    /// restore verifies a file still at the path a manifest names, nor once a stop has handed it over (either way
    /// it is the next start's, and is never removed here).
    Mapped { block: Option<Block<MindId>>, path: PathBuf, owned: bool },
}

impl KvMemory {
    /// Whether she has storage of her own the stream can view.
    pub(super) fn held(&self) -> bool {
        matches!(self, Self::Mapped { .. } | Self::Device)
    }

    /// The bytes her mapping takes for `bytes` of storage: whole pages.
    fn mapped_len(bytes: usize) -> usize {
        bytes.div_ceil(MAPPING_GRANULE) * MAPPING_GRANULE
    }

    /// A block over a new file at `path`, `bytes` rounded to whole pages, for `mind`; this core's.
    pub(super) fn mapped(mind: MindId, path: PathBuf, bytes: usize) -> io::Result<(Self, NonNull<u8>, usize)> {
        let len = Self::mapped_len(bytes);
        let region = Region::mapped(TierKind::Unified, &path, len).map_err(|refused| io::Error::other(refused.to_string()))?;
        let at = region.as_ptr();
        Ok((Self::Mapped { block: Some(Block::new(mind, region)), path, owned: true }, at, len))
    }

    /// A block over the file a stop handed over at `path`, mapped where it is and not yet this core's: its
    /// length must be exactly what `bytes` of storage maps (a file of another layout or a torn one is refused
    /// before it is mapped). No byte is read until a page is touched.
    pub(super) fn handed(mind: MindId, path: PathBuf, bytes: usize) -> io::Result<(Self, NonNull<u8>, usize)> {
        let len = Self::mapped_len(bytes);
        let found = std::fs::metadata(&path)?.len();
        if found != len as u64 {
            return Err(io::Error::new(io::ErrorKind::InvalidData, format!("{} holds {found} bytes, her storage maps {len}", path.display())));
        }
        let region = Region::mapped(TierKind::Unified, &path, len).map_err(|refused| io::Error::other(refused.to_string()))?;
        let at = region.as_ptr();
        Ok((Self::Mapped { block: Some(Block::new(mind, region)), path, owned: false }, at, len))
    }

    /// Take a verified handed-over file into this core's directory at `to`: one durable rename, before
    /// any byte of it can change. From here a crash leaves the manifest's path empty, so she starts cold.
    pub(super) fn adopt(&mut self, to: &Path) -> io::Result<()> {
        let Self::Mapped { path, owned, .. } = self else { return Ok(()) };
        continuum_host::durable::replace(path, to)?;
        *path = to.to_path_buf();
        *owned = true;
        Ok(())
    }

    /// Hand her file to the next start at `to`: every dirty page written and the file synced, the
    /// mapping dropped (Windows refuses to rename a file mapped here), then one durable rename. The
    /// caller has released the fork's view of it. `false` when her storage is no file of hers.
    pub(super) fn hand_over(&mut self, to: &Path) -> io::Result<bool> {
        let Self::Mapped { block, path, owned } = self else { return Ok(false) };
        if let Some(held) = block.take() {
            if let Backing::Mapped(map) = held.region().backing() {
                map.flush()?;
            }
        }
        continuum_host::durable::fence_path(path)?;
        continuum_host::durable::replace(path, to)?;
        *path = to.to_path_buf();
        *owned = false;
        Ok(true)
    }
}

impl Drop for KvMemory {
    // A file of this core's holds only this run's KV, so it goes with her; a handed-over one is the next
    // start's and stays. The mapping is released first: Windows refuses to delete a mapped file.
    fn drop(&mut self) {
        if let Self::Mapped { block, path, owned } = self {
            drop(block.take());
            if !*owned {
                return;
            }
            if let Err(why) = std::fs::remove_file(&*path) {
                continuum_base::probe!(class = "persona.kv_home.not_removed", path = %path.display(), why = %why, "a mind's KV file was not removed");
            }
        }
    }
}
