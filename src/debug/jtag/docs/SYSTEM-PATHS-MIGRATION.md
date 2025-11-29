# SystemPaths Migration Plan

## Problem

User frustration with scattered `.continuum` directory references throughout codebase:

> "why does everything go randomly into a different .continuum dir depending on your mood"
> "the personas are a bunch of numbers" (154ee833 vs helper-ai)
> "like I'd assume, we would have literally ONE .continuum string in the ENTIRE repo"

## Solution

Created `system/core/config/SystemPaths.ts` as **SINGLE SOURCE OF TRUTH** for all filesystem paths.

## Current State

✅ **COMPLETED**:
- Created SystemPaths.ts with organized path categories
- Updated SubsystemLogger to use SystemPaths
- TypeScript compilation passing
- Committed foundational infrastructure

## Directory Structure

```
.continuum/jtag/
├── logs/
│   ├── {persona-name}/        ← NAME-BASED (was UUID)
│   │   ├── mind.log           ← Cognition, state tracking
│   │   ├── body.log           ← Action, execution, tools
│   │   ├── soul.log           ← Memory, learning, genome
│   │   └── cns.log            ← Orchestration, coordination
│   └── system/
│       ├── sql.log            ← Database queries
│       ├── errors.log         ← System-wide errors
│       └── cognition.log      ← Tool execution logs
├── database/
│   ├── main.db                ← Primary SQLite database
│   └── backups/               ← Database backups
├── sessions/
│   ├── user/                  ← User session data
│   └── validation/            ← Test session data
├── registry/
│   ├── process-registry.json  ← Running processes
│   └── dynamic-ports.json     ← Port allocations
├── temp/
│   ├── screenshots/           ← Screenshot files
│   └── artifacts/             ← Temporary artifacts
└── genome/
    ├── lora-adapters/         ← LoRA adapter files
    └── training-data/         ← Training datasets
```

## Migration Tasks

### Priority 1: Core System Files (NEXT)

Files that need immediate migration to SystemPaths:

1. **system/shared/Constants.ts** (10 paths)
   - CONTINUUM, JTAG_DATA, JTAG_BACKUPS, SQLITE_DB, etc.
   - Should import from SystemPaths instead

2. **system/data/config/DatabaseConfig.ts** (5 paths)
   - SQLITE, DATA_DIR, BACKUP_DIR, DATASETS_DIR, LEGACY
   - Replace with SystemPaths.database.*

3. **system/shared/BrowserSafeConfig.ts** (4 paths)
   - logs, screenshots, data_directory, pid_file
   - Use SystemPaths

4. **system/shared/ConfigurationFactory.ts** (4 paths)
   - Same as BrowserSafeConfig
   - Should reference SystemPaths

5. **system/user/server/PersonaUser.ts** (1 path)
   - Per-persona state database path
   - Use SystemPaths for consistency

6. **system/user/server/modules/PersonaLogger.ts** (2 paths)
   - Log directory construction
   - Already uses persona names, needs SystemPaths

7. **system/user/server/modules/PersonaToolExecutor.ts** (1 path)
   - COGNITION_LOG_PATH
   - Use SystemPaths.logs.system

8. **system/user/server/modules/cognitive/memory/Hippocampus.ts** (1 path)
   - Long-term memory database
   - Use SystemPaths.database or persona-specific path

9. **system/core/process/ProcessCoordinator.ts** (2 paths)
   - lockFile, pidFile
   - Use SystemPaths.registry.*

10. **system/core/registry/** (5 files)
    - RegistryPath.ts, RegistrySync.ts
    - Hardcode registry paths
    - Use SystemPaths.registry.*

### Priority 2: Data Layer (AFTER Priority 1)

11. **system/data/adapters/** (2 files)
    - JsonFileAdapter.ts, SQLiteAdapter.ts
    - Database path construction
    - Use SystemPaths.database.*

12. **system/data/services/DataServiceFactory.ts** (12 paths)
    - Multiple database path references
    - Consolidate using SystemPaths

### Priority 3: Genome/Secrets (AFTER Priority 2)

13. **system/genome/** (3 files)
    - LayerLoader.ts, BaseServerLoRATrainer.ts, PEFTLoRAAdapter.ts
    - LoRA adapter paths
    - Use SystemPaths.genome.*

14. **system/secrets/SecretManager.ts** (6 paths)
    - ~/.continuum/config.env handling
    - Special case: $HOME vs repo-root (user's request)

### Priority 4: Tests & Scripts (LAST)

- 100+ test files with hardcoded paths
- Scripts with hardcoded paths
- LOW PRIORITY: These are for testing, not production

## User's Additional Request

> "I'd say one in $home and one in each repo root"

**Analysis**: User wants TWO .continuum directories:
1. **$HOME/.continuum** - User-global config (secrets, preferences)
2. **{repo-root}/.continuum/jtag** - Project-specific data (logs, database)

**Action**: Consider adding to SystemPaths:
```typescript
export const SystemPaths = {
  // Project-specific (already done)
  root: path.join(BASE_DIR, '.continuum', 'jtag'),

  // User-global (NEW)
  userHome: path.join(os.homedir(), '.continuum'),
  userConfig: path.join(os.homedir(), '.continuum', 'config.env'),
  userSecrets: path.join(os.homedir(), '.continuum', 'secrets.json'),
  // ...
}
```

## Migration Strategy

### Phase 1: Update SystemPaths.ts
- Add userHome paths for $HOME/.continuum
- Add any missing categories

### Phase 2: Core System Files (Priority 1)
- Migrate one file at a time
- Test compilation after each change
- Verify runtime behavior with `npm start`

### Phase 3: Data Layer (Priority 2)
- Migrate database-related files
- Ensure backward compatibility with existing .db files

### Phase 4: Genome/Secrets (Priority 3)
- Migrate LoRA and secret paths
- Consider $HOME vs repo-root distinction

### Phase 5: Tests & Scripts (Priority 4)
- Batch migrate test files
- Low risk since tests are isolated

## Testing Checklist

After each migration:
- [ ] TypeScript compilation passes
- [ ] `npm start` deploys successfully
- [ ] Logs appear in correct locations
- [ ] Database connections work
- [ ] Screenshots save to correct directory
- [ ] No console errors about missing paths

## Success Criteria

1. ✅ Only ONE `.continuum` string in codebase (in SystemPaths.ts)
2. ⏳ All system/ files import from SystemPaths
3. ⏳ Human-readable directory names (helper-ai vs 154ee833)
4. ⏳ Clear organization: logs, database, sessions, etc.
5. ⏳ $HOME vs repo-root distinction implemented

## Notes

- **DO NOT break backward compatibility** - existing .continuum directories should still work
- **Migrate incrementally** - one file at a time, test each change
- **Document path changes** - update CLAUDE.md if paths change
- **Consider deprecation** - old paths marked as DEPRECATED_PATHS in SystemPaths.ts
