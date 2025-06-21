# Continuum - Living Intelligence Ecosystem
> 🧬 **A place where code becomes art and intelligence becomes architecture**  
> 🎨 **Sophisticated capabilities through elegant organization**  
> 🚀 **NASA-level reliability with human-admirable beauty**

## Vision: Code as Living Architecture

Continuum is an **ecosystem of intelligences** that creates code so elegant it's simultaneously sophisticated and beautiful. Code that makes humans think *"I wish I could write like this"* while being advanced enough to power autonomous space missions.

### Core Philosophy

- **🧬 Living Intelligence Hierarchy**: Nano to meta-level AIs continuously improving the codebase
- **🎨 Code as Art**: Every module is both functional excellence and aesthetic beauty  
- **🤖 AI Craftsmanship**: Specialized AI personas write code humans admire and understand
- **🔬 Collaborative Verification**: NASA-level testing through AI teams checking each other's work
- **📚 Educational by Design**: Every component teaches best practices naturally
- **🌱 Organic Growth**: New capabilities emerge elegantly without architectural debt

## The Continuum Experience

**For Humans**: Code that's immediately comprehensible, visually pleasing, and architecturally admirable  
**For AIs**: Logical patterns, consistent abstractions, and predictable evolution paths  
**For Systems**: Self-testing, self-healing, self-documenting autonomous operation

---

## Technical Architecture

Continuum implements **containerization principles at the module level** - the same isolation, modularity, and orchestration patterns that made Docker/Kubernetes revolutionary:

- **🏗️ Command Bus Architecture**: Central orchestration with modular commands
- **🤖 AI Portal**: Primary interface for AI agents (`python-client/ai-portal.py`)
- **📡 Promise-based API**: Clean async/await patterns across all clients
- **🛡️ Sentinel System**: Monitoring and logging for AI task management  
- **📁 Workspace Management**: No hardcoded paths, configurable workspaces
- **📚 Self-documenting**: Live help system keeps docs in sync

### Quick Start

```bash
# FIRST: Install git hooks (required for all developers)
bash scripts/install-git-hooks.sh

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
- **help** 📚 - Show help information and sync documentation
- **workspace** 📁 - Manage workspace directories and paths
- **sentinel** 🛡️ - AI guardian for logging and task management
- **restart** 🔄 - Restart server with version bump
- **info** ℹ️ - System information and status

💡 **Get live commands**: `python3 python-client/ai-portal.py --cmd help`

## Command Status Dashboard
> 📊 Built-in project management - tracks command health and TODOs

| Status | Command | Icon | TODOs | Tests | Last Updated | Notes |
|--------|---------|------|-------|-------|--------------|-------|
| 🔴 | reload | 🔄 | 4 | No tests | 2025-06-18 | CRITICAL ISSUES |
| 🟠 | continuon_status | 📄 | ? | No tests | Never | No documentation |
| 🟠 | emotion | 📄 | ? | No tests | Never | No documentation |
| 🟠 | event | 📄 | ? | No tests | Never | No documentation |
| 🟠 | markread | 📄 | ? | No tests | Never | No documentation |
| 🟠 | notify | 📄 | ? | No tests | Never | No documentation |
| 🟠 | preferences | 📄 | ? | No tests | Never | No documentation |
| 🟠 | status_text | 📄 | ? | No tests | Never | No documentation |
| 🟠 | transfer | 📄 | ? | No tests | Never | No documentation |
| 🟡 | info | ℹ️ | 5 | No tests | 2025-06-18 | In migration |
| ⚪ | help | 📚 | 0 | No tests | Unknown | Needs review |
| ⚪ | restart | 🔄 | 0 | No tests | Unknown | Needs review |
| ⚪ | workspace | 📁 | 0 | No tests | Unknown | Needs review |

### Project Health Summary

| Status | Count | Description |
|--------|-------|-------------|
| 🔴 | 1 | Broken - Do not use |
| 🟠 | 8 | Untested - Needs documentation |
| 🟡 | 1 | Testing - In progress |
| 🟢 | 0 | Stable - Production ready |
| ⚪ | 3 | Unknown - Needs assessment |

**Project Health: 0% stable (0/13 commands)**

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
*Documentation auto-generated on 2025-06-20T19:39:38.265Z*  
*Source: Live help system via `help --sync` command*  
*Architecture: Command bus with thin client adapters*
