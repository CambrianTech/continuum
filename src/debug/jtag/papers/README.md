# Academic Papers

Research papers written in markdown, version-controlled alongside implementation.

**Philosophy**: Code and papers evolve together. If you wouldn't write the Constitution in Google Docs, don't write research papers there either. Markdown + Git = traceable intellectual lineage.

## Papers in Progress

### 1. Consent-Based Attention (READY FOR DRAFTING)
**Status**: Implementation complete in sentinel-ai repo, validated metrics
**Path**: `consent-based-attention/`
**Key Contribution**: Neural attention heads with agency signals (state/consent/utilization)
**Evidence**: 50% pruning maintains 98% quality, perplexity 975→211, 2.26x speedup

### 2. LoRA Genome Attribution (ARCHITECTURE COMPLETE)
**Status**: Architecture designed, implementation Phase 6+
**Path**: `lora-genome-attribution/`
**Key Contribution**: Skill-decomposed adapters as natural attribution boundaries
**Evidence**: Virtual memory for skills, 64-512MB per adapter, LRU eviction

### 3. Ethical Attribution Architecture (VISION COMPLETE)
**Status**: Full system design, needs end-to-end implementation
**Path**: `ethical-attribution-architecture/`
**Key Contribution**: Three-layer attribution (attention → adapter → sample)
**Evidence**: Combines Sentinel agency + LoRA genome + training provenance

## Writing Standards

- **Markdown format**: LaTeX can be generated later
- **Git version control**: Every revision tracked
- **Code references**: Link to actual implementation with line numbers
- **Reproducible**: Instructions to run experiments
- **Honest**: No bullshit, no hype, show the gaps

## Target Venues

- **NeurIPS**: Consent-based attention (novel architecture)
- **ICML**: LoRA genome attribution (ML systems)
- **ACM FAccT**: Ethical attribution (fairness/accountability/transparency)
- **arXiv first**: Always. Academic gatekeeping can wait.

## Submission Strategy

1. Write in markdown here
2. Get feedback from repo collaborators (issues/PRs)
3. Convert to LaTeX when ready for submission
4. Post preprint to arXiv
5. Submit to conference
6. Keep markdown as canonical version (LaTeX is just export format)

---

**"Fuck Notion. Fuck Google Docs. Fuck vendor lock-in. Markdown + Git = intellectual freedom."**

*Last updated: 2025-11-05*
