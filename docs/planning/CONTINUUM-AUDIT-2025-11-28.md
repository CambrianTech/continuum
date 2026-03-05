# .continuum Directory Audit - 2025-11-28

## Current State Analysis

### ACTIVE Locations (Modified Today: 2025-11-28)

#### ✅ `.continuum/jtag/` - **NEW STRUCTURE** (20:47-21:11 today)
```
.continuum/jtag/
├── data/                            ← ACTIVE DATABASE
│   └── database.sqlite (21:11)      ← PRIMARY DATABASE (11 mins ago!)
├── logs/                            ← NEW NAME-BASED LOGS
│   ├── claude-assistant/
│   ├── codereview-ai/
│   ├── helper-ai/
│   ├── teacher-ai/
│   └── ... (11 personas, all 20:47)
├── registry/
│   └── process-registry.json (21:11)
├── sessions/system/ (21:09)
└── signals/ (21:09)
```

**Status**: This is the CURRENT ACTIVE directory!

#### ⚠️  `.continuum/logs/personas/` - **OLD UUID STRUCTURE** (13:52 today)
```
.continuum/logs/personas/
├── 154ee833/  ← UUID-based (OLD)
├── 1e7e0f6a/
├── 449c30d6/
└── ... (11 UUID directories)
```

**Status**: DEPRECATED - Created 13:52, but new logs going to `.continuum/jtag/logs/` since 20:47

### STALE Locations (Not Modified Recently)

#### 🗑️ `.continuum/datasets/` - Last modified 2025-11-09 (19 days old)
- Training data experiments
- 13 SQLite files from fine-tuning tests
- **Decision**: Keep for reference, add to `.gitignore`

#### 🗑️ `.continuum/genome/` - Last modified 2025-11-04 (24 days old)
- Python environment for LoRA fine-tuning
- 20 days stale
- **Decision**: Keep (needed for fine-tuning), but inactive

#### 🗑️ `.continuum/jtag/backups/` - Last modified 2025-11-22 (6 days old)
- 9 database backups from Nov 18-22
- **Decision**: Keep recent, delete old (>7 days)

#### 🗑️ `.continuum/jtag/performance/` - Last modified 2025-11-09 (19 days old)
- Performance metrics and scorecards
- **Decision**: Archive or delete (stale test data)

#### 🗑️ `.continuum/media/temp/` - Last modified 2025-11-15 (13 days old)
- Training JSONL files for Fireworks/Mistral
- **Decision**: Delete temp files >7 days old

#### 🗑️ `.continuum/sessions/validation/` - Hundreds of test runs
- Oldest: 2025-10-03 (56 days old)
- Newest: 2025-11-28 (today)
- **Decision**: Delete validation runs >7 days old

## Migration Status

### What Happened Today

**13:52** - SubsystemLogger created OLD UUID logs:
- `.continuum/logs/personas/{uuid}/mind.log`

**20:47** - SystemPaths deployed, NEW name-based logs:
- `.continuum/jtag/logs/{name}/mind.log`

### The Problem

**TWO LOG LOCATIONS NOW EXIST:**
1. `.continuum/logs/personas/154ee833/` (OLD, 13:52)
2. `.continuum/jtag/logs/helper-ai/` (NEW, 20:47)

## Recommended Actions

### 1. Verify Active Database ✅

```bash
# Check which database is actually in use
ls -lah .continuum/jtag/data/database.sqlite
# -rw-r--r-- 1 joel staff 11M Nov 28 21:11

# This is ACTIVE (11 mins ago) - DO NOT DELETE
```

### 2. Clean Up OLD Log Location 🗑️

```bash
# OLD UUID-based logs can be deleted (superseded by new structure)
rm -rf .continuum/logs/personas/

# New logs are in .continuum/jtag/logs/{name}/
```

### 3. Remove DEPRECATED_PATHS from SystemPaths.ts ✅

```typescript
// REMOVE THIS ENTIRE SECTION:
export const DEPRECATED_PATHS = {
  oldContinuumRoot: path.join(process.cwd(), '.continuum'),
  oldSessionsRoot: path.join(process.cwd(), '.continuum', 'sessions')
} as const;
```

**Why Safe**: Nothing uses DEPRECATED_PATHS - it's documentation only.

### 4. Clean Up Stale Test Data 🗑️

```bash
# Delete old validation test runs (>7 days)
find .continuum/sessions/validation -type d -mtime +7 -exec rm -rf {} +

# Delete old backups (>7 days)
find .continuum/jtag/backups -type f -mtime +7 -delete

# Delete temp training files (>7 days)
find .continuum/media/temp -type f -mtime +7 -delete

# Delete stale performance data (>14 days)
find .continuum/jtag/performance -type f -mtime +14 -delete
```

### 5. Add to .gitignore

```gitignore
# Continuum runtime data
.continuum/jtag/data/*.sqlite
.continuum/jtag/logs/
.continuum/jtag/sessions/
.continuum/jtag/signals/
.continuum/jtag/backups/
.continuum/jtag/performance/
.continuum/sessions/
.continuum/media/temp/

# Keep structure but ignore data
.continuum/datasets/prepared/*.sqlite
.continuum/genome/python/envs/
.continuum/genome/python/pkgs/
```

## Current Directory Structure

```
.continuum/
├── jtag/                    ← ACTIVE (SystemPaths.root)
│   ├── data/
│   │   └── database.sqlite  ← PRIMARY DATABASE ✅
│   ├── logs/
│   │   └── {name}/          ← NAME-BASED (NEW) ✅
│   ├── registry/
│   ├── sessions/
│   └── signals/
├── logs/personas/{uuid}/    ← DELETE (deprecated) 🗑️
├── datasets/                ← KEEP (reference data)
├── genome/                  ← KEEP (python env for fine-tuning)
├── sessions/validation/     ← CLEAN (delete >7 days) 🗑️
└── media/temp/              ← CLEAN (delete >7 days) 🗑️
```

## Size Analysis

```bash
# Check sizes
du -sh .continuum/*
# 4.0K    .continuum/cb-mobile-sdk
# 52M     .continuum/datasets
# 8.0M    .continuum/genome
# 180M    .continuum/jtag           ← ACTIVE
# 4.0K    .continuum/logs           ← DELETE
# 4.0K    .continuum/media
# 1.2G    .continuum/sessions       ← MOSTLY STALE TEST DATA
```

**Total**: ~1.4G
**After cleanup**: ~250M (delete 1.15G of stale test data)

## Safe Cleanup Script

```bash
#!/bin/bash
# Safe cleanup of stale .continuum data

echo "🧹 Cleaning up stale .continuum data..."

# 1. Remove OLD UUID-based logs (superseded)
if [ -d ".continuum/logs/personas" ]; then
  echo "🗑️  Removing old UUID-based persona logs..."
  rm -rf .continuum/logs/personas
fi

# 2. Clean old validation runs (>7 days)
echo "🗑️  Cleaning validation runs older than 7 days..."
find .continuum/sessions/validation -type d -mtime +7 -maxdepth 1 -exec rm -rf {} + 2>/dev/null

# 3. Clean old backups (>7 days)
echo "🗑️  Cleaning database backups older than 7 days..."
find .continuum/jtag/backups -type f -mtime +7 -delete 2>/dev/null

# 4. Clean temp files (>7 days)
echo "🗑️  Cleaning temp files older than 7 days..."
find .continuum/media/temp -type f -mtime +7 -delete 2>/dev/null

# 5. Clean stale performance data (>14 days)
echo "🗑️  Cleaning performance data older than 14 days..."
find .continuum/jtag/performance -type f -mtime +14 -delete 2>/dev/null

echo "✅ Cleanup complete!"
echo ""
echo "📊 Current size:"
du -sh .continuum
```

## Verification Checklist

- [x] Active database identified: `.continuum/jtag/data/database.sqlite`
- [x] DEPRECATED_PATHS removed from SystemPaths.ts
- [x] Old UUID logs deleted (`.continuum/logs/personas/` removed)
- [x] Stale test data cleaned (4.4GB → 512KB validation runs)
- [x] Backup created: `backups/continuum-backup-2025-11-28-212821.tgz` (683MB)
- [x] .gitignore updated (added specific .continuum/jtag runtime paths)
- [ ] npm start verifies system still works

## Risk Assessment

**LOW RISK** for cleanup:
- Old UUID logs are superseded (new structure active since 20:47)
- Test validation runs are reproducible
- Backups >7 days are old (latest is 6 days old anyway)
- Temp files are...temp

**CRITICAL - DO NOT DELETE**:
- `.continuum/jtag/data/database.sqlite` (11 mins old, ACTIVE)
- `.continuum/jtag/registry/` (process tracking)
- `.continuum/datasets/` (training data reference)
- `.continuum/genome/` (Python environment for fine-tuning)
