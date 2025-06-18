# Sentinel Command

## Definition
- **Name**: sentinel
- **Description**: AI guardian agent for logging and task management
- **Category**: Core
- **Icon**: 🛡️
- **Parameters**: `<action> [task] [options]`

## Overview
The Sentinel command provides comprehensive monitoring, logging, and task management for AI development workflows. Acts as a JTAG-style debugging interface for AI agents.

**🎯 THE VISION: Completely Autonomous Agent Code Generation**

Sentinel is the foundation for fully autonomous AI agent development with:
- **📊 Full Observability**: Complete logs of client, server, browser console activity
- **🛡️ Task-Based Monitoring**: Organized logging per debugging session  
- **🔄 Code Execution Tracking**: Monitor command execution with full context
- **📸 Visual Verification**: Screenshots captured automatically during testing
- **🤖 Future Autonomous Agents**: Sentinels will run as persistent AI bots executing Continuum JS scripts

**How This Enables Autonomous Development:**
1. **Agent picks ticket** from dashboard
2. **Starts sentinel** for organized logging
3. **Tests & debugs** with full log capture
4. **Reviews logs** to understand failures
5. **Documents findings** for next agent
6. **Repeats** until system is stable

Future: Sentinel bots will execute complex debugging scripts autonomously, sleep, check results, and continue without human intervention.

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

## Log Contents (Critical for Debugging)

**🔥 issues-*.log**: Most important for debugging
- JavaScript console errors and warnings
- Browser console.log statements  
- Runtime exceptions and stack traces
- Command execution failures

**🖥️ server-monitor-*.log**: Server-side execution  
- Command processing logs
- Server startup/shutdown events
- WebSocket connection events
- Backend error traces

**🌐 client-monitor-*.log**: Browser client activity
- Page load events
- DOM manipulation logs
- Client-side script execution
- Browser navigation events

**🛡️ sentinel-*.log**: Sentinel bot activity
- Task start/stop events
- Monitoring configuration
- Sentinel bot health status
- Future: Autonomous script execution logs

## Future: Autonomous Sentinel Agents
The sentinel system is designed to eventually run AI agents autonomously:
- **Deploy AI agents as persistent sentinels** running 24/7
- **Execute complex debugging scripts** written in Continuum JS
- **Sleep, wake, and check results** without human intervention
- **Command bus access** gives sentinels full system capabilities
- **Organized logging** helps AI agents track their autonomous work
- **Task-based isolation** prevents agent conflicts