# Graceful Shutdown & Port Conflict Resolution

This document describes the enhanced instance management features implemented in Continuum for production-ready deployment.

## Features

### 1. Automatic Port Conflict Resolution

Continuum automatically detects and resolves port conflicts when starting new instances:

- **Existing Instance Detection**: Checks for running instances using PID files
- **Graceful Replacement**: Sends SIGTERM to existing instances for graceful shutdown
- **Port Discovery**: If the target port is occupied by non-Continuum processes, automatically finds the next available port
- **Timeout Handling**: Falls back to SIGKILL if graceful shutdown takes too long

```bash
# Start first instance
continuum start --port 5555

# Start second instance (will replace first)
continuum start --port 5555  # Automatically shuts down first instance

# Start with port discovery
continuum start --port 5555  # Will use 5556 if 5555 is occupied by another process
```

### 2. Stay-Alive Mode

Prevent automatic replacement of existing instances:

```bash
# Start first instance
continuum start --port 5555

# Try to start second instance with stay-alive
continuum start --stay-alive --port 5555  # Exits gracefully, keeps first instance running
```

### 3. Graceful Shutdown

Comprehensive signal handling for clean shutdowns:

- **Signal Handlers**: SIGINT, SIGTERM, SIGHUP
- **Connection Cleanup**: Closes all WebSocket connections gracefully
- **Resource Cleanup**: Removes PID files and temporary resources
- **Error Handling**: Handles uncaught exceptions and unhandled rejections

```bash
# Manual shutdown via CLI
continuum stop

# Or send signals directly
kill -TERM <pid>
```

### 4. Process Management Commands

New CLI commands for instance management:

```bash
# Check if instance is running
continuum status

# Start with options
continuum start --port 8080 --stay-alive

# Graceful shutdown
continuum stop

# Restart (shutdown + start)
continuum restart
```

## Implementation Details

### PID File Management

- **Location**: `.continuum/continuum.pid` in the working directory
- **Format**: Simple text file containing the process ID
- **Cleanup**: Automatically removed on graceful shutdown
- **Stale Detection**: Detects and cleans up stale PID files from crashed processes

### Port Testing

```javascript
async testPort(port) {
  // Creates temporary server to test port availability
  // Returns true if port is free, false if occupied
}

async findFreePort(startPort) {
  // Searches for next available port starting from startPort
  // Tests up to 100 ports before giving up
}
```

### Graceful Shutdown Process

1. **Signal Reception**: Process receives SIGTERM/SIGINT
2. **Flag Setting**: Sets `isShuttingDown` to prevent duplicate shutdowns
3. **WebSocket Cleanup**: Closes all active WebSocket connections
4. **Server Cleanup**: Stops HTTP server from accepting new connections
5. **Resource Cleanup**: Removes PID files and temporary resources
6. **Exit**: Cleanly exits with status 0

### Instance Replacement Process

1. **PID Detection**: Reads existing PID file
2. **Process Verification**: Verifies the process is actually running
3. **Graceful Shutdown**: Sends SIGTERM to existing process
4. **Timeout Handling**: Waits up to 5 seconds for graceful shutdown
5. **Force Kill**: Uses SIGKILL if graceful shutdown fails
6. **Port Testing**: Verifies port is available after cleanup
7. **New Instance**: Starts new instance and writes new PID file

## Configuration

### Environment Variables

```bash
# Override default port
export CONTINUUM_PORT=8080

# Config file location
export CONTINUUM_CONFIG_DIR=/custom/path
```

### Command Line Options

```bash
continuum start [options]

Options:
  --port <number>     Custom port (default: 5555)
  --stay-alive        Don't replace existing instances
  --persist          Alias for --stay-alive
```

## Error Handling

### Common Scenarios

1. **Port Already in Use**: Automatically finds next available port
2. **Permission Denied**: Graceful error message with suggestions
3. **Stale PID File**: Automatically detects and cleans up
4. **Process Crash**: Next startup cleans up automatically
5. **Signal Handling**: All major signals handled gracefully

### Troubleshooting

```bash
# Check if instance is running
continuum status

# Force cleanup of stale files
rm -rf .continuum/

# Check port availability manually
netstat -an | grep 5555

# View logs for debugging
continuum start 2>&1 | tee continuum.log
```

## Testing

Use the provided test script to verify functionality:

```bash
# Run comprehensive tests
node test-graceful-shutdown.cjs

# Run interactive demo
node demo-graceful-shutdown.cjs
```

## Production Deployment

### Recommended Setup

1. **Process Manager**: Use PM2 or systemd for production deployment
2. **Health Checks**: Implement health check endpoints
3. **Logging**: Configure structured logging for monitoring
4. **Monitoring**: Set up alerts for process crashes or port conflicts

### PM2 Configuration

```javascript
// ecosystem.config.js
module.exports = {
  apps: [{
    name: 'continuum',
    script: 'continuum.cjs',
    args: 'start --port 5555',
    instances: 1,
    autorestart: true,
    watch: false,
    max_memory_restart: '1G',
    env: {
      NODE_ENV: 'production',
      CONTINUUM_PORT: 5555
    }
  }]
};
```

### Systemd Service

```ini
[Unit]
Description=Continuum AI Coordination Platform
After=network.target

[Service]
Type=simple
User=continuum
WorkingDirectory=/opt/continuum
ExecStart=/usr/bin/node continuum.cjs start
Restart=always
RestartSec=10
Environment=NODE_ENV=production
Environment=CONTINUUM_PORT=5555

[Install]
WantedBy=multi-user.target
```

## Security Considerations

- **PID File Permissions**: Ensure proper file permissions on PID files
- **Signal Security**: Only allow authorized users to send signals
- **Port Binding**: Bind to localhost in production unless external access needed
- **Resource Limits**: Set appropriate memory and CPU limits

## Backwards Compatibility

All existing functionality remains unchanged. New features are opt-in:

- Default behavior unchanged (still replaces existing instances)
- New `--stay-alive` flag provides new behavior
- Existing scripts and integrations continue to work
- No breaking changes to API or command structure