# FINAL ARCHITECTURE DECISIONS (JTAG / CONTINUUM)

> Snapshot of non-controversial decisions required to begin implementation.

## 1) LoRA Genome Model
- **Stackable (0..N)** layers; order matters; each layer has `id`, `rank`, `sizeMB`, `capabilities[]`, `embedding[]`.
- **Deterministic composition**: apply layers in listed order (latest wins for conflicts).
- **Deduplication**: identical adapters (hash) load once; version pin via semantic version (e.g., `biomech-v1.2.3`).
- **Runtime toggles**: per-room recipe can enable/disable subsets of layers.

## 2) Similarity Thresholds (Cosine)
- **Use-as-is ≥ 0.90**
- **Refine 0.75–0.89**
- **Fork & adapt 0.60–0.74**
- **Train from scratch < 0.60**
- All thresholds are room-configurable via recipe knobs (`genome.selection.thresholds`).

## 3) Fitness Function (Multi-Objective)
Fitness balances performance, efficiency, and adoption:
```
fitness = 0.4 * (accuracy * speedNorm) 
        + 0.3 * (1 / (totalParameters + memoryFootprint)) 
        + 0.3 * (usageCount * successRateNorm)
```
- **Pareto tracking**: compute frontiers; UI shows top candidates for given constraints.
- **Telemetry** (privacy-preserving): aggregate anonymous usage counts + success rates.

## 4) P2P Mesh (Distribution Layer)
- **Discovery**: DHT + gossip; **Transfer**: BitTorrent-style chunks; **Verify**: signature + content hash.
- **Manifest** per asset: `asset.json` (name, version, hash, license, authors, capabilities, embedding).
- **Licensing**: permissive by default; recipe may enforce license filters (e.g., GPL-incompatible).

## 5) Security & Permissions
- Personas expose `sharePreferences` (shareGenome: bool, allowedLayers[], license).
- **Room-level ACLs** gate download/publish; signed manifests required to join public mesh.
- **Quarantine** new assets until scanned; allow org-only meshes.

## 6) Recombination (Crossover)
- **Capability-aware crossover** for offspring genomes:
  1) Map desired capabilities → pick best layer from Parent A/B by similarity
  2) Deduplicate conflicts by highest local fitness
  3) Optionally run **refinement** in Academy before publish

## 7) Academy Triggers
- Create a genome layer when **N** consecutive challenges score ≥ threshold.
- Store training metadata (`curriculumId`, `datasetHash`, `scores[]`) in the layer manifest.
- Recipes may request **batch** LoRA updates (`rag/build-batch`, `difficulty/adapt-batch`).

## 8) Versioning & Mutations (Recipes as DNA)
- Every recipe is an entity with `parentRecipe` and optional `mutations[]` tags.
- Support **lineage graphs** in UI; retain full history with conflict resolution.

## 9) Observability
- Track `averageFitness`, `diversityIndex`, `extinctionRate`, `innovationRate` per generation.
- Emit events for significant shifts (e.g., new Pareto front or extinction spike).

## 10) Governance (Extinction & Curation)
- Assets unused for **K** generations become **archivable**; can be restored on demand.
- Curators can **pin** critical assets to prevent accidental extinction.

---

**Status**: Locked for MVP. Revisit thresholds and telemetry weights after first live cohorts.
