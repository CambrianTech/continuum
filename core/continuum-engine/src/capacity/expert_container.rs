//! Expert container reader — the #269 side of the #268 seam.
//!
//! BigMama's foundry (#268) re-packs a MoE's routed experts into a streaming
//! container (docs/reference/WASTE-EXTRACT.md, mined from sqliteai/waste):
//! a directory of `manifest.json` + one `experts-L{n}.bin` bank per layer,
//! each bank a run of fixed-size 4 KiB-aligned records sorted by expert id.
//! The bank file is THE grid shard unit — a peer that holds `experts-L07.bin`
//! can serve layer 7 to the mesh without holding anything else.
//!
//! This module is the READ side only: open a container, verify its geometry
//! loudly, and fetch one expert's whole record in one positioned read. The
//! write side (converter/re-packer) is the foundry's; the cache that decides
//! WHICH records stay resident is [`super::expert_ecache`]. Keeping the three
//! separate is what lets the same reader serve a local NVMe bank and a
//! remote peer's shard behind one fetch seam.
//!
//! Loudness contract: geometry violations (record not a page multiple, bank
//! size not a record multiple, header identity mismatch) are ERRORS, never
//! best-effort reads. WASTE's `bank_open` verifies alignment because O_DIRECT
//! turns a misaligned record into `EINVAL` on every read — we verify for the
//! same reason plus one more: a silently-wrong offset table is exactly the
//! shape of bug that produced the three-day reuse=0 mystery.

use std::fs::File;
use std::io::Read;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use super::expert_ecache::ExpertKey;

/// Record alignment the format guarantees (WASTE FORMAT.md). O_DIRECT needs
/// 512 or 4096; whole-4KiB records satisfy both block sizes by construction.
pub const RECORD_ALIGN: u64 = 4096;

/// Per-record header magic: ASCII `WEXP`, little-endian on disk.
pub const RECORD_MAGIC: u32 = u32::from_le_bytes(*b"WEXP");

/// Bytes of `ExpertRec` header we verify on fetch: magic(4) + layer(2) +
/// expert_id(2). The rest of the header (fmt, offsets, block count) belongs
/// to the decode stage, not the fetch seam.
const HEADER_IDENT_BYTES: usize = 8;

/// One precision tier in a v2 container (all-star/cruft allocation,
/// 2026-07-31 seam sync with the foundry): descending fidelity, `id` equal
/// to its index in `tiers`. Each tier has its own record size (and therefore
/// its own bank files) — the PAGER chooses which tier to fetch per expert;
/// the container just ships them all. The manifest stays policy-free.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct TierSpec {
    /// Tier id == index in `tiers`, 0 = sharpest.
    pub id: u16,
    /// Quant label for this tier (e.g. "VQ3R", "IQ2", "IQ1"). Decode-stage
    /// concern; the fetch seam carries it through.
    pub quant: String,
    /// Fixed record size at this tier. MUST be a multiple of
    /// [`RECORD_ALIGN`] — the alignment law holds per tier independently.
    pub record_bytes: u64,
}

/// The native foundry emits `MoecQuant` integers, while older Rust producers
/// emit quant labels. Preserve the original representation in provenance and
/// round trips; interpreting a tag must not rewrite the producer's manifest.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(untagged)]
pub enum ContainerFormat {
    Label(String),
    Native(u32),
}

impl From<&str> for ContainerFormat {
    fn from(label: &str) -> Self {
        Self::Label(label.to_owned())
    }
}

impl ContainerFormat {
    fn quant_label(&self) -> String {
        match self {
            Self::Label(label) => label.clone(),
            // ggml-moe-container.h::MoecQuant is the producer authority.
            Self::Native(1) => "RVQ3".to_owned(),
            Self::Native(2) => "IQ2".to_owned(),
            Self::Native(3) => "MXFP4".to_owned(),
            Self::Native(tag) => format!("native:{tag}"),
        }
    }
}

/// A whole-container storage observation. This is geometry evidence, not a
/// producer-completion certificate or permission to retire the source. Migration
/// must additionally retain reader custody and verify every copied byte.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CompleteContainer {
    pub root: PathBuf,
    pub manifest: ContainerManifest,
    /// Original producer bytes, including unknown metadata. Placement receipts
    /// can fingerprint these; they must never synthesize a replacement manifest.
    pub manifest_bytes: Vec<u8>,
    pub bank_bytes: u64,
    /// All original flat entries, including auxiliary producer evidence. A
    /// migration must preserve these too, not just regenerate a manifest.
    pub files: Vec<ContainerFile>,
    pub total_bytes: u64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ContainerFile {
    pub name: std::ffi::OsString,
    pub bytes: u64,
}

/// `manifest.json` at the container root — the seam contract with the
/// foundry (#268). The foundry WRITES this; nothing on the serving side ever
/// hand-authors one. Versioned so the format can grow without silent
/// misreads.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ContainerManifest {
    /// Manifest schema version. Readers refuse versions they don't know.
    pub version: u32,
    /// Model this container was packed from (e.g. "kimi-k3"). Identity for
    /// grid advertisement; never parsed for behavior.
    pub model: String,
    /// Record encoding (e.g. "VQ3R", "VQ2R"). Decode-stage concern; the
    /// fetch seam only carries it through.
    pub fmt: ContainerFormat,
    /// Fixed size of every expert record in every bank, in bytes. MUST be a
    /// multiple of [`RECORD_ALIGN`].
    pub record_bytes: u64,
    /// Number of MoE layers packed (one `experts-L{n}.bin` each, 0-indexed).
    pub n_layers: u16,
    /// Routed experts per layer (records per bank).
    pub experts_per_layer: u16,
    /// TOTAL expert records activated per decoded token ACROSS ALL MoE
    /// layers (`top_k_per_layer × n_moe_layers` for a uniform arch — K3:
    /// 8 × 61 ≈ 488, NOT 8). This is the input to the cliff arithmetic in
    /// [`super::expert_ecache::EcacheBudget::one_token_working_set`]:
    /// a cache below `activated_per_token × record_bytes` has structurally
    /// ZERO hit rate. Pinned v1 semantics (2026-07-30 seam sync with the
    /// foundry): total, never per-layer — a per-layer value here would
    /// under-state the cliff ~60× and re-open the reuse=0 trap.
    pub activated_per_token: u32,
    /// Per-layer router top-k (arch fact, e.g. 8 for K3). Self-documenting
    /// companion to `activated_per_token` so the total is auditable
    /// (`top_k_per_layer × MoE layer count == activated_per_token`).
    /// Optional in v1: absent in containers packed before the field existed.
    #[serde(default)]
    pub top_k_per_layer: Option<u32>,
    /// Precision tiers, descending fidelity (v2). Empty/absent = the v1
    /// degenerate case: exactly one tier whose `record_bytes` is the
    /// top-level field and whose banks use the v1 `experts-L{n}.bin` names.
    /// v2 containers (version ≥ 2) name banks `experts-L{n}-T{t}.bin`.
    #[serde(default)]
    pub tiers: Vec<TierSpec>,
}

impl ContainerManifest {
    /// The effective tier table: declared tiers for v2, or the synthesized
    /// single-tier view of a v1 manifest — ONE code path downstream, the v1
    /// case is just the degenerate table (seam sync 2026-07-31).
    pub fn effective_tiers(&self) -> Vec<TierSpec> {
        if self.tiers.is_empty() {
            vec![TierSpec {
                id: 0,
                quant: self.fmt.quant_label(),
                record_bytes: self.record_bytes,
            }]
        } else {
            self.tiers.clone()
        }
    }
}

/// Everything that can go wrong opening or reading a container. Every
/// variant names the file and the numbers — a geometry error you can't act
/// on from the message alone is a bug in the error, not just the container.
#[derive(Debug, thiserror::Error)]
pub enum ContainerError {
    #[error("container {path}: invalid whole-container geometry: {reason}")]
    WholeGeometry { path: PathBuf, reason: String },
    #[error("container manifest {path}: {source}")]
    ManifestIo {
        path: PathBuf,
        source: std::io::Error,
    },
    #[error("container manifest {path}: invalid JSON: {source}")]
    ManifestParse {
        path: PathBuf,
        source: serde_json::Error,
    },
    #[error("container manifest {path}: unknown version {found} (reader knows ≤ {known})")]
    ManifestVersion {
        path: PathBuf,
        found: u32,
        known: u32,
    },
    #[error(
        "container manifest {path}: record_bytes {record_bytes} is not a multiple of \
         {align} — misaligned records make every O_DIRECT read fail EINVAL"
    )]
    RecordMisaligned {
        path: PathBuf,
        record_bytes: u64,
        align: u64,
    },
    #[error("expert bank {path}: {source}")]
    BankIo {
        path: PathBuf,
        source: std::io::Error,
    },
    #[error(
        "expert bank {path}: size {file_bytes} is not records×{record_bytes} for \
         {experts_per_layer} experts (expected {expected_bytes}) — truncated or \
         wrong-manifest bank"
    )]
    BankGeometry {
        path: PathBuf,
        file_bytes: u64,
        record_bytes: u64,
        experts_per_layer: u16,
        expected_bytes: u64,
    },
    #[error("layer {layer} out of range: container has {n_layers} layers")]
    LayerOutOfRange { layer: u16, n_layers: u16 },
    #[error("tier {tier} out of range: container has {n_tiers} tiers")]
    TierOutOfRange { tier: u16, n_tiers: u16 },
    #[error(
        "container manifest {path}: tier {index} has id {id} — tier ids must equal \
         their index (descending-fidelity order is the contract)"
    )]
    TierIdMismatch { path: PathBuf, index: u16, id: u16 },
    #[error("expert {expert} out of range: {experts_per_layer} experts per layer")]
    ExpertOutOfRange { expert: u16, experts_per_layer: u16 },
    #[error(
        "expert record identity mismatch in {path} at offset {offset}: header says \
         (magic {found_magic:#010x}, layer {found_layer}, expert {found_expert}), \
         fetch asked for (layer {want_layer}, expert {want_expert}) — corrupt bank \
         or drifted offset table"
    )]
    RecordIdentity {
        path: PathBuf,
        offset: u64,
        found_magic: u32,
        found_layer: u16,
        found_expert: u16,
        want_layer: u16,
        want_expert: u16,
    },
}

/// The fetch seam, adapterized (OpenCV-style): ONE interface the cache and
/// pager talk to, many sources behind it. Outlier A is the local NVMe
/// container below; outlier B (a remote peer's shard over the grid — same
/// key, same identity check, bytes arrive over a stream instead of a pread)
/// slots in when the layer-split lands, WITHOUT the pager learning a second
/// call shape. The blended local/remote fetcher for a split serving plan is
/// itself just another impl that routes by layer range.
pub trait ExpertFetcher {
    /// Fetch the record identified by `key` (layer + expert + tier) into
    /// `buf`, sized to the key's tier. Identity-verified; errors are loud.
    fn fetch(&mut self, key: ExpertKey, buf: &mut [u8]) -> Result<(), ContainerError>;

    /// Record size for a tier — what the caller must size `buf` (and the
    /// cache must account) with. `None` when the tier doesn't exist.
    fn record_bytes(&self, tier: u16) -> Option<u64>;
}

/// One opened `experts-L{n}.bin`: a verified run of fixed-size records
/// sorted by expert id. Offset arithmetic is the WHOLE index — contiguity +
/// sort order means `expert_id × record_bytes`, no side table to drift.
#[derive(Debug)]
pub struct ExpertBank {
    file: File,
    path: PathBuf,
    layer: u16,
    record_bytes: u64,
    experts: u16,
}

impl ExpertBank {
    /// Open + verify one bank (WASTE `bank_open`). Checks size geometry
    /// against the manifest; identity of individual records is verified per
    /// fetch (cheap: 8 bytes of an already-read buffer).
    pub fn open(
        path: &Path,
        layer: u16,
        record_bytes: u64,
        experts_per_layer: u16,
    ) -> Result<Self, ContainerError> {
        let file = File::open(path).map_err(|source| ContainerError::BankIo {
            path: path.to_path_buf(),
            source,
        })?;
        let file_bytes = file
            .metadata()
            .map_err(|source| ContainerError::BankIo {
                path: path.to_path_buf(),
                source,
            })?
            .len();
        let expected_bytes = record_bytes * experts_per_layer as u64;
        if file_bytes != expected_bytes {
            return Err(ContainerError::BankGeometry {
                path: path.to_path_buf(),
                file_bytes,
                record_bytes,
                experts_per_layer,
                expected_bytes,
            });
        }
        Ok(Self {
            file,
            path: path.to_path_buf(),
            layer,
            record_bytes,
            experts: experts_per_layer,
        })
    }

    /// Fetch one expert's whole record: ONE positioned read of
    /// `record_bytes`, then an 8-byte identity check against the header.
    pub fn fetch(&self, expert: u16, buf: &mut [u8]) -> Result<(), ContainerError> {
        if expert >= self.experts {
            return Err(ContainerError::ExpertOutOfRange {
                expert,
                experts_per_layer: self.experts,
            });
        }
        debug_assert_eq!(buf.len() as u64, self.record_bytes);
        let offset = expert as u64 * self.record_bytes;
        continuum_host::positioned::read_exact_at(&self.file, buf, offset).map_err(|source| {
            ContainerError::BankIo {
                path: self.path.clone(),
                source,
            }
        })?;
        self.verify_identity(buf, offset, expert)
    }

    fn verify_identity(&self, buf: &[u8], offset: u64, expert: u16) -> Result<(), ContainerError> {
        let Some(&[m0, m1, m2, m3, l0, l1, e0, e1]) = buf.first_chunk::<8>() else {
            // A record too short to carry its 8-byte identity header cannot match any identity.
            return Err(ContainerError::RecordIdentity {
                path: self.path.clone(),
                offset,
                found_magic: 0,
                found_layer: 0,
                found_expert: 0,
                want_layer: self.layer,
                want_expert: expert,
            });
        };
        let found_magic = u32::from_le_bytes([m0, m1, m2, m3]);
        let found_layer = u16::from_le_bytes([l0, l1]);
        let found_expert = u16::from_le_bytes([e0, e1]);
        if found_magic != RECORD_MAGIC || found_layer != self.layer || found_expert != expert {
            return Err(ContainerError::RecordIdentity {
                path: self.path.clone(),
                offset,
                found_magic,
                found_layer,
                found_expert,
                want_layer: self.layer,
                want_expert: expert,
            });
        }
        Ok(())
    }
}

/// An opened container directory: manifest + lazily-opened banks. Banks are
/// opened on first touch, not at container-open — a grid node holding two of
/// sixty shards must not fail (or stat sixty files) for the ones it serves
/// from peers instead.
#[derive(Debug)]
pub struct ExpertContainer {
    root: PathBuf,
    manifest: ContainerManifest,
    manifest_bytes: Vec<u8>,
    /// Effective tier table (synthesized single tier for v1) — validated at
    /// open so fetch-time indexing is pure arithmetic.
    tiers: Vec<TierSpec>,
    /// Lazily-opened banks, indexed `layer × n_tiers + tier`.
    banks: Vec<Option<ExpertBank>>,
    /// Declared last so bank handles are dropped before reader custody ends.
    _reader_lease: Option<File>,
}

impl ExpertContainer {
    /// Highest manifest version this reader understands. v2 adds `tiers[]`
    /// + per-(layer,tier) bank naming (`moec_pack_dir_tiered` writer side,
    /// locked 2026-07-31).
    pub const KNOWN_VERSION: u32 = 2;

    /// Observe the entire container for placement, unlike lazy shard serving.
    /// A manifest is written before banks by the native packer, so parsing it
    /// alone never establishes that an artifact is complete.
    pub fn observe_complete(root: &Path) -> Result<CompleteContainer, ContainerError> {
        let refuse = |reason: String| ContainerError::WholeGeometry {
            path: root.to_path_buf(),
            reason,
        };
        let plain_metadata = |path: &Path| -> Result<std::fs::Metadata, ContainerError> {
            let metadata = std::fs::symlink_metadata(path)
                .map_err(|error| refuse(format!("{}: {error}", path.display())))?;
            #[cfg(windows)]
            let redirected = {
                use std::os::windows::fs::MetadataExt;
                metadata.file_attributes() & 0x400 != 0
            };
            #[cfg(not(windows))]
            let redirected = metadata.file_type().is_symlink();
            if redirected {
                return Err(refuse(format!("redirected entry {}", path.display())));
            }
            Ok(metadata)
        };
        if !plain_metadata(root)?.is_dir()
            || !plain_metadata(&root.join("manifest.json"))?.is_file()
        {
            return Err(refuse("expected plain directory and manifest file".into()));
        }
        let container = Self::open_physical(root)?;
        if container.manifest.version == 0
            || container.manifest.n_layers == 0
            || container.manifest.experts_per_layer == 0
        {
            return Err(refuse("empty or unversioned container".into()));
        }
        let mut bank_bytes = 0u64;
        let mut bank_paths = std::collections::HashSet::new();
        for layer in 0..container.manifest.n_layers {
            for tier in &container.tiers {
                let expected = tier
                    .record_bytes
                    .checked_mul(container.manifest.experts_per_layer as u64)
                    .ok_or_else(|| refuse("bank size overflow".into()))?;
                let path = container.bank_path(layer, tier.id);
                if !bank_paths.insert(path.clone()) {
                    return Err(refuse(format!(
                        "multiple tiers name the same bank {}",
                        path.display()
                    )));
                }
                let metadata = plain_metadata(&path)?;
                if !metadata.is_file() || metadata.len() != expected {
                    return Err(refuse(format!(
                        "{}: expected {expected} bytes, observed {}",
                        path.display(),
                        metadata.len()
                    )));
                }
                bank_bytes = bank_bytes
                    .checked_add(expected)
                    .ok_or_else(|| refuse("container size overflow".into()))?;
            }
        }
        let mut files = Vec::new();
        let mut total_bytes = 0u64;
        for entry in std::fs::read_dir(root).map_err(|error| refuse(error.to_string()))? {
            let entry = entry.map_err(|error| refuse(error.to_string()))?;
            let metadata = plain_metadata(&entry.path())?;
            if !metadata.is_file() {
                return Err(refuse(format!(
                    "non-file container entry {}",
                    entry.path().display()
                )));
            }
            total_bytes = total_bytes
                .checked_add(metadata.len())
                .ok_or_else(|| refuse("whole artifact size overflow".into()))?;
            files.push(ContainerFile {
                name: entry.file_name(),
                bytes: metadata.len(),
            });
        }
        files.sort_by(|left, right| left.name.cmp(&right.name));
        Ok(CompleteContainer {
            root: root.to_path_buf(),
            manifest: container.manifest,
            manifest_bytes: container.manifest_bytes,
            bank_bytes,
            files,
            total_bytes,
        })
    }

    /// Open a container directory: parse + verify the manifest. No bank IO
    /// happens here.
    pub fn open(root: &Path) -> Result<Self, ContainerError> {
        let (resolved, lease) = crate::model_registry::artifacts::expert_placement::reader_lease(
            root,
        )
        .map_err(|reason| ContainerError::WholeGeometry {
            path: root.to_path_buf(),
            reason,
        })?;
        let mut container = Self::open_physical(&resolved)?;
        container._reader_lease = Some(lease);
        Ok(container)
    }

    fn open_physical(root: &Path) -> Result<Self, ContainerError> {
        let manifest_path = root.join("manifest.json");
        let mut raw = String::new();
        File::open(&manifest_path)
            .and_then(|mut f| f.read_to_string(&mut raw))
            .map_err(|source| ContainerError::ManifestIo {
                path: manifest_path.clone(),
                source,
            })?;
        let manifest: ContainerManifest =
            serde_json::from_str(&raw).map_err(|source| ContainerError::ManifestParse {
                path: manifest_path.clone(),
                source,
            })?;
        if manifest.version > Self::KNOWN_VERSION {
            return Err(ContainerError::ManifestVersion {
                path: manifest_path,
                found: manifest.version,
                known: Self::KNOWN_VERSION,
            });
        }
        if manifest.record_bytes == 0 || manifest.record_bytes % RECORD_ALIGN != 0 {
            return Err(ContainerError::RecordMisaligned {
                path: manifest_path,
                record_bytes: manifest.record_bytes,
                align: RECORD_ALIGN,
            });
        }
        // Tier table: ids must equal their index (the descending-fidelity
        // order IS the id space), and the alignment law holds per tier.
        let tiers = manifest.effective_tiers();
        for (index, tier) in tiers.iter().enumerate() {
            if tier.id as usize != index {
                return Err(ContainerError::TierIdMismatch {
                    path: manifest_path,
                    index: index as u16,
                    id: tier.id,
                });
            }
            if tier.record_bytes == 0 || tier.record_bytes % RECORD_ALIGN != 0 {
                return Err(ContainerError::RecordMisaligned {
                    path: manifest_path,
                    record_bytes: tier.record_bytes,
                    align: RECORD_ALIGN,
                });
            }
        }
        let banks = (0..manifest.n_layers as usize * tiers.len())
            .map(|_| None)
            .collect();
        Ok(Self {
            root: root.to_path_buf(),
            manifest,
            manifest_bytes: raw.into_bytes(),
            tiers,
            banks,
            _reader_lease: None,
        })
    }

    /// The validated tier table (single synthesized tier for v1 containers).
    pub fn tiers(&self) -> &[TierSpec] {
        &self.tiers
    }

    pub fn manifest(&self) -> &ContainerManifest {
        &self.manifest
    }

    /// Path of one (layer, tier) bank — the grid shard unit, exposed so
    /// placement can advertise/transfer shards without going through fetch.
    /// v1 containers keep the legacy `experts-L{n}.bin` name (their only
    /// tier); v2 containers name every bank `experts-L{n}-T{t}.bin`, single
    /// tier included — the version picks the naming, never a heuristic.
    pub fn bank_path(&self, layer: u16, tier: u16) -> PathBuf {
        if self.manifest.version < 2 {
            self.root.join(format!("experts-L{layer}.bin"))
        } else {
            self.root.join(format!("experts-L{layer}-T{tier}.bin"))
        }
    }

    /// Fetch one expert's record into `buf` (must be exactly the KEY's
    /// tier's `record_bytes` long). The tier rides IN the key — one identity
    /// end-to-end from the pager's policy seam through the cache to the bank
    /// (a v1/sharp caller uses [`ExpertKey::sharp`] and never mentions
    /// tiers). Opens the (layer, tier) bank on first touch; pure mechanism —
    /// the policy seam (all-star sharp / cruft decayed) chose the tier.
    pub fn fetch(&mut self, key: ExpertKey, buf: &mut [u8]) -> Result<(), ContainerError> {
        if key.layer >= self.manifest.n_layers {
            return Err(ContainerError::LayerOutOfRange {
                layer: key.layer,
                n_layers: self.manifest.n_layers,
            });
        }
        let n_tiers = self.tiers.len() as u16;
        let tier = key.tier;
        if tier >= n_tiers {
            return Err(ContainerError::TierOutOfRange { tier, n_tiers });
        }
        let record_bytes = self.tiers[tier as usize].record_bytes;
        let path = self.bank_path(key.layer, tier);
        let slot = &mut self.banks[key.layer as usize * n_tiers as usize + tier as usize];
        let bank = match slot {
            Some(bank) => bank,
            None => slot.insert(ExpertBank::open(
                &path,
                key.layer,
                record_bytes,
                self.manifest.experts_per_layer,
            )?),
        };
        bank.fetch(key.expert, buf)
    }
}

impl ExpertFetcher for ExpertContainer {
    fn fetch(&mut self, key: ExpertKey, buf: &mut [u8]) -> Result<(), ContainerError> {
        ExpertContainer::fetch(self, key, buf)
    }

    fn record_bytes(&self, tier: u16) -> Option<u64> {
        self.tiers.get(tier as usize).map(|t| t.record_bytes)
    }
}

/// Foundry stand-ins for tests — the ONE fixture per concern (CLAUDE.md
/// test-fixtures rule): every test that needs a container on disk uses these,
/// never a parallel writer. Gated so production binaries cannot link them.
#[cfg(any(test, feature = "test-fixtures"))]
pub mod fixtures {
    use super::*;
    use std::io::Write;

    /// Foundry stand-in: write a tiny valid v1 container (record = one page).
    pub fn write_container(root: &Path, n_layers: u16, experts: u16) -> ContainerManifest {
        let manifest = ContainerManifest {
            version: 1,
            model: "test-moe".into(),
            fmt: "VQ3R".into(),
            record_bytes: RECORD_ALIGN,
            n_layers,
            experts_per_layer: experts,
            activated_per_token: 2,
            top_k_per_layer: Some(1),
            tiers: vec![],
        };
        std::fs::write(
            root.join("manifest.json"),
            serde_json::to_string_pretty(&manifest).expect("serialize"), // JUSTIFIED: test-fixture writer (cfg test/test-fixtures only); a failed write must fail the test loudly
        )
        .expect("write manifest"); // JUSTIFIED: test-fixture writer (cfg test/test-fixtures only); a failed write must fail the test loudly
        for layer in 0..n_layers {
            let mut f = File::create(root.join(format!("experts-L{layer}.bin"))).expect("bank"); // JUSTIFIED: test-fixture writer (cfg test/test-fixtures only); a failed write must fail the test loudly
            for expert in 0..experts {
                let mut rec = vec![0u8; RECORD_ALIGN as usize];
                rec[0..4].copy_from_slice(&RECORD_MAGIC.to_le_bytes());
                rec[4..6].copy_from_slice(&layer.to_le_bytes());
                rec[6..8].copy_from_slice(&expert.to_le_bytes());
                // Distinguishable payload so round-trip is provable.
                rec[HEADER_IDENT_BYTES] = (layer as u8) ^ 0xA0;
                rec[HEADER_IDENT_BYTES + 1] = expert as u8;
                f.write_all(&rec).expect("record"); // JUSTIFIED: test-fixture writer (cfg test/test-fixtures only); a failed write must fail the test loudly
            }
        }
        manifest
    }

    /// Foundry stand-in for the TIERED format (`moec_pack_dir_tiered`,
    /// locked 2026-07-31): manifest v2 with descending-fidelity tiers +
    /// per-(layer,tier) banks `experts-L{n}-T{t}.bin`.
    pub fn write_tiered_container(root: &Path, n_layers: u16, experts: u16) -> ContainerManifest {
        let tiers = vec![
            TierSpec {
                id: 0,
                quant: "IQ2".into(),
                record_bytes: 2 * RECORD_ALIGN,
            },
            TierSpec {
                id: 1,
                quant: "IQ1".into(),
                record_bytes: RECORD_ALIGN,
            },
        ];
        let manifest = ContainerManifest {
            version: 2,
            model: "test-moe".into(),
            fmt: "IQ2".into(),
            record_bytes: 2 * RECORD_ALIGN,
            n_layers,
            experts_per_layer: experts,
            activated_per_token: 2,
            top_k_per_layer: Some(1),
            tiers: tiers.clone(),
        };
        std::fs::write(
            root.join("manifest.json"),
            serde_json::to_string_pretty(&manifest).expect("serialize"), // JUSTIFIED: test-fixture writer (cfg test/test-fixtures only); a failed write must fail the test loudly
        )
        .expect("write manifest"); // JUSTIFIED: test-fixture writer (cfg test/test-fixtures only); a failed write must fail the test loudly
        for layer in 0..n_layers {
            for tier in &tiers {
                let mut f = File::create(root.join(format!("experts-L{layer}-T{}.bin", tier.id)))
                    .expect("bank"); // JUSTIFIED: test-fixture writer (cfg test/test-fixtures only); a failed write must fail the test loudly
                for expert in 0..experts {
                    let mut rec = vec![0u8; tier.record_bytes as usize];
                    rec[0..4].copy_from_slice(&RECORD_MAGIC.to_le_bytes());
                    rec[4..6].copy_from_slice(&layer.to_le_bytes());
                    rec[6..8].copy_from_slice(&expert.to_le_bytes());
                    // Payload distinguishable by TIER too — a tier-0 read
                    // must never come back with tier-1 bytes.
                    rec[HEADER_IDENT_BYTES] = tier.id as u8 ^ 0x5A;
                    rec[HEADER_IDENT_BYTES + 1] = expert as u8;
                    f.write_all(&rec).expect("record"); // JUSTIFIED: test-fixture writer (cfg test/test-fixtures only); a failed write must fail the test loudly
                }
            }
        }
        manifest
    }

    /// Assert a fetched v1 record carries ITS OWN payload signature (the
    /// pattern `write_container` stamps) — shared so depot/pager tests can
    /// prove round-trips without re-learning the fixture's byte layout.
    pub fn assert_v1_record_identity(buf: &[u8], layer: u16, expert: u16) {
        assert_eq!(
            buf[HEADER_IDENT_BYTES],
            (layer as u8) ^ 0xA0,
            "layer payload byte"
        );
        assert_eq!(
            buf[HEADER_IDENT_BYTES + 1],
            expert as u8,
            "expert payload byte"
        );
    }
}

#[cfg(test)]
mod tests;
