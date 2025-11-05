# Roadmap Command

Parse and analyze project roadmap items from ROADMAP.md files.

## Command Definition

```yaml
name: roadmap
description: Parse and analyze project roadmap items
icon: 🗺️
category: planning
```

## Parameters

- `action` (string, optional): Action to perform - list, analyze, filter, status
- `filter` (string, optional): Filter criteria - high-impact, low-complexity, ready
- `format` (string, optional): Output format - json, table, summary  
- `file` (string, optional): Custom roadmap file path

## Examples

```bash
continuum roadmap
continuum roadmap --action list --format json
continuum roadmap --filter high-impact
continuum roadmap --action status
```

## Features

- Parse markdown roadmap files
- Extract priority, complexity, and impact ratings
- Filter by various criteria
- Multiple output formats
- Dependency analysis
- Status tracking

## Self-Contained

This command is completely self-contained with its own:
- Parameter parsing
- Error handling  
- Output formatting
- Test suite
- Documentation

Can be dropped into any Continuum installation and works immediately.