# Continuum - AI Workforce Construction Platform
> 📖 Documentation auto-generated from live help system
> 🔄 To update: `python3 python-client/ai-portal.py --cmd help --sync`

## Overview

Continuum is a revolutionary AI workforce construction platform with clean architecture principles:

- **🏗️ Command Bus Architecture**: Central orchestration with modular commands
- **🤖 AI Portal**: Primary interface for AI agents (`python-client/ai-portal.py`)
- **📡 Promise-based API**: Clean async/await patterns across all clients
- **🛡️ Sentinel System**: Monitoring and logging for AI task management  
- **📁 Workspace Management**: No hardcoded paths, configurable workspaces
- **📚 Self-documenting**: Live help system keeps docs in sync

### Quick Start

```bash
# For AI Agents (primary interface)
python3 python-client/ai-portal.py --help
python3 python-client/ai-portal.py --cmd help

# For Humans
continuum --help
continuum --agents
```

## AI Agent Quick Start

The AI Portal provides a clean, thin client adapter for the Continuum command bus:

```bash
# Primary AI interface
python3 python-client/ai-portal.py --cmd [command] [--params '{}']

# Essential commands for AI agents
python3 python-client/ai-portal.py --cmd workspace     # Get workspace paths
python3 python-client/ai-portal.py --cmd sentinel      # Start monitoring/logging  
python3 python-client/ai-portal.py --cmd restart       # Version bump + server restart
python3 python-client/ai-portal.py --cmd help          # Live API documentation

# All commands are self-documenting
python3 python-client/ai-portal.py --cmd [command] --help
```

### Architecture Principles for AI Agents

- ✅ **No hardcoded paths** - Use workspace command for all directory management
- ✅ **No god objects** - Thin client adapter pattern, all logic in server commands  
- ✅ **Self-documenting** - Live help system provides current API documentation
- ✅ **Promise-based** - Clean async/await, no callback complexity
- ✅ **Modular** - Add functionality via Continuum commands, not client code

## Available Commands
### Core Commands
- **AGENTS** - Command available
- **BROWSER** - Command available
- **BROWSER_JS** - Command available
- **CHAT** - Command available
- **CLEAR** - Command available
- **CREATEROOM** - Command available
- **CURSOR** - Command available
- **DIAGNOSTICS** - Command available
- **DOCS** - Command available
- **EMOTION** - Command available
- **EXEC** - Command available
- **FILESAVE** - Command available
- **FINDUSER** - Command available
- **HELP** - Command available
- **INFO** - Command available
- **INPUT** - Command available
- **JOINROOM** - Command available
- **LISTAGENTS** - Command available
- **LISTROOMS** - Command available
- **LOADROOMS** - Command available
- **MACRO** - Command available
- **MOVE** - Command available
- **PREFERENCES** - Command available
- **PROMISE_JS** - Command available
- **RELOAD** - Command available
- **RESTART** - Command available
- **SAVE_FILE** - Command available
- **SCREENSHOT** - Command available
- **SENTINEL** - Command available
- **SHARE** - Command available
- **TYPE** - Command available
- **VALIDATE_CODE** - Command available
- **VALIDATE_JS** - Command available
- **WORKSPACE** - Command available

💡 **Get detailed help**: `python3 python-client/ai-portal.py --cmd [command] --help`

## Architecture

```
┌─────────────────────────────────────────┐
│           Continuum Server              │
│         (OS/Orchestrator)               │
│  ┌─────────────────────────────────────┐ │
│  │         Command Bus                 │ │
│  │  ┌─────┐ ┌─────┐ ┌─────┐ ┌─────┐  │ │
│  │  │work │ │restart│sentinel│help │  │ │ 
│  │  │space│ │     │ │     │ │     │  │ │
│  │  └─────┘ └─────┘ └─────┘ └─────┘  │ │
│  └─────────────────────────────────────┘ │
└─────────────────────────────────────────┘
         ↑                    ↑
   ┌─────────┐          ┌─────────┐
   │ Python  │          │Browser  │
   │ Client  │          │ Client  │
   │(AI Portal)│        │   (UI)  │
   └─────────┘          └─────────┘
```

### Design Patterns

- **Adapter Pattern**: Thin clients forward commands to server bus
- **Command Bus**: All business logic in modular server commands
- **Promise-Based**: Async/await patterns across all interfaces
- **Self-Documenting**: Help system generates live documentation
- **No God Objects**: Clean separation of concerns throughout

## Key Locations

| Location | Purpose |
|----------|---------|
| `python-client/ai-portal.py` | 🚀 Primary AI agent interface (thin client adapter) |
| `python-client/continuum_client/` | Promise-based Python API library |
| `src/commands/core/` | Modular command implementations |
| `src/integrations/WebSocketServer.cjs` | Command bus message routing |
| `.continuum/` | Workspace directory (managed by workspace command) |
| `.continuum/ai-portal/` | AI portal workspace and logs |
| `.continuum/sentinel/` | Sentinel monitoring and task logs |
| `docs/AI_PORTAL_ARCHITECTURE.md` | Detailed architecture documentation |

---
*Documentation auto-generated on 2025-06-18T01:56:42.380Z*  
*Source: Live help system via `help --sync` command*  
*Architecture: Command bus with thin client adapters*
