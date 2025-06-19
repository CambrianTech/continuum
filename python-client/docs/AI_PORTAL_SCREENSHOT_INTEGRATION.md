# AI Portal Screenshot Integration

## Overview

The AI Portal integrates with Continuum's DevTools daemon system to provide intelligent screenshot capture with automatic path routing. The daemon receives screenshot directory configuration from the Continuum API and respects the project's organizational preferences.

## How It Works

### 1. Continuum API Provides Configuration

When the Portal connects to Continuum, it receives screenshot directory settings from the API:

```javascript
// Continuum API response includes screenshot configuration
{
  "screenshot_dir": "/project/.continuum/screenshots",
  "target_url": "localhost:9000",
  "devtools_config": {
    "ports": [9222, 9223],
    "auto_heal": true
  }
}
```

### 2. Portal Passes Configuration to Daemon

The Portal automatically passes these settings when starting DevTools daemons:

```python
# Portal integration in ai-portal.py
async def start_devtools_daemon():
    # Get configuration from Continuum API
    config = await continuum_client.get_screenshot_config()
    
    # Start daemon with API-provided paths
    daemon_id = await start_devtools_daemon(
        target_url=config.target_url,
        screenshot_dir=config.screenshot_dir,  # From Continuum API
        ports=config.devtools_ports
    )
    
    return daemon_id
```

### 3. Daemon Respects Path Hierarchy

The DevTools daemon implements intelligent path resolution:

1. **API Settings** (highest priority): Uses screenshot directory from Continuum API
2. **Project Default**: Falls back to `.continuum/screenshots` if API unavailable
3. **Daemon Default**: Uses `.continuum/daemons/devtools/screenshots` as last resort

## Portal Commands

### Basic Screenshot Commands

```bash
# Start DevTools daemon with API-configured paths
python3 ai-portal.py --devtools

# Capture screenshot using Continuum's configured directory
python3 ai-portal.py --cmd screenshot --filename myshot.png

# List running daemons and their configurations
python3 ai-portal.py --daemons

# Check daemon logs for screenshot activity
python3 ai-portal.py --daemon-logs devtools-123456
```

### Advanced Usage

```bash
# Emergency recovery with path validation
python3 ai-portal.py --failsafe

# Check daemon status including screenshot directory
python3 ai-portal.py --daemon-status devtools-123456
```

## Directory Structure Examples

### Typical Continuum Project

```
project/
├── .continuum/
│   ├── screenshots/              # ← API configured directory
│   │   ├── portal_capture_001.png
│   │   ├── test_screenshot.png
│   │   └── automated_shot.jpeg
│   ├── daemons/
│   │   └── devtools/
│   │       ├── devtools-123456.log
│   │       └── screenshots/      # ← Fallback directory
│   └── ai-portal/
│       └── logs/
└── src/
```

### Custom Configuration

```
project/
├── assets/
│   └── screenshots/              # ← Custom API configured path
│       ├── ui_screenshot_001.png
│       └── feature_demo.png
├── .continuum/
│   └── daemons/
│       └── devtools/
│           └── devtools-789012.log
└── src/
```

## Integration Points

### 1. Portal Startup

- Portal connects to Continuum API
- Retrieves screenshot configuration
- Stores configuration for daemon creation

### 2. Daemon Creation

- Portal passes API configuration to daemon
- Daemon validates and creates screenshot directory
- Logs directory selection in structured format

### 3. Screenshot Capture

- Commands use daemon's configured directory
- Filenames automatically get proper extensions
- Paths logged for audit and debugging

## Configuration Flow

```mermaid
graph LR
    A[Continuum API] --> B[Portal Client]
    B --> C[DevTools Daemon]
    C --> D[Screenshot Directory]
    
    A -.-> E[screenshot_dir config]
    E -.-> B
    B -.-> F[start_devtools_daemon()]
    F -.-> C
    C -.-> G[Path Resolution]
    G -.-> D
```

## Logging and Monitoring

### Screenshot Activity Logs

```json
{
  "timestamp": "2025-06-19T02:02:35.123456",
  "daemon_id": "devtools-020235",
  "level": "SCREENSHOT_SUCCESS",
  "message": "Screenshot saved: /project/.continuum/screenshots/api_shot.png",
  "data": {
    "filename": "api_shot.png",
    "path": "/project/.continuum/screenshots/api_shot.png",
    "format": "png",
    "size_bytes": 328359,
    "source": "api_configured"
  }
}
```

### Directory Configuration Logs

```json
{
  "timestamp": "2025-06-19T02:02:30.123456",
  "daemon_id": "devtools-020235",
  "level": "SCREENSHOT_DIR",
  "message": "Using Continuum API screenshot directory: /project/.continuum/screenshots",
  "data": {
    "source": "continuum_api",
    "directory": "/project/.continuum/screenshots",
    "fallback_available": true
  }
}
```

## Best Practices

### For Developers

1. **Check Configuration**: Use `--daemon-status` to verify screenshot directory
2. **Monitor Logs**: Use `--daemon-logs` to track screenshot activity
3. **Validate Paths**: Ensure Continuum API provides correct screenshot directory

### For CI/CD

1. **Environment Setup**: Ensure `.continuum/screenshots` exists in test environments
2. **Permission Checks**: Verify write permissions on screenshot directories
3. **Cleanup**: Use daemon logs to track and cleanup test screenshots

### For API Integration

1. **Configuration Consistency**: Ensure API returns consistent screenshot directory
2. **Path Validation**: API should validate screenshot directory accessibility
3. **Fallback Planning**: Design graceful fallbacks for directory issues

## Troubleshooting

### Common Issues

1. **Permission Denied**
   ```bash
   # Check daemon logs
   python3 ai-portal.py --daemon-logs devtools-123456
   # Look for SCREENSHOT_ERROR with permission details
   ```

2. **Directory Not Found**
   ```bash
   # Check configuration
   python3 ai-portal.py --daemon-status devtools-123456
   # Verify screenshot_dir path exists
   ```

3. **API Configuration Missing**
   ```bash
   # Use failsafe mode
   python3 ai-portal.py --failsafe
   # Falls back to daemon default directory
   ```

### Recovery Commands

```bash
# Emergency recovery
python3 ai-portal.py --failsafe

# Restart with fresh configuration
python3 ai-portal.py --devtools

# Check all daemon configurations
python3 ai-portal.py --daemons
```

## Summary

The AI Portal's screenshot integration provides:

- **Intelligent Path Routing**: Respects Continuum API configuration
- **Automatic Fallbacks**: Graceful degradation when API unavailable
- **Structured Logging**: Complete audit trail of screenshot activity
- **Easy Monitoring**: Portal commands for daemon inspection
- **Configuration Flexibility**: Supports custom directories while maintaining defaults

This ensures screenshots are always saved to the correct location according to project preferences and API configuration.