# Info Command

## Definition
- **Name**: info
- **Description**: Display system information and server status
- **Category**: Core
- **Icon**: ℹ️
- **Status**: 🟡 TESTING (2025-06-18) - README-driven migration in progress, base class needs execute method fix
- **Parameters**: `[section]`

## Overview
The Info command provides comprehensive system information about the Continuum server, including version, uptime, memory usage, and active connections.

## Parameters
- `section`: Optional section to display (overview, system, connections, memory)

## Usage Examples
```bash
# Show all system information
python3 ai-portal.py --cmd info

# Show specific section
python3 ai-portal.py --cmd info --params '{"section": "system"}'

# Show memory usage
python3 ai-portal.py --cmd info --params '{"section": "memory"}'
```

## Package Rules
```json
{
  "timeouts": {"client": 5.0, "server": 3.0},
  "retries": {"client": 1, "server": 0},
  "behavior": {"client": "standard", "server": "info_reporter"},
  "concurrency": {"client": true, "server": true},
  "sideEffects": ["reads_system_info"]
}
```

## TODO:
- TODO: Test basic info display functionality
- TODO: Test section-specific info requests
- TODO: Verify memory usage reporting accuracy
- TODO: Test performance with multiple concurrent requests