# Documentation Staging Area

**Status**: Ready for review and final organization
**Total**: 122 design documents extracted from implementation directories

## What Happened

All `.md` files scattered across `system/`, `commands/`, and implementation directories have been moved here for proper organization. This does NOT include:
- Test README files (`commands/*/test/README.md`)
- Package documentation in `system/genome/python/venv/`
- Root-level docs like `docs/` or `CLAUDE.md`

## Organization

Documents are grouped by major topic area:

- **architecture/** (16) - System design, core patterns, infrastructure
- **cognition/** (13) - AI reasoning, decision-making, thought processes
- **commands/** (6) - Command system architecture
- **coordination/** (10) - AI-to-AI interaction, turn-taking
- **genome/** (27) - LoRA, fine-tuning, training, providers
- **memory/** (9) - RTOS memory consolidation, hippocampus architecture
- **persona/** (41) - PersonaUser architecture, autonomous loops, CNS

## Next Steps

1. **Review** - Check for duplicates, outdated content, superseded plans
2. **Consolidate** - Merge similar topics (multiple refactor plans, phase docs)
3. **Structure** - Decide final `docs/` organization strategy:
   - By feature? (memory/, coordination/, genome/)
   - By component? (persona/, commands/, architecture/)
   - Chronological? (roadmaps/, implementations/, deprecated/)
4. **Index** - Create navigation/index files
5. **Migrate** - Move to `docs/` with proper structure
6. **Update** - Fix references in `CLAUDE.md` and code comments

## Files to Review

See `DETAILED-MANIFEST.md` for complete file listing by category.

## PR Context

This cleanup covers multiple PRs with scattered documentation:
- Latest: RTOS memory consolidation (10 docs)
- Previous: Genome fine-tuning, persona architecture, coordination
- Historical: Phase implementations, refactoring plans

Many documents may be outdated or superseded - git history will help determine which are still relevant.
