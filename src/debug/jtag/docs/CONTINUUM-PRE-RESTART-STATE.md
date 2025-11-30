# .continuum Directory State - PRE RESTART

**Date**: 2025-11-29
**Purpose**: Document clean state BEFORE `npm start` to verify persona directory creation

## Status
- ✅ All persona directories deleted (cleanup after path migration chaos)
- ✅ No @ prefix directories
- ✅ No orphaned name-only directories
- ✅ Clean slate for fresh persona creation

## Directory Tree

```
.continuum/
├── datasets
│   ├── parsed
│   └── prepared
├── genome
│   └── python
│       ├── envs
│       │   └── jtag-training
│       ├── micromamba
│       │   └── bin
│       └── pkgs
│           ├── bzip2-1.0.8-hd037594_8
│           ├── ca-certificates-2025.10.5-hbd8a1cb_0
│           ├── cache
│           ├── icu-75.1-hfee45f7_0
│           ├── libexpat-2.7.1-hec049ff_0
│           ├── libffi-3.5.2-he5f378a_0
│           ├── liblzma-5.8.1-h39f12f2_2
│           ├── libsqlite-3.50.4-h4237e3c_0
│           ├── libzlib-1.3.1-h8359307_2
│           ├── ncurses-6.5-h5e97a16_3
│           ├── openssl-3.5.4-h5503f6c_0
│           ├── pip-25.2-pyh8b19718_0
│           ├── python-3.11.14-h18782d2_2_cpython
│           ├── readline-8.2-h1d1bf99_2
│           ├── setuptools-80.9.0-pyhff2d567_0
│           ├── tk-8.6.13-h892fb3f_2
│           ├── tzdata-2025b-h78e105d_0
│           └── wheel-0.45.1-pyhd8ed1ab_1
├── jtag
│   ├── backups
│   ├── data
│   ├── registry
│   ├── signals
│   └── system
│       ├── logs
│       └── state
│           └── bookmarks
├── reports
├── sessions
│   └── validation
│       ├── run_20251129-003444-94567
│       │   ├── logs
│       │   └── screenshots
│       ├── run_20251129-005053-9217
│       │   ├── logs
│       │   └── screenshots
│       ├── run_20251129-011411-29148
│       │   ├── logs
│       │   └── screenshots
│       ├── run_20251129-012428-38753
│       │   ├── logs
│       │   └── screenshots
│       └── run_20251129-013710-48920
│           ├── logs
│           └── screenshots
├── tests
└── training
    └── claude-sessions -> /Users/joel/.claude/projects/-Volumes-FlashGordon-cambrian-continuum

59 directories
```

## Key Observations

**Missing (Expected After Restart)**:
- `personas/` directory - Should be created by seed script
- `jtag/database/` directory - Should be created on first DB access
- `jtag/logs/categorized/` - System logs from Logger

**Present (Infrastructure)**:
- ✅ `genome/python/` - Python training environment (25GB)
- ✅ `jtag/` - JTAG infrastructure root
- ✅ `sessions/validation/` - Test runs preserved
- ✅ `training/claude-sessions/` - Symlink to Claude Code sessions

## Expected After `npm start`

When personas are created, we should see:

```
.continuum/personas/
├── {name}-{shortId}/           # e.g., helper-ai-154ee833
│   ├── data/                   # All databases
│   │   ├── longterm.db        # Hippocampus
│   │   ├── state.db           # PersonaState
│   │   └── memory.db          # Memory systems
│   └── logs/                   # All log files (flat)
│       ├── mind.log
│       ├── body.log
│       ├── soul.log
│       └── cns.log
```

**CORRECT uniqueId Format**: `{name}-{shortId}` (e.g., `helper-ai-154ee833`)
**WRONG Format**: `@username` (e.g., `@helper`) - This was the bug

## Related Files

- `system/core/config/SystemPaths.ts` - Path registry
- `system/user/server/PersonaUser.ts` - Uses `SystemPaths.personas.dir(this.entity.uniqueId)`
- `scripts/seed/personas.ts` - Seed script (may have @ prefix bug)

## Verification After Restart

```bash
# Check persona directories created
find .continuum/personas -maxdepth 1 -type d

# Check database files
find .continuum/personas -name "*.db"

# Check log files
find .continuum/personas -name "*.log"
```

Expected: All directories should use `{name}-{shortId}` format, NO @ prefix.
