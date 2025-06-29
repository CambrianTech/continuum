# Help Command

## Definition
- **Name**: help
- **Description**: Show help information and sync documentation
- **Category**: Core
- **Icon**: 📚
- **Parameters**: `[section] [sync] [output]`

## Overview
The Help command is the heart of Continuum's self-documenting system. It aggregates README.md files from all command packages and generates live documentation from the actual running system.

## Parameters
- `section`: Help section to show (overview, commands, debugging, setup)
- `sync`: Generate README.md from live help system (boolean)
- `output`: Output file for sync (default: README.md)
- `status_table`: Include command status dashboard table (default: true for sync, false for help)
- `verbose`: Show complete project management dashboard in terminal (includes status table, TODOs, health metrics)

## Usage Examples
```bash
# Show general help (one-liner health status)
python3 ai-portal.py --cmd help

# Show complete project management dashboard
python3 ai-portal.py --cmd help --params '{"verbose": true}'

# Show specific section
python3 ai-portal.py --cmd help --params '{"section": "commands"}'

# Sync documentation with status table (README-driven)
python3 ai-portal.py --cmd help --params '{"sync": true}'

# Sync to custom file
python3 ai-portal.py --cmd help --params '{"sync": true, "output": "API.md"}'

# Sync without status table
python3 ai-portal.py --cmd help --params '{"sync": true, "status_table": false}'
```

## Package Rules
```json
{
  "timeouts": {"client": 15.0, "server": 10.0},
  "retries": {"client": 1, "server": 0},
  "behavior": {"client": "standard", "server": "readme_aggregator"},
  "concurrency": {"client": true, "server": true},
  "sideEffects": ["generates_documentation", "reads_filesystem"]
}
```

## Architecture: README-Driven System with Test Integration
The Help command implements the revolutionary README-driven documentation system with integrated unit test reporting:

### Single Source of Truth
- **README.md files** contain all command documentation
- **getDefinition()** methods parse from README.md
- **help output** reads from README.md files
- **No duplication** between docs and help system

### Documentation Aggregation
1. **Walks command tree** collecting all README.md files
2. **Merges with live definitions** from getDefinition()
3. **Builds hierarchical docs** with cross-references
4. **Generates master README** from live system

### Self-Documenting Flow with Test Integration
```
README.md → getDefinition() → help output → sync → master README.md
    ↑                                                      ↓
Unit Tests → Trace Logs → Dashboard → Agent View → Work Planning
```

### Test-Driven Dashboard Updates
- **Unit tests write to trace logs** in `.continuum/test-results/`
- **Test results populate README status** automatically
- **Dashboard shows live test status** with pass/fail counts
- **Agents can query test results** for work prioritization
- **Dependency graphs include test coverage** information

## Implementation Pattern
```javascript
// Commands read their own README for definition
static getDefinition() {
  const readme = fs.readFileSync('./README.md', 'utf8');
  return parseReadmeForDefinition(readme);
}

// Help aggregates all READMEs
static async execute(params, continuum) {
  const allReadmes = await collectAllReadmes('./');
  return formatHelpFromReadmes(allReadmes);
}
```

## Benefits
- **No stale docs**: README.md IS the help system
- **Perfect consistency**: All documentation from same source
- **Self-documenting**: System documents itself from living code
- **Zero maintenance**: Documentation stays in sync automatically