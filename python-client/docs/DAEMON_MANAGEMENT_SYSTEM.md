# Daemon Management System

## Overview

The Continuum Daemon Management System provides an object-oriented framework for running and monitoring background services. Each daemon maintains its own structured logs and can be easily inspected through the Portal interface.

## Architecture

### Core Components

```
continuum_client/
├── core/
│   └── daemon_manager.py      # Base classes and manager
└── devtools/
    └── devtools_daemon.py     # DevTools-specific daemon
```

### Class Hierarchy

```python
BaseDaemon (Abstract)
├── DevToolsDaemon
├── MonitoringDaemon (future)
└── AutomationDaemon (future)
```

## BaseDaemon Class

All daemons inherit from `BaseDaemon` which provides:

- **Structured Logging**: JSON logs with timestamps and metadata
- **Signal Handling**: Graceful shutdown on SIGINT/SIGTERM
- **Status Tracking**: Runtime status and health monitoring
- **Memory Management**: In-memory log buffers for fast access

### Key Methods

```python
class BaseDaemon(ABC):
    def __init__(self, daemon_type: str, daemon_id: Optional[str] = None)
    def write_log(self, level: str, message: str, data: Optional[Dict] = None)
    def get_logs(self, lines: int = 50) -> List[Dict]
    def get_status(self) -> Dict[str, Any]
    
    @abstractmethod
    async def run(self):
        """Main daemon execution loop"""
        pass
```

## DevToolsDaemon

Object-oriented DevTools monitoring daemon that provides:

- **Browser Console Monitoring**: Real-time console log capture with millisecond latency
- **Connection Management**: Auto-reconnection and health checks with port healing
- **Screenshot Capability**: DevTools Protocol screenshot capture with intelligent path routing
- **Structured Logging**: Detailed connection and activity logs
- **Path Intelligence**: Automatically uses screenshot directories from Continuum API settings

### Example Usage

```python
from continuum_client.devtools.devtools_daemon import start_devtools_daemon

# Start a DevTools daemon with default Continuum screenshot routing
daemon_id = await start_devtools_daemon("localhost:9000", [9222, 9223])

# Start with custom screenshot directory
daemon_id = await start_devtools_daemon("localhost:9000", [9222, 9223], 
                                       screenshot_dir="/custom/screenshots")

# Capture screenshots with intelligent path routing
daemon = daemon_manager.active_daemons[daemon_id]
screenshot_path = await daemon.capture_screenshot("my_screenshot.png")
# Automatically saved to Continuum's configured screenshot directory
```

## DaemonManager

Central management system for all daemon instances:

```python
from continuum_client.core.daemon_manager import daemon_manager

# List all daemons
daemons = daemon_manager.list_daemons()

# Get daemon status
status = daemon_manager.get_daemon_status(daemon_id)

# Get daemon logs
logs = daemon_manager.get_daemon_logs(daemon_id, lines=50)
```

## Log Structure

Each daemon creates structured JSON logs:

```json
{
  "timestamp": "2025-06-19T01:30:45.123",
  "daemon_id": "devtools-143045",
  "daemon_type": "devtools",
  "level": "CONNECTION",
  "message": "DevTools connected successfully on port 9222",
  "data": {
    "port": 9222,
    "target_url": "localhost:9000",
    "attempt_number": 1
  }
}
```

## Portal Integration

### Command Reference

```bash
# List all running daemons
python3 ai-portal.py --daemons

# Show logs for specific daemon
python3 ai-portal.py --daemon-logs devtools-143045

# Show status for specific daemon  
python3 ai-portal.py --daemon-status devtools-143045

# Start DevTools daemon
python3 ai-portal.py --devtools

# Emergency failsafe recovery
python3 ai-portal.py --failsafe
```

### Example Output

```bash
$ python3 ai-portal.py --daemons
🤖 RUNNING DAEMONS:
  🤖 devtools-143045
     Type: devtools
     Status: 🟢 Running
     Uptime: 234.5s
     Logs: 45 entries

$ python3 ai-portal.py --daemon-status devtools-143045
📊 DAEMON STATUS: devtools-143045
  🤖 Daemon ID: devtools-143045
  📋 Type: devtools
  🔄 Running: True
  ⏱️  Uptime: 234.5s
  📝 Log File: /path/.continuum/daemons/devtools/devtools-143045.log
  💾 Memory Logs: 45
  🌐 Browser Connected: True
  📊 Logs Captured: 127
  🔗 Target URL: localhost:9000
```

## File System Structure

```
.continuum/
├── daemons/
│   ├── devtools/
│   │   ├── devtools-143045.log
│   │   └── devtools-144230.log
│   ├── monitoring/
│   │   └── monitor-145500.log
│   └── automation/
│       └── auto-150000.log
└── ai-portal/
    └── logs/
        └── buffer.log
```

## Log Levels

- **DAEMON_START**: Daemon initialization
- **CONNECTION**: DevTools connection events
- **BROWSER_CONSOLE**: Browser console logs
- **SCREENSHOT**: Screenshot operations
- **ERROR**: Error conditions
- **HEALTH_CHECK**: Periodic health status
- **SHUTDOWN**: Graceful shutdown

## Screenshot Path Routing

The DevTools daemon implements intelligent screenshot path routing that respects Continuum's configuration:

### Path Resolution Priority

1. **Custom Directory**: If explicitly provided, use the custom path
2. **Continuum API Settings**: Use screenshot directory from Continuum's API configuration  
3. **Continuum Default**: Use `.continuum/screenshots` if it exists
4. **Daemon Fallback**: Use `.continuum/daemons/devtools/screenshots` as last resort

### Example Directory Structures

```bash
# Continuum configured screenshots (preferred)
.continuum/screenshots/
├── continuum_screenshot_20250619_015842.png
├── test_default_routing.png
└── my_custom_screenshot.jpeg

# Custom directory routing
/custom/path/screenshots/
├── project_screenshot_001.png
└── test_custom_routing.png

# Daemon fallback directory
.continuum/daemons/devtools/screenshots/
└── fallback_screenshot.png
```

### API Integration

```python
# Portal automatically passes Continuum's screenshot directory
daemon_id = await start_devtools_daemon(
    target_url=continuum_api.target_url,
    screenshot_dir=continuum_api.screenshot_dir  # From Continuum settings
)

# Screenshot saved to Continuum's configured path
screenshot_path = await daemon.capture_screenshot("api_screenshot.png")
```

## Benefits

### For Development
- **Debugging**: Detailed logs for troubleshooting
- **Monitoring**: Real-time status of all background services
- **Recovery**: Easy identification and restart of failed services
- **Path Intelligence**: Screenshots automatically saved to correct directories

### For Operations
- **Reliability**: Auto-reconnection and health monitoring
- **Visibility**: Complete audit trail of daemon activity
- **Scalability**: Easy addition of new daemon types
- **Configuration Respect**: Honors Continuum API screenshot settings

### For AI Agents
- **State Awareness**: Always know what daemons are running
- **Log Inspection**: Examine any system's recent activity
- **Process Recovery**: Failsafe mechanisms for critical services
- **Automatic Organization**: Screenshots organized per Continuum's preferences

## Extending the System

### Creating New Daemon Types

1. **Inherit from BaseDaemon**:
```python
class MyDaemon(BaseDaemon):
    def __init__(self):
        super().__init__("mydaemon")
    
    async def run(self):
        # Implement daemon logic
        while self.running:
            self.write_log("ACTIVITY", "Doing work...")
            await asyncio.sleep(1)
```

2. **Register with DaemonManager**:
```python
daemon = MyDaemon()
daemon_id = await daemon_manager.start_daemon(daemon)
```

3. **Add Portal Integration**:
```python
# Add CLI options and handlers in ai-portal.py
@click.option('--mydaemon', is_flag=True)
def main(..., mydaemon):
    if mydaemon:
        return await start_mydaemon()
```

## Best Practices

1. **Structured Logging**: Always include relevant metadata in log data
2. **Health Checks**: Implement periodic health monitoring
3. **Graceful Shutdown**: Handle signals properly for clean termination
4. **Error Recovery**: Implement retry logic for transient failures
5. **Resource Management**: Clean up connections and resources on shutdown

## Troubleshooting

### Common Issues

1. **Daemon Not Starting**:
   - Check logs with `--daemon-logs daemon-id`
   - Verify permissions on `.continuum` directory
   - Check for port conflicts

2. **Connection Failures**:
   - DevTools daemons require browser with `--remote-debugging-port`
   - Check firewall settings
   - Verify target URLs are accessible

3. **Log File Issues**:
   - Ensure `.continuum` directory exists
   - Check disk space
   - Verify write permissions

### Recovery Commands

```bash
# Emergency recovery
python3 ai-portal.py --failsafe

# Check all daemon status
python3 ai-portal.py --daemons

# Restart specific daemon type
python3 ai-portal.py --devtools  # Starts new DevTools daemon
```

## Integration with CLAUDE.md Requirements

This system fulfills the CLAUDE.md requirement for **log monitoring as prerequisite for all development**:

- ✅ **Critical Feedback Mechanism**: Each daemon logs its activity
- ✅ **State Awareness**: Portal can inspect any system anytime
- ✅ **Recovery Capability**: Failsafe mechanisms for critical services
- ✅ **No Manual Interventions**: Automated daemon management
- ✅ **Modular Architecture**: Object-oriented, extensible design

The system ensures that **log monitoring is always available** as the foundation for development and debugging activities.