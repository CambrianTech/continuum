# Sentinel Command

## Definition
- **Name**: sentinel
- **Description**: AI guardian agent for logging and task management
- **Category**: Core
- **Icon**: 🛡️
- **Parameters**: `<action> [task] [options]`

## Overview
The Sentinel command provides comprehensive monitoring, logging, and task management for AI development workflows. Acts as a JTAG-style debugging interface for AI agents.

## Parameters
- `action`: Action to perform (start, stop, status, logs, path)
- `task`: Task name for organized logging (optional)
- `options`: Additional configuration (optional)

## Usage Examples
```bash
# Start monitoring with task name
python3 ai-portal.py --cmd sentinel --params '{"action": "start", "task": "debug-session"}'

# Check sentinel status
python3 ai-portal.py --cmd sentinel --params '{"action": "status"}'

# Get recent logs
python3 ai-portal.py --cmd sentinel --params '{"action": "logs", "lines": 20}'

# Get task directory path
python3 ai-portal.py --cmd sentinel --params '{"action": "path", "task": "debug-session"}'

# Stop monitoring
python3 ai-portal.py --cmd sentinel --params '{"action": "stop", "task": "debug-session"}'
```

## Package Rules
```json
{
  "timeouts": {"client": 45.0, "server": 60.0},
  "retries": {"client": 2, "server": 1},
  "behavior": {"client": "persistent_connection", "server": "background_task"},
  "resources": {"server": ["log_files", "process_monitoring", "file_system"]},
  "concurrency": {"client": false, "server": true}
}
```

## Architecture
- **Organized logging**: Each task gets its own directory in `.continuum/sentinel/task_name/`
- **Multiple log types**: `sentinel-*.log`, `client-monitor-*.log`, `server-monitor-*.log`, `issues-*.log`
- **AI agent ready**: Designed for future AI agent deployment as sentinels
- **Command bus access**: Full access to Continuum capabilities for monitoring

## Directory Structure
```
.continuum/sentinel/
├── task_name/
│   ├── sentinel-YYYYMMDD.log      # Sentinel activity logs
│   ├── client-monitor-YYYYMMDD.log # Client monitoring
│   ├── server-monitor-YYYYMMDD.log # Server monitoring  
│   └── issues-YYYYMMDD.log         # Issues and errors
```

## Future: AI Agent Integration
The sentinel system is designed to eventually run AI agents:
- Deploy AI agents as persistent sentinels
- Organized logging helps AI agents track their work
- Command bus access gives AI agents full system capabilities
- Workspace integration provides clean working directories