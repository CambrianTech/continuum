# Workspace Command

## Definition
- **Name**: workspace
- **Description**: Manage workspace directories and paths
- **Category**: Core
- **Icon**: 📁
- **Parameters**: `<action> [workspace] [subdir]`

## Overview
Replaces hardcoded paths with configurable workspace management. Provides clean directory structure for AI agents and development workflows.

## Parameters
- `action`: Action to perform (path, create, list, info)
- `workspace`: Workspace name (default: current project)
- `subdir`: Subdirectory within workspace (optional)

## Usage Examples
```bash
# Get workspace path
python3 ai-portal.py --cmd workspace --params '{"action": "path"}'

# Get specific workspace path
python3 ai-portal.py --cmd workspace --params '{"workspace": "ai-portal", "action": "path"}'

# Create new workspace
python3 ai-portal.py --cmd workspace --params '{"action": "create", "workspace": "my-project"}'

# List all workspaces
python3 ai-portal.py --cmd workspace --params '{"action": "list"}'

# Get workspace info
python3 ai-portal.py --cmd workspace --params '{"action": "info", "workspace": "ai-portal"}'
```

## Package Rules
```json
{
  "timeouts": {"client": 5.0, "server": 1.0},
  "retries": {"client": 0, "server": 0},
  "concurrency": {"client": true, "server": true},
  "sideEffects": ["creates_directories"]
}
```

## Architecture
- **No hardcoded paths**: All directory management goes through workspace command
- **Configurable**: Workspaces can be created dynamically
- **Consistent structure**: `.continuum/workspace_name/` pattern
- **Fast response**: Minimal processing, instant results

## Directory Structure
```
.continuum/
├── ai-portal/          # AI Portal workspace
├── sentinel/           # Sentinel monitoring workspace
├── workspace_name/     # Custom workspaces
└── screenshots/        # Screenshot workspace
```