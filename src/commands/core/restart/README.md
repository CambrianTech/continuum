# Restart Command

## Definition
- **Name**: restart
- **Description**: Server restart with version management
- **Category**: Core
- **Icon**: 🔄
- **Parameters**: `[options]`

## Overview
Handles clean server restart with automatic version bumping and process management. Designed for development workflows where version tracking is essential.

## Parameters
- `bump`: Whether to bump version (default: true)
- `delay`: Delay before restart in seconds (default: 2)

## Usage Examples
```bash
# Full restart with version bump
python3 ai-portal.py --cmd restart

# Restart without version bump
python3 ai-portal.py --cmd restart --params '{"bump": false}'

# Restart with custom delay
python3 ai-portal.py --cmd restart --params '{"delay": 5}'
```

## Package Rules
```json
{
  "timeouts": {"client": 70.0, "server": 30.0},
  "retries": {"client": 1, "server": 0},
  "behavior": {"client": "wait_and_auto_heal", "server": "kill_self_after_response"},
  "concurrency": {"client": false, "server": false},
  "sideEffects": ["version_bump", "process_restart", "file_system"]
}
```

## Architecture
- **Expected timeout**: Client expects server to timeout as process restarts
- **Auto-healing integration**: Client auto-healing handles expected disconnection
- **Version management**: Automatically increments build number in package.json
- **Clean shutdown**: Server responds then terminates itself cleanly

## Behavior Pattern
1. **Server**: Bumps version, responds to client, then kills process
2. **Client**: Waits up to 70s, expects timeout around 30s
3. **Auto-healing**: Client detects expected timeout and starts new server
4. **Result**: Clean restart with version tracking

This command demonstrates the dual-side timeout pattern where client and server have different timeout expectations based on the command's specific behavior.