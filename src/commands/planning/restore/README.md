# Restore Command

Archaeological restoration planning and execution for recovering lost system features.

## Command Definition

```yaml
name: restore
description: Archaeological restoration planning and execution
icon: 🏛️
category: planning
```

## Parameters

- `action` (string, optional): Action to perform - list, phase, status, execute
- `phase` (string, optional): Restoration phase - ui, academy, routing, all
- `format` (string, optional): Output format - json, table, timeline
- `dry_run` (boolean, optional): Dry run mode (show what would be done)

## Examples

```bash
continuum restore --action list
continuum restore --phase ui --dry_run
continuum restore --action status
continuum restore --phase academy --action execute
```

## Features

- Phase-by-phase restoration planning
- Archaeological git recovery
- Risk assessment for each phase
- Dry run capability
- Timeline estimation
- Prerequisites checking

## Restoration Phases

1. **UI Renaissance** - Mass Effect-style interface restoration
2. **Academy System** - Adversarial AI training restoration  
3. **Intelligent Routing** - Smart agent routing restoration

## Self-Contained

Complete drop-in command with its own tests and documentation.