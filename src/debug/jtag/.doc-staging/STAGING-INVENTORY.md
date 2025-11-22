# Documentation Staging Inventory

**Created**: 2025-11-22
**Purpose**: Organize scattered markdown files before finalizing docs/ structure

## Summary by Category

### Architecture (16 docs)
System-level design decisions, core patterns, infrastructure.

### Cognition (13 docs)
AI decision-making, reasoning, memory, thought processes.

### Commands (6 docs)
Command architecture, specific command implementations.

### Coordination (10 docs)
AI-to-AI interaction, turn-taking, thoughtstream, coordination primitives.

### Genome (27 docs)
LoRA adapters, fine-tuning, training pipelines, provider integrations.

### Memory (9 docs)
RTOS-style memory consolidation, hippocampus architecture, lean core loop.

### Persona (41 docs)
PersonaUser architecture, autonomous loops, CNS, phases, roadmaps.

## Total: 122 design documents moved from implementation directories

## Next Steps

1. Review each category for duplicates/outdated docs
2. Decide final docs/ structure (by feature? by component? chronological?)
3. Create index files for navigation
4. Move to final docs/ location
5. Update references in code/CLAUDE.md

## Notes

- Many "roadmap" and "plan" docs may be outdated (check git history)
- Some phase docs (phase-3bis, phase-6) may be superseded
- Consider consolidating similar topics (e.g., multiple persona refactor plans)
- READMEs in test/ directories were left in place (legitimate package docs)
