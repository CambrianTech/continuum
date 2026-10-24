# Packages: one load/store for every layer, any depth

**Status:** design, 2026-10-24 (Fable). Joel: "A packaging solution is necessary all over ... think in
and out of everything, a load/store customizable for anything. I'd do this for all my layers." /
"Anything that needs to store things gets it easily, by merely implementing. And it handles any depth."

## The problem, measured (core/, 2026-10-24, `rg -t rust`, vendor excluded)

- ~51 `fn save*`, 120 `fn load*`, 51 `*persist*`, 75 `*restore*` implementations; serde_json called
  ~540 times each way; 749 `fs::write(` sites.
- 44 hand-written temp+rename sites in 40 files beside three `continuum-base` helpers with 1-6 callers
  each; most never fsync; four are bare non-atomic writes (`grid/registry.rs:331`,
  `code/repo_registry.rs:36`, `served_window_store.rs:184`, `plasticity/topology.rs`).
- Four copy-pasted engine map stores (`decode_knee`, `serving_rates`, `prefill_rate`, `lane_footprint`)
  that ignore a failed rename and read a corrupt file as empty: a silent fallback, four times.
- A KV page is `.bin` + `.ckpt` + `.lora` + `.dft`, its compatibility in a separate ledger keyed on
  paths and mtimes; publishing the four as one grew a staging dir + commit record + recovery reader
  (fork #70). A page cannot leave its node.
- Genes: three manifests (`TrainedAdapter`, `GeneHandle`, `BundleManifest`), three id schemes
  (`ArtifactId` = 16 of 32 SHA-256 bytes, `AlloyHash` string, bundle bare hex), lineage twice.
- Room/activity state stored three ways (airc wall, ORM, local JSON); no project or persona export.

Every layer re-solves the same four problems (encode, write atomically, version, move) and each
solution is partial. The fix is one layer every type plugs into.

## The design: a type says WHAT it is; a store says WHERE bytes go

Two traits, separated the way NSCoding separates an object from its archive and OCI separates an
image from its registry. Neither knows the other's half.

### `Packable`: the type's side, customizable, usually derived

```rust
pub trait Packable: Sized + Send + Sync + 'static {
    const KIND: Kind;                      // Kind { name: "gene", schema: 3 }
    type Context: ?Sized;                  // what loading needs from the host: model, embedder, keys
    fn pack(&self, p: &mut Packer<'_>) -> Result<(), PackError>;
    fn unpack(u: &mut Unpacker<'_>, cx: &Self::Context) -> Result<Self, PackError>;
}
```

A type describes itself as parts. The `Packer` verbs are the whole vocabulary:

| verb | what it is | stored as |
|---|---|---|
| `field(name, &T)` | small structured data (serde) | inline in the manifest |
| `blob(name, media, source)` | large bytes: a buffer, an mmap, a reader, or a writer closure (the engine fills it) | a blob, streamed and hashed in flight, never copied |
| `child(name, &C)` / `children(name, iter)` | another `Packable`, any kind | a blob that is the child's manifest: recursion |
| `secret(name, &T, audience)` | a field or blob only its owner/audience may read | encrypted part; the digest covers the ciphertext, so stores verify without keys |
| `requires(req)` | what a loader must match: `Model(ContentId)`, `KvFormat{..}`, `Embedder(ContentId)`, `Key(audience)` | in the manifest, checked first |

`#[derive(Packable)]` makes this one line per type; attributes customize parts:

```rust
#[derive(Packable)]
#[pack(kind = "gene", schema = 3, migrate = gene_migrations)]
struct Gene {
    #[pack(blob = "application/vnd.gguf.lora")] weights: Blob,
    signature: BehaviourSignature,               // field
    #[pack(child)] parents: Vec<Handle<Gene>>,  // lineage, recorded once
    #[pack(requires)] base: ModelId,
}
```

A hand-written `pack`/`unpack` is for types that need more (the KV page, whose bytes come out of the
engine); the derive covers the rest.

### `Store`: the medium's side, five verbs

```rust
pub trait Store: Send + Sync {
    fn put(&self, src: BlobSource) -> Result<ContentId, StoreError>;     // idempotent, hashes in flight
    fn get(&self, id: &ContentId, range: Range) -> Result<BlobRead, StoreError>; // bytes | mmap | stream
    fn has(&self, id: &ContentId) -> bool;
    fn set_ref(&self, name: &RefName, to: ContentId, expect: Option<ContentId>) -> Result<(), StoreError>;
    fn get_ref(&self, name: &RefName) -> Option<ContentId>;
}
```

Blobs are immutable and named by content, so they never need a commit protocol. `set_ref` is the ONLY
mutation: one atomic compare-and-swap of a name onto a manifest. Implementations:

- `DiskStore`: `blobs/sha256/<hex>` + `refs/<scope>/<name>`. Its one write path (temp, sync, rename,
  dir sync, errors propagated) is where all 44 hand-written sites and the three base helpers go.
- `RamStore`: bounded, a `ResourcePool` (eviction already exists).
- `Tiered(RamStore, DiskStore, Remote)`: stores composed; promotion and demotion IS paging
  (VRAM > RAM > SSD > peer). `PagedResourcePool` and spill's `LivePin` become its policy, not a parallel path.
- `AircStore`, `OciStore`, `HfStore`, `GhStore`: remotes. Today's `Publisher` trait becomes these.

Transport between any two stores is one generic function: walk the DAG from a root, copy what the
destination lacks (`has`), flip the ref last. Dedup, resume and integrity come with it.

## Depth: a Merkle DAG

A child is a part whose bytes are another package's manifest, so depth is unbounded: a persona holds
memories that hold engrams; a project holds genes that hold their parents. Content addressing makes
it a DAG: a package's digest covers its children, so it cannot contain itself, and an identical
subtree (a gene twenty minds share, a common checkpoint) is stored once.

- **Save** walks bottom-up and flips ONE ref at the root: a whole persona commits atomically or not
  at all. A crash leaves the old root current and unreferenced blobs for collection.
- **Load** walks top-down and lazily: `Handle<T>` is a `ContentId` + a store; `.load()` fetches on
  first use; blob reads take ranges (spill's `read_extent` is this). Opening a project does not pull
  every gene's weights.

## Identity

`ContentId([u8; 32])`: full SHA-256, on the stack, rendered `sha256:<hex>` only at the edges. The
manifest is canonical JSON (sorted keys), so the same value always has the same id, and an OCI
registry accepts it as an artifact manifest unchanged. Provenance (ed25519, `GenomeProvenance`) signs
a manifest's id. This one id replaces `ArtifactId`, `AlloyHash` and the bundle digest.

## Provenance: the Merkle root locks it all down

Joel: "Merkle maintains provenance and locks it all down cryptographically."

- **One signature covers the whole tree.** A root's id commits to every field, blob and child below
  it, so an ed25519 signature over that one id (`GenomeProvenance`, generalized to every kind) vouches
  for every byte at any depth. Change one weight in one gene of a project and the project's id changes.
- **Verified on every load and every transport.** A blob's bytes must hash to the id that names it,
  whichever store or peer it came from, so a remote, a cache or a registry is never trusted for
  integrity, only for availability.
- **Lineage is part of identity.** A gene's `parents`, a page's genes, a project's inputs are children
  or ids inside the manifest, so where something came from cannot be stripped or rewritten without
  changing what it is. Credit travels with the work.
- **Who may load what** is the manifest's `requires(Key(audience))` plus the signer's trust scope:
  a mind accepts packages signed by the scopes it trusts and refuses the rest with a typed reason.
  Secrets stay sealed to their audience even inside a shared tree.

## Versions: a migration chain, never two readers

Each kind carries a schema number and an ordered list of pure functions `manifest(n) -> manifest(n+1)`,
run on load. An unknown newer schema, a digest mismatch, or an unmet `requires` is a typed refusal,
never a default (`unwrap_or_default` on a corrupt file is the defect this removes).

## What it is not

- The ORM stays the QUERY layer for records (`OrmEntity`, `StorageAdapter`). A package can be indexed
  into a collection (its ref + queryable fields); rows do not become packages.
- IPC frames and live wire messages stay wire. Append logs (captures, `experience.jsonl`) keep
  `CappedAppender` for the open tail; sealing tail segments into packages is a later slice.

## One conformance suite for every type

`pack::conformance::<T>()` runs for every `Packable`, so a type that implements the trait inherits the
proof: round trip; same value = same id; a fault at every store operation (a `FaultStore` wrapper)
leaves the previous root loadable; a flipped byte is caught; an unmet `requires` refuses before any
blob read; every recorded past schema migrates; a shared child is stored once; a lazy child loads.
Test the contract once; never per type.

## Cost (ballpark, then measured)

Small state is a manifest-only package: one blob and one ref, two syncs. A 27B KV page is ~2.1 GB;
SHA-256 with the ARM extensions is on the order of 2 GB/s per core, so serial hashing would add ~1 s to
a ~0.6 s spill. Parts therefore hash in flight on their own blocking threads, in parallel. Validator:
spill at 35k tokens on the 27B with and without the digest; over 10% added and local KV parts take a
generation id with the digest computed at first export, with no change to the manifest format.

## Where it lives and the order it lands

`continuum-pack`, a leaf crate beside `continuum-base` (no upward imports), with the derive in
`continuum-pack-derive` and ts-rs exports for clients. The fork's server gets a small C++ writer for
the one kind it produces (the KV page) in the same `DiskStore` layout.

Each slice lands with its validator and deletes what it replaces:
1. The crate: `ContentId`, `Packable` + derive, `DiskStore`, `RamStore`, `Handle`, conformance suite,
   `FaultStore`. The base atomic helpers fold into `DiskStore`.
2. The four engine map stores, `served_window`, `engine_residency`, `mind_state`, `room_directory`,
   `repo_registry`, `grid/registry` become derived `Packable`s (mechanical; one PR, cheap model).
   Then the remaining hand-written temp+rename sites migrate by owner.
3. The KV page kind, in the fork and the engine: `.bin` + sidecars + #70's staging/commit code go; the
   ledger keeps only slot occupancy. Old sidecar pages are a classified one-time cold start.
4. The gene kind: `gene_bundle`, `adapter_manifest`, `GeneHandle`, `ArtifactId` fold into one; HF, GH
   and OCI become stores.
5. The persona as a package: identity, memories, genes (by ref), pages, volatile state as children,
   with secrets for what is hers alone. A whole mind backs up, restores and moves between nodes.
6. `AircStore` transfer between nodes; `project` packages (refs only, never copies of personal data).
