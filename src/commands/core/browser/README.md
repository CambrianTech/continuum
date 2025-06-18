# Browser Command

## Definition
- **Name**: browser
- **Description**: Launch and control browser instances for automation
- **Category**: Core
- **Icon**: 🌐
- **Status**: 🟠 UNTESTED (2025-06-18) - Needs documentation and testing
- **Parameters**: `[action] [url] [options]`

## Overview
The Browser command manages browser instances for web automation, testing, and interaction. It can launch browsers, navigate to URLs, and manage browser state.

## Parameters
- `action`: Action to perform (launch, navigate, close, status)
- `url`: URL to navigate to (for navigate action)
- `options`: Browser options (headless, size, timeout)

## Usage Examples
```bash
# Launch browser
python3 ai-portal.py --cmd browser --params '{"action": "launch"}'

# Navigate to URL
python3 ai-portal.py --cmd browser --params '{"action": "navigate", "url": "https://example.com"}'

# Get browser status
python3 ai-portal.py --cmd browser --params '{"action": "status"}'

# Launch headless browser
python3 ai-portal.py --cmd browser --params '{"action": "launch", "options": {"headless": true}}'
```

## Package Rules
```json
{
  "timeouts": {"client": 15.0, "server": 12.0},
  "retries": {"client": 2, "server": 1},
  "behavior": {"client": "standard", "server": "browser_manager"},
  "concurrency": {"client": true, "server": false},
  "sideEffects": ["launches_browser", "modifies_system_state"]
}
```

## TODO:
- TODO: Test browser launch functionality
- TODO: Test URL navigation and page loading
- TODO: Test headless vs headed browser modes
- TODO: Test browser state management
- TODO: Verify proper browser cleanup on exit