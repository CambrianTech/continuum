# AI Portal Architecture

> 📖 See also: [Commands README](../src/commands/README.md) for package-based command architecture
> 🚀 Implementation: [AI Portal Code](../python-client/ai-portal.py)

## Overview

The AI Portal demonstrates Continuum's clean architectural separation between **thin client adapters** and a **robust command bus**. This design enables powerful automation workflows without bloated client-side logic.

## Architecture Principles

### 1. Continuum as OS/Orchestrator
```
┌─────────────────────────────────────────┐
│           Continuum Server              │
│  ┌─────────────────────────────────────┐ │
│  │         Command Bus                 │ │
│  │  ┌─────┐ ┌─────┐ ┌─────┐ ┌─────┐  │ │
│  │  │ ws  │ │restart│sentinel│help │  │ │ 
│  │  │pace │ │     │ │     │ │     │  │ │
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

### 2. Thin Client Adapter Pattern
```python
# ❌ BAD: Business logic in client
def restart_server():
    bump_version()     # Client knows how to version
    kill_process()     # Client manages processes  
    spawn_new()        # Client handles spawning

# ✅ GOOD: Adapter forwards to command bus
async def restart_server():
    return await client.send_command('restart', {})
```

### 3. Promise-Based API
```python
# Clean async/await pattern mirroring JavaScript
result = await client.send_command('sentinel', {
    'action': 'start',
    'task': 'debug-session'
})
```

## Command Discovery & Self-Documentation

Commands are **discoverable** and **self-documenting**:

```bash
# Live API documentation
python3 ai-portal.py --cmd help

# Command-specific help  
python3 ai-portal.py --cmd workspace --help
python3 ai-portal.py --cmd sentinel --help
```

## Key Components

### 1. Workspace Command
**Purpose**: Replaces hardcoded paths with configurable workspace management

```bash
# Get workspace path
python3 ai-portal.py --cmd workspace --params '{"workspace": "ai-portal"}'

# Create new workspace  
python3 ai-portal.py --cmd workspace --params '{"action": "create", "workspace": "my-project"}'

# List all workspaces
python3 ai-portal.py --cmd workspace --params '{"action": "list"}'
```

**Architecture**: 
- No hardcoded `get_portal_dir()` in Python
- Configurable paths managed by Continuum command
- Consistent with modular design

### 2. Sentinel Command  
**Purpose**: AI guardian agent for logging and task management

```bash
# Start monitoring
python3 ai-portal.py --cmd sentinel --params '{"action": "start", "task": "debug"}'

# Check logs
python3 ai-portal.py --cmd sentinel --params '{"action": "logs", "lines": 20}'

# Get status
python3 ai-portal.py --cmd sentinel --params '{"action": "status"}'

# Get paths for integration
python3 ai-portal.py --cmd sentinel --params '{"action": "path", "task": "debug"}'
```

**Architecture**:
- Organized logging in `.continuum/sentinel/task_name/`
- Separate log files: `sentinel-*.log`, `client-monitor-*.log`, `server-monitor-*.log`, `issues-*.log`
- Future: Can be enhanced with AI agent capabilities

### 3. Restart Command
**Purpose**: Version bump + clean server restart

```bash
# Full restart with version bump
python3 ai-portal.py --cmd restart

# Restart without version bump  
python3 ai-portal.py --cmd restart --params '{"bump": false}'
```

**Architecture**:
- Client doesn't know how to restart - just forwards command
- Server handles: version bump, process management, spawning
- Clean timeout expected as server process restarts

## Message Protocol

### Client → Server
```json
{
    "type": "task",
    "role": "system", 
    "task": "[CMD:WORKSPACE] {\"action\": \"path\"}",
    "commandId": "workspace_123456789"
}
```

### Server → Client  
```json
{
    "type": "bus_command_execution",
    "role": "BusCommand",
    "result": {
        "command": "WORKSPACE", 
        "result": {
            "success": true,
            "data": { "path": "/path/to/workspace" }
        }
    }
}
```

## Automation Workflows

### Example: Continuous Monitoring
```bash
# Start sentinel monitoring
python3 ai-portal.py --cmd sentinel --params '{"action": "start", "task": "ci-monitor"}'

# Check for issues every 30 seconds
python3 ai-portal.py --program 'cmd:sentinel,sleep:30,cmd:sentinel'
```

### Example: Development Cycle
```bash  
# Restart server + take screenshot + check logs
python3 ai-portal.py --program 'cmd:restart,sleep:5,cmd:screenshot,cmd:sentinel'
```

### Example: Workspace Setup
```bash
# Create workspace + start monitoring + get path
python3 ai-portal.py --program 'cmd:workspace --params {"action":"create","workspace":"new-project"},cmd:sentinel --params {"action":"start","task":"new-project"},cmd:workspace --params {"action":"path","workspace":"new-project"}'
```

## Future: AI Agent Integration

The **sentinel command** is designed to eventually run **AI agents**:

```bash
# Future: Deploy AI agent as sentinel
python3 ai-portal.py --cmd sentinel --params '{
    "action": "deploy", 
    "agent": "claude", 
    "task": "code-review",
    "config": {"repo": "continuum", "branch": "main"}
}'
```

**Architecture for AI Agents**:
- Sentinel provides the **monitoring infrastructure**
- AI agents can be **deployed as sentinels** 
- **Organized logging** helps AI agents track their work
- **Command bus access** gives AI agents full Continuum capabilities
- **Workspace management** provides clean working directories

## Benefits

### ✅ No God Objects
- Client has no business logic
- All functionality in modular commands
- Easy to extend without changing client

### ✅ Not Flaky  
- Promise-based async handling
- Proper timeout management
- Clean error handling

### ✅ Self-Documenting
- Live help system
- Commands describe themselves
- Examples included in definitions

### ✅ Modular
- Add functionality by creating commands
- Commands are discoverable
- Consistent patterns

### ✅ AI-Ready
- Sentinel pattern for AI deployment
- Organized logging for AI context
- Command bus gives AI full system access

## Development Process

1. **Need new functionality?** → Create a Continuum command
2. **Need client access?** → Use existing `ai-portal.py` 
3. **Need AI automation?** → Deploy as sentinel
4. **Need help?** → `--cmd help` provides live docs

This architecture scales from simple scripts to complex AI agent deployments while maintaining clean separation of concerns.