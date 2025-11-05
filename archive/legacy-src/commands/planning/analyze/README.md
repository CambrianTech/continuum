# Analyze Command

Strategic analysis and dependency planning for projects and codebases.

## Command Definition

```yaml
name: analyze
description: Strategic analysis and dependency planning  
icon: 🔍
category: planning
```

## Parameters

- `target` (string, optional): What to analyze - roadmap, dependencies, risk, codebase
- `filter` (string, optional): Filter criteria - high-impact, low-complexity, ready
- `format` (string, optional): Output format - json, table, graph, summary
- `depth` (number, optional): Analysis depth level (1-5)

## Examples

```bash
continuum analyze roadmap
continuum analyze dependencies --format graph
continuum analyze risk --filter high-impact
continuum analyze codebase --depth 3
```

## Features

- Strategic roadmap analysis
- Dependency graph visualization
- Risk assessment and prioritization
- Codebase complexity analysis
- Multiple output formats
- Configurable analysis depth

## Self-Contained

Complete drop-in command with its own tests and documentation.