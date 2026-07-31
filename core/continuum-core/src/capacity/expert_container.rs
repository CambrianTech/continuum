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
use std::os::unix::fs::FileExt;
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
    pub fmt: String,
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
                quant: self.fmt.clone(),
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
    ManifestVersion { path: PathBuf, found: u32, known: u32 },
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
        self.file
            .read_exact_at(buf, offset)
            .map_err(|source| ContainerError::BankIo {
                path: self.path.clone(),
                source,
            })?;
        self.verify_identity(buf, offset, expert)
    }

    fn verify_identity(
        &self,
        buf: &[u8],
        offset: u64,
        expert: u16,
    ) -> Result<(), ContainerError> {
        let found_magic = u32::from_le_bytes(buf[0..4].try_into().expect("8-byte header"));
        let found_layer = u16::from_le_bytes(buf[4..6].try_into().expect("8-byte header"));
        let found_expert = u16::from_le_bytes(buf[6..8].try_into().expect("8-byte header"));
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
    /// Effective tier table (synthesized single tier for v1) — validated at
    /// open so fetch-time indexing is pure arithmetic.
    tiers: Vec<TierSpec>,
    /// Lazily-opened banks, indexed `layer × n_tiers + tier`.
    banks: Vec<Option<ExpertBank>>,
}

impl ExpertContainer {
    /// Highest manifest version this reader understands. v2 adds `tiers[]`
    /// + per-(layer,tier) bank naming (`moec_pack_dir_tiered` writer side,
    /// locked 2026-07-31).
    pub const KNOWN_VERSION: u32 = 2;

    /// Open a container directory: parse + verify the manifest. No bank IO
    /// happens here.
    pub fn open(root: &Path) -> Result<Self, ContainerError> {
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
            tiers,
            banks,
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

    /// Fetch one expert's record at the SHARPEST tier (tier 0) — the v1 call
    /// shape, kept so single-tier callers never mention tiers.
    pub fn fetch(&mut self, key: ExpertKey, buf: &mut [u8]) -> Result<(), ContainerError> {
        self.fetch_tier(key, 0, buf)
    }

    /// Fetch one expert's record at `tier` into `buf` (must be exactly that
    /// tier's `record_bytes` long). Opens the (layer, tier) bank on first
    /// touch. The PAGER's policy seam decides which tier to ask for
    /// (all-star sharp / cruft decayed); this is pure mechanism.
    pub fn fetch_tier(
        &mut self,
        key: ExpertKey,
        tier: u16,
        buf: &mut [u8],
    ) -> Result<(), ContainerError> {
        if key.layer >= self.manifest.n_layers {
            return Err(ContainerError::LayerOutOfRange {
                layer: key.layer,
                n_layers: self.manifest.n_layers,
            });
        }
        let n_tiers = self.tiers.len() as u16;
        if tier >= n_tiers {
            return Err(ContainerError::TierOutOfRange { tier, n_tiers });
        }
        let record_bytes = self.tiers[tier as usize].record_bytes;
        let path = self.bank_path(key.layer, tier);
        let slot = &mut self.banks[key.layer as usize * n_tiers as usize + tier as usize];
        if slot.is_none() {
            *slot = Some(ExpertBank::open(
                &path,
                key.layer,
                record_bytes,
                self.manifest.experts_per_layer,
            )?);
        }
        slot.as_ref().expect("just opened").fetch(key.expert, buf)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    /// Foundry stand-in: write a tiny valid container (record = one page).
    fn write_container(root: &Path, n_layers: u16, experts: u16) -> ContainerManifest {
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
            serde_json::to_string_pretty(&manifest).expect("serialize"),
        )
        .expect("write manifest");
        for layer in 0..n_layers {
            let mut f = File::create(root.join(format!("experts-L{layer}.bin"))).expect("bank");
            for expert in 0..experts {
                let mut rec = vec![0u8; RECORD_ALIGN as usize];
                rec[0..4].copy_from_slice(&RECORD_MAGIC.to_le_bytes());
                rec[4..6].copy_from_slice(&layer.to_le_bytes());
                rec[6..8].copy_from_slice(&expert.to_le_bytes());
                // Distinguishable payload so round-trip is provable.
                rec[HEADER_IDENT_BYTES] = (layer as u8) ^ 0xA0;
                rec[HEADER_IDENT_BYTES + 1] = expert as u8;
                f.write_all(&rec).expect("record");
            }
        }
        manifest
    }

    #[test]
    fn fetch_round_trips_every_expert_in_one_read() {
        // what this catches: offset arithmetic drift (the reuse=0 bug shape) —
        // every (layer, expert) must come back with ITS OWN header + payload.
        let dir = tempfile::tempdir().expect("tempdir");
        write_container(dir.path(), 3, 4);
        let mut c = ExpertContainer::open(dir.path()).expect("open");
        let mut buf = vec![0u8; c.manifest().record_bytes as usize];
        for layer in 0..3u16 {
            for expert in 0..4u16 {
                c.fetch(ExpertKey { layer, expert }, &mut buf).expect("fetch");
                assert_eq!(buf[HEADER_IDENT_BYTES], (layer as u8) ^ 0xA0);
                assert_eq!(buf[HEADER_IDENT_BYTES + 1], expert as u8);
            }
        }
    }

    #[test]
    fn geometry_violations_fail_loud_at_open_not_at_read() {
        // what this catches: silent tolerance of a truncated bank or
        // misaligned record size — both must refuse before any fetch.
        let dir = tempfile::tempdir().expect("tempdir");
        let mut manifest = write_container(dir.path(), 1, 4);

        // Truncated bank: chop one record off.
        let bank = dir.path().join("experts-L0.bin");
        let full = std::fs::read(&bank).expect("read bank");
        std::fs::write(&bank, &full[..full.len() - RECORD_ALIGN as usize]).expect("truncate");
        let mut c = ExpertContainer::open(dir.path()).expect("manifest still fine");
        let mut buf = vec![0u8; RECORD_ALIGN as usize];
        let err = c
            .fetch(ExpertKey { layer: 0, expert: 0 }, &mut buf)
            .expect_err("truncated bank must refuse");
        assert!(matches!(err, ContainerError::BankGeometry { .. }), "{err}");

        // Misaligned record_bytes: manifest itself must refuse.
        manifest.record_bytes = RECORD_ALIGN + 512;
        std::fs::write(
            dir.path().join("manifest.json"),
            serde_json::to_string(&manifest).expect("serialize"),
        )
        .expect("rewrite manifest");
        let err = ExpertContainer::open(dir.path()).expect_err("misaligned must refuse");
        assert!(matches!(err, ContainerError::RecordMisaligned { .. }), "{err}");
    }

    #[test]
    fn record_identity_mismatch_is_an_error_not_a_wrong_answer() {
        // what this catches: a drifted offset table serving expert A's bytes
        // for expert B — the identity check must turn that into an ERROR.
        let dir = tempfile::tempdir().expect("tempdir");
        write_container(dir.path(), 1, 2);
        // Corrupt record 1's header to claim it is expert 0.
        let bank = dir.path().join("experts-L0.bin");
        let mut bytes = std::fs::read(&bank).expect("read");
        let off = RECORD_ALIGN as usize + 6;
        bytes[off..off + 2].copy_from_slice(&0u16.to_le_bytes());
        std::fs::write(&bank, &bytes).expect("write");

        let mut c = ExpertContainer::open(dir.path()).expect("open");
        let mut buf = vec![0u8; RECORD_ALIGN as usize];
        let err = c
            .fetch(ExpertKey { layer: 0, expert: 1 }, &mut buf)
            .expect_err("identity mismatch must refuse");
        assert!(matches!(err, ContainerError::RecordIdentity { .. }), "{err}");
    }

    #[test]
    fn ecache_over_container_skips_disk_on_a_recurring_working_set() {
        // what this catches: the #268/#269 seam composing wrong — with a
        // budget above the cliff, the SECOND pass over the same working set
        // must hit cache and issue ZERO container reads. This is the whole
        // point of the lane: reuse > 0, measured, not assumed.
        use super::super::expert_ecache::{EcacheBudget, EvictionPolicy, ExpertEcache};

        let dir = tempfile::tempdir().expect("tempdir");
        let manifest = write_container(dir.path(), 2, 8);
        let mut container = ExpertContainer::open(dir.path()).expect("open");

        let budget = EcacheBudget::derive(
            32 * RECORD_ALIGN, // 32 records available >> 2-per-token cliff
            manifest.activated_per_token,
            manifest.record_bytes,
        )
        .expect("above the cliff");
        let mut cache = ExpertEcache::new(budget, EvictionPolicy::Lfru);

        let working_set: Vec<ExpertKey> = (0..2u16)
            .flat_map(|layer| (0..8u16).map(move |expert| ExpertKey { layer, expert }))
            .collect();
        let mut buf = vec![0u8; manifest.record_bytes as usize];
        let mut disk_reads = 0usize;
        for pass in 0..2 {
            for &key in &working_set {
                if !cache.touch(key) {
                    container.fetch(key, &mut buf).expect("fetch");
                    disk_reads += 1;
                }
            }
            if pass == 0 {
                assert_eq!(disk_reads, working_set.len(), "cold pass reads each once");
            }
        }
        assert_eq!(
            disk_reads,
            working_set.len(),
            "warm pass must be served entirely from cache — zero new disk reads"
        );
    }

    /// Foundry stand-in for the TIERED format (`moec_pack_dir_tiered`,
    /// locked 2026-07-31): manifest v2 with descending-fidelity tiers +
    /// per-(layer,tier) banks `experts-L{n}-T{t}.bin`.
    fn write_tiered_container(root: &Path, n_layers: u16, experts: u16) -> ContainerManifest {
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
            serde_json::to_string_pretty(&manifest).expect("serialize"),
        )
        .expect("write manifest");
        for layer in 0..n_layers {
            for tier in &tiers {
                let mut f = File::create(root.join(format!("experts-L{layer}-T{}.bin", tier.id)))
                    .expect("bank");
                for expert in 0..experts {
                    let mut rec = vec![0u8; tier.record_bytes as usize];
                    rec[0..4].copy_from_slice(&RECORD_MAGIC.to_le_bytes());
                    rec[4..6].copy_from_slice(&layer.to_le_bytes());
                    rec[6..8].copy_from_slice(&expert.to_le_bytes());
                    // Payload distinguishable by TIER too — a tier-0 read
                    // must never come back with tier-1 bytes.
                    rec[HEADER_IDENT_BYTES] = tier.id as u8 ^ 0x5A;
                    rec[HEADER_IDENT_BYTES + 1] = expert as u8;
                    f.write_all(&rec).expect("record");
                }
            }
        }
        manifest
    }

    // what this catches: the v2 tiered contract end-to-end — the reader mirror
    // of the packer's own 82-test round-trip. Every (layer, expert, tier) must
    // come back from ITS tier's bank at ITS tier's record size with ITS OWN
    // header + tier-distinguishable payload; offset = expert × tier.record_bytes.
    #[test]
    fn tiered_fetch_round_trips_every_expert_at_every_tier() {
        let dir = tempfile::tempdir().expect("tempdir");
        write_tiered_container(dir.path(), 2, 3);
        let mut c = ExpertContainer::open(dir.path()).expect("open v2");
        assert_eq!(c.tiers().len(), 2);
        for tier in 0..2u16 {
            let record_bytes = c.tiers()[tier as usize].record_bytes;
            let mut buf = vec![0u8; record_bytes as usize];
            for layer in 0..2u16 {
                for expert in 0..3u16 {
                    c.fetch_tier(ExpertKey { layer, expert }, tier, &mut buf)
                        .expect("fetch_tier");
                    assert_eq!(buf[HEADER_IDENT_BYTES], tier as u8 ^ 0x5A);
                    assert_eq!(buf[HEADER_IDENT_BYTES + 1], expert as u8);
                }
            }
        }
    }

    // what this catches: v1 stays the degenerate single-tier case — a raw v1
    // manifest JSON WITHOUT any `tiers` field parses, synthesizes one tier
    // from the top-level record_bytes, and the tier-less fetch() keeps
    // reading legacy `experts-L{n}.bin` banks. Back-compat is the contract,
    // not an accident.
    #[test]
    fn v1_manifest_without_tiers_field_reads_as_single_tier() {
        let dir = tempfile::tempdir().expect("tempdir");
        write_container(dir.path(), 1, 2);
        // Strip the serialized `tiers` field entirely — a pre-v2 foundry
        // never wrote one.
        let raw = std::fs::read_to_string(dir.path().join("manifest.json")).expect("read");
        let mut v: serde_json::Value = serde_json::from_str(&raw).expect("json");
        v.as_object_mut().expect("obj").remove("tiers");
        std::fs::write(
            dir.path().join("manifest.json"),
            serde_json::to_string(&v).expect("serialize"),
        )
        .expect("rewrite");

        let mut c = ExpertContainer::open(dir.path()).expect("open v1");
        assert_eq!(c.tiers().len(), 1, "v1 = one synthesized tier");
        assert_eq!(c.tiers()[0].record_bytes, RECORD_ALIGN);
        let mut buf = vec![0u8; RECORD_ALIGN as usize];
        c.fetch(ExpertKey { layer: 0, expert: 1 }, &mut buf)
            .expect("legacy fetch path");
        assert_eq!(buf[HEADER_IDENT_BYTES + 1], 1);
    }

    // what this catches: tier indexing refusals — an out-of-range tier and a
    // tier table whose ids don't equal their index (order drift between
    // writer and reader) must both refuse loudly, never mis-read a bank.
    #[test]
    fn tier_range_and_id_order_violations_refuse() {
        let dir = tempfile::tempdir().expect("tempdir");
        let mut manifest = write_tiered_container(dir.path(), 1, 1);
        let mut c = ExpertContainer::open(dir.path()).expect("open");
        let mut buf = vec![0u8; RECORD_ALIGN as usize];
        let err = c
            .fetch_tier(ExpertKey { layer: 0, expert: 0 }, 2, &mut buf)
            .expect_err("tier 2 of 2 must refuse");
        assert!(matches!(err, ContainerError::TierOutOfRange { .. }), "{err}");

        manifest.tiers.swap(0, 1); // ids no longer equal their index
        std::fs::write(
            dir.path().join("manifest.json"),
            serde_json::to_string(&manifest).expect("serialize"),
        )
        .expect("rewrite");
        let err = ExpertContainer::open(dir.path()).expect_err("id/index drift must refuse");
        assert!(matches!(err, ContainerError::TierIdMismatch { .. }), "{err}");
    }

    #[test]
    fn unknown_manifest_version_is_refused() {
        // what this catches: a v2 foundry container silently misread by a v1
        // reader — version gates the whole open.
        let dir = tempfile::tempdir().expect("tempdir");
        let mut manifest = write_container(dir.path(), 1, 1);
        manifest.version = ExpertContainer::KNOWN_VERSION + 1;
        std::fs::write(
            dir.path().join("manifest.json"),
            serde_json::to_string(&manifest).expect("serialize"),
        )
        .expect("rewrite");
        let err = ExpertContainer::open(dir.path()).expect_err("future version must refuse");
        assert!(matches!(err, ContainerError::ManifestVersion { .. }), "{err}");
    }
}
