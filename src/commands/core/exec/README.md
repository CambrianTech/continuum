# Exec Command

## Definition
- **Name**: exec
- **Description**: Execute shell commands on server
- **Category**: Core
- **Icon**: ⚡
- **Status**: 🟢 STABLE (Last verified: 2025-06-18, Auto-tested: Daily)
- **Parameters**: `<command_and_args>`

## Overview
The Exec command provides powerful shell command execution on the server. Use with caution as it has full system access and can modify files, install packages, and run any system command.

## Parameters
- `command_and_args`: Complete shell command with arguments to execute

## Usage Examples
```bash
# Basic commands
python3 ai-portal.py --cmd exec --params '"ls -la"'
python3 ai-portal.py --cmd exec --params '"ps aux | grep node"'

# Git operations
python3 ai-portal.py --cmd exec --params '"git status"'
python3 ai-portal.py --cmd exec --params '"git log --oneline -5"'

# Development commands
python3 ai-portal.py --cmd exec --params '"npm test"'
python3 ai-portal.py --cmd exec --params '"python3 -m pytest tests/"'

# System monitoring
python3 ai-portal.py --cmd exec --params '"df -h"'
python3 ai-portal.py --cmd exec --params '"top -l 1 | head -10"'
```

## Package Rules
```json
{
  "timeouts": {"client": 60.0, "server": 30.0},
  "retries": {"client": 1, "server": 0},
  "concurrency": {"client": false, "server": false},
  "sideEffects": ["system_commands", "file_system", "process_spawning"],
  "security": "HIGH_RISK"
}
```

## Architecture
- **Full system access**: Can execute any shell command
- **Process spawning**: Creates child processes for command execution
- **Stream output**: Returns command output and error streams
- **Exit codes**: Properly handles command success/failure status

## Security Considerations
- ⚠️ **HIGH RISK**: Full system access - use responsibly
- ⚠️ **No sandboxing**: Commands run with server privileges
- ⚠️ **Data exposure**: Command output may contain sensitive information
- ⚠️ **System modification**: Can install packages, modify files, change system state

## Status History
- 🟢 **2025-06-18**: STABLE - Command working reliably in production
- 🟡 **2025-06-17**: TESTING - Migrated to README-driven system
- 🟢 **2025-06-16**: STABLE - Core functionality verified
- 🟢 **2025-06-15**: STABLE - Security review completed

## TODO: Future Improvements
- TODO: Add command sandboxing for security
- TODO: Implement command whitelist/blacklist
- TODO: Add execution time limits per command type
- TODO: Create audit logging for all executed commands
- TODO: Add interactive command support (stdin)

## Test Status
- ✅ **Basic commands**: ls, ps, pwd - PASSING
- ✅ **Git operations**: status, log, diff - PASSING  
- ✅ **NPM commands**: test, install, build - PASSING
- ✅ **Python scripts**: pytest, python3 execution - PASSING
- ⚠️ **Long-running commands**: Needs timeout testing
- ❌ **Interactive commands**: Not supported yet