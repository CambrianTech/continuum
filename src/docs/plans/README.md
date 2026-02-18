# Planning Documents

This directory contains **temporary planning documents** for active refactoring efforts and architectural changes.

## Purpose

Planning docs help us:
- Break down complex refactoring into phases
- Track implementation progress
- Document decisions and tradeoffs
- Provide step-by-step implementation guides

## Lifecycle

**Active Plans**: While work is in progress, the plan stays here.

**Completed Plans**: Once fully implemented, either:
- Delete the plan (implementation is self-documenting)
- Move key insights to permanent architecture docs in parent `docs/` directory

## Organization

```
docs/                          # Permanent architecture documentation
  ├── ARCHITECTURE-RULES.md    # Core architecture principles
  ├── UNIVERSAL-PRIMITIVES.md  # Commands & Events patterns
  └── ...
docs/plans/                    # Temporary refactoring plans
  ├── SQLITE-ADAPTER-REFACTORING-PLAN.md
  └── ...
```

## Current Plans

### SQLITE-ADAPTER-REFACTORING-PLAN.md
**Status**: Active
**Goal**: Reduce SqliteStorageAdapter from 2277 → ~1107 lines
**Phases**: 7 phases of refactoring (2A, 2B, 2C)
**Started**: 2025-01-27
**Progress**: Phase 1 complete (utility extraction)

---

**Note**: Keep plans concise and actionable. Once work is done, clean up this directory.
