# Docs Command

## Definition
- **Name**: docs
- **Description**: Generate dynamic documentation from help system
- **Category**: Core  
- **Icon**: 📖
- **Status**: 🟢 WORKING (Fixed: 2025-06-18)
- **Parameters**: `[format] [output] [include] [sync]`

## Overview  
✅ **COMMAND RESTORED AND ENHANCED** - Safe to use

The Docs command generates comprehensive documentation from the live help system and integrates with the file structure sync system for complete documentation management.

## Parameters
- `format`: Output format (markdown, json) - default: markdown
- `output`: Output file path - default: README.md  
- `include`: What to include (all, commands, help, agents) - default: all
- `sync`: Sync FILES.md with current file structure first - default: false

## Usage Examples
```bash
# Generate documentation from live help system
python3 ai-portal.py --cmd docs

# Sync file structure and generate docs
python3 ai-portal.py --cmd docs --sync

# Generate specific documentation 
python3 ai-portal.py --cmd docs --include commands --output COMMANDS.md

# Export as JSON
python3 ai-portal.py --cmd docs --format json --output api.json
```

## ✅ UNIFIED DOCUMENTATION WORKFLOW
**Perfect Integration with Thin Client Architecture:**

1. **`--cmd docs --sync`**: Updates FILES.md (preserves archaeological content) + generates README.md
2. **`--cmd help --sync`**: Uses help system to sync documentation 
3. **Single generator**: `scripts/generate-files-tree.sh` preserves agent content
4. **Thin client**: All documentation accessible via `python-client/ai-portal.py`

**Benefits:**
- **One command**: `docs --sync` handles everything
- **Preserves archaeology**: Generator keeps tombstones and visual documentation
- **Live system**: Documentation from actual running commands  
- **No duplication**: Single source of truth for file structure

## Package Rules
```json
{
  "timeouts": {"client": 30.0, "server": 20.0},
  "retries": {"client": 1, "server": 0},
  "concurrency": {"client": true, "server": false},
  "sideEffects": ["file_system", "calls_shell_scripts"],
  "status": "WORKING"
}
```

## Status History
- 🟢 **2025-06-18**: RESTORED - Fixed and enhanced with file sync integration
- 🔴 **2025-06-15**: BROKEN - Memory leak causes server crashes
- 🟡 **2025-06-12**: TESTING - PDF generation failing
- 🟢 **2025-06-10**: STABLE - Last known working version

## Architecture: Thin Client Documentation System

**Command Bus Integration:**
```javascript
// Unified workflow through thin client
python3 ai-portal.py --cmd docs --sync
  ↓
DocsCommand.execute() 
  ↓
syncFileStructure() → calls scripts/generate-files-tree.sh
  ↓  
generateDocs() → creates README.md from live help system
```

**Key Benefits:**
- **Single entry point**: All documentation via AI portal
- **Live system**: Always up-to-date with running commands
- **Preserves archaeology**: File generator keeps agent content
- **Thin client pattern**: No business logic in client, all in command bus