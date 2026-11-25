//! [`Region`]: a run of memory, RAII. Two backings: host memory the allocator gave out, or a file-backed
//! mapping the OS pages to and from its drive. Either way the bytes are addressed in place: a device
//! that wraps host memory (Metal's `buffer_from_host_ptr`) reads them where they lie.

use std::alloc::Layout;
use std::path::Path;
use std::ptr::NonNull;

use continuum_host::mapped_region::MappedRegion;

use crate::TierKind;

/// Why a region could not be given out.
#[derive(Debug, thiserror::Error)]
pub enum RegionRefusal {
    #[error("a region of zero bytes")]
    Empty,
    #[error("no layout for {bytes} bytes aligned to {align}")]
    Layout { bytes: usize, align: usize },
    #[error("the allocator refused {bytes} bytes")]
    OutOfMemory { bytes: usize },
    #[error("the mapping was refused: {0}")]
    Mapping(#[from] std::io::Error),
}

/// Where a region's bytes are.
pub enum Backing {
    /// Host memory from the allocator (unified or pageable).
    Host { base: NonNull<u8>, layout: Layout },
    /// A file mapping: its pages are the file's, and the OS moves them between memory and the drive.
    Mapped(MappedRegion),
}

/// A run of memory and the tier it is in. Dropping it returns the memory.
pub struct Region {
    tier: TierKind,
    backing: Backing,
}

// SAFETY: a region owns plain memory with no thread-bound state; access is ordered by `&`/`&mut`.
unsafe impl Send for Region {}
unsafe impl Sync for Region {}

impl Region {
    /// `bytes` of host memory aligned to `align`, zeroed, in `tier` (`Unified` or `Ram`).
    pub fn host(tier: TierKind, bytes: usize, align: usize) -> Result<Self, RegionRefusal> {
        if bytes == 0 {
            return Err(RegionRefusal::Empty);
        }
        let layout = Layout::from_size_align(bytes, align).map_err(|_| RegionRefusal::Layout { bytes, align })?;
        // SAFETY: a non-zero layout; zeroed so no padding a kernel reads is garbage.
        let base = NonNull::new(unsafe { std::alloc::alloc_zeroed(layout) }).ok_or(RegionRefusal::OutOfMemory { bytes })?;
        Ok(Self { tier, backing: Backing::Host { base, layout } })
    }

    /// `bytes` of the file at `path` mapped shared and writable, resident in `tier` until demoted to
    /// `Flash`. An untouched page costs neither memory nor drive.
    pub fn mapped(tier: TierKind, path: &Path, bytes: usize) -> Result<Self, RegionRefusal> {
        if bytes == 0 {
            return Err(RegionRefusal::Empty);
        }
        Ok(Self { tier, backing: Backing::Mapped(MappedRegion::create(path, bytes)?) })
    }

    pub fn tier(&self) -> TierKind {
        self.tier
    }

    pub fn backing(&self) -> &Backing {
        &self.backing
    }

    pub(crate) fn set_tier(&mut self, tier: TierKind) {
        self.tier = tier;
    }

    pub fn len(&self) -> usize {
        match &self.backing {
            Backing::Host { layout, .. } => layout.size(),
            Backing::Mapped(map) => map.len(),
        }
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// The first byte, for a device that wraps host memory where it lies. Valid while the region lives.
    pub fn as_ptr(&self) -> NonNull<u8> {
        match &self.backing {
            Backing::Host { base, .. } => *base,
            Backing::Mapped(map) => map.as_ptr(),
        }
    }

    pub fn as_slice(&self) -> &[u8] {
        match &self.backing {
            // SAFETY: `base` owns `layout.size()` initialized (zeroed) bytes while the region lives.
            Backing::Host { base, layout } => unsafe { std::slice::from_raw_parts(base.as_ptr(), layout.size()) },
            Backing::Mapped(map) => map.as_slice(),
        }
    }

    pub fn as_mut_slice(&mut self) -> &mut [u8] {
        match &mut self.backing {
            // SAFETY: as above, and `&mut self` is the only access.
            Backing::Host { base, layout } => unsafe { std::slice::from_raw_parts_mut(base.as_ptr(), layout.size()) },
            Backing::Mapped(map) => map.as_mut_slice(),
        }
    }
}

impl Drop for Region {
    fn drop(&mut self) {
        if let Backing::Host { base, layout } = &self.backing {
            // SAFETY: allocated by `alloc_zeroed` with this layout, freed once.
            unsafe { std::alloc::dealloc(base.as_ptr(), *layout) };
        }
    }
}
