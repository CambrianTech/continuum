# Security Monitor System

A professional, real-time process monitoring system with beautiful dark mode UI.

## Overview

This system monitors for screen recording/remote access capabilities on macOS, with a focus on detecting MDM software like JumpCloud Remote Assist, VNC, TeamViewer, and similar tools.

**Key Features:**
- ✅ Real-time process detection (30s scan interval)
- ✅ Beautiful dark mode HTML reports with categorized processes
- ✅ Process categorization with visual indicators:
  - 🔴 Remote Access (screen sharing capabilities)
  - 🟡 Monitoring (endpoint security, agents)
  - ⚪ Support (logging, background services)
- ✅ Scrollable activity log (last 50 entries)
- ✅ Stealth configuration (code words, innocuous notifications)
- ✅ Whitelisting for legitimate processes
- ✅ Click-to-view notifications via terminal-notifier

## Files

- **monitor-screen-watchers.sh** - Main monitoring daemon (continuous)
- **generate-threat-report.sh** - HTML report generator with categorization
- **monitor-config.json** - Configuration (code word, alert mode, intervals)
- **threat-profiles.json** - Threat definitions (process names, indicators)
- **whitelist.conf** - Trusted processes (Claude, Continuum, etc.)
- **view-report.sh** - Manual report viewer

## Quick Start

```bash
# 1. Install terminal-notifier (for clickable notifications)
brew install terminal-notifier

# 2. Start the monitor
./monitor-screen-watchers.sh &

# 3. View the HTML report anytime
./view-report.sh

# 4. (Optional) Install transparent shell proxy for command logging
sudo ln -sf $(pwd)/transparent-shell-proxy.sh /usr/local/bin/safebash
echo "/usr/local/bin/safebash" | sudo tee -a /etc/shells
chsh -s /usr/local/bin/safebash
```

## Configuration

### monitor-config.json
```json
{
  "code_word": "System Update Available",  // Innocuous notification text
  "alert_mode": "silent",                  // Notification mode
  "notification_sound": "Tink",            // System sound
  "alert_once": true,                      // Only alert on first detection
  "scan_interval": 30,                     // Seconds between scans
  "show_popup": true,                      // Auto-open report
  "auto_close_popup_seconds": 5            // Popup duration
}
```

### threat-profiles.json
Define threat indicators:
```json
{
  "profiles": [
    {
      "id": "jumpcloud-mdm",
      "indicators": {
        "process_names": ["jumpcloud", "jumpcloud-agent", "jumpcloud-assist"],
        "process_paths": ["/opt/jc/bin/", "/Applications/JumpCloud"]
      },
      "response": {
        "filter_mode": "appear-normal",
        "log_level": "verbose",
        "alert_user": true
      }
    }
  ]
}
```

### whitelist.conf
Trusted processes that won't trigger alerts:
```bash
TRUSTED_PROCESSES=(
    "claude"
    "claude-code"
    "continuum"
    "Terminal"
    "screencapture"
    # ... add more as needed
)
```

## How It Works

### Detection Methods

1. **Process Name Matching**
   - Scans for known threat process names from threat-profiles.json
   - Matches patterns like "remote", "assist", "vnc", "teamviewer"

2. **TCC Database Query** (optional)
   - Checks macOS Transparency, Consent, and Control database
   - Identifies processes with screen recording permissions

3. **Pattern Matching**
   - Detects remote assist/screen share applications
   - Identifies endpoint security extensions

### Categorization Logic

Processes are automatically categorized:

| Pattern | Category | Icon | Description |
|---------|----------|------|-------------|
| assist, remote, vnc | Remote Access | 🔴 | Screen sharing capability |
| EndpointSecurity, agent | Monitoring | 🟡 | Security/MDM monitoring |
| log, durt, service | Support | ⚪ | Background support services |
| (unknown) | Unknown | ⚫ | Uncategorized |

### Alert Flow

```
[Scan] → [Detect] → [Compare to known] → [New process?]
                                             ↓
                              [Generate HTML Report] → [Notify User]
                                             ↓
                              [Update known watchers state]
```

## HTML Report Features

The generated `threat-report.html` includes:

- **Header**: Current status, timestamp, active badge
- **Stats Cards**: Active monitors, scan interval, auto-refresh
- **Process List**: Categorized with icons, process names, PIDs
- **Activity Log**: Scrollable log viewer (last 50 entries) with syntax highlighting
- **Auto-refresh**: Updates every 30 seconds
- **Dark Mode**: Professional black/gray theme

## Threat Assessment

### What We Currently Detect (JumpCloud Example)

**Capability Detection:**
- ✅ 9 JumpCloud processes running
- ✅ Remote Assist installed (PID 524, 855)
- ✅ Endpoint Security extension (PID 626 - kernel-level)
- ✅ MDM agent (PID 525 - can execute remote commands)

**Active Streaming Detection:**
- Network bandwidth analysis (high upload = active)
- External connection monitoring (localhost only = inactive)
- CPU usage of assist processes

**Current Status:**
- Capability: ✅ PRESENT (can remotely view screen)
- Active: ❌ INACTIVE (no external network connections)

## Log Files

- `screen-watchers.log` - All detection events with timestamps
- `.known-watchers` - State file tracking previously detected processes

## Advanced Usage

### Disable Alert Once
```json
{
  "alert_once": false  // Will alert every scan
}
```

### Custom Scan Interval
```json
{
  "scan_interval": 60  // Check every minute
}
```

### Add Custom Threat
```json
{
  "id": "custom-vnc",
  "indicators": {
    "process_names": ["myvnc", "customremote"]
  }
}
```

## Troubleshooting

### No notifications appearing
```bash
# Check if terminal-notifier is installed
which terminal-notifier

# Install if missing
brew install terminal-notifier
```

### Monitor not starting
```bash
# Check for bash 3.2 compatibility issues
bash --version

# View errors
tail -f screen-watchers.log
```

### Processes not detected
```bash
# Check threat-profiles.json syntax
cat threat-profiles.json | jq .

# Manually test detection
ps aux | grep -iE "jumpcloud|vnc|teamviewer"
```

## Transparent Shell Proxy (Optional)

**Problem**: Restricting shell breaks your workflow (no npm, git, etc.)

**Solution**: Transparent proxy that looks/acts like normal bash but logs JumpCloud activity.

### How It Works

```
JumpCloud connects → transparent-shell-proxy.sh
                     ├─ Detects JumpCloud (parent process check)
                     ├─ Logs command to /var/log/ares/shell-commands.log
                     ├─ Notifies Continuum (if running)
                     └─ Passes through to /bin/bash (100% transparent)

Your terminal → /bin/zsh (normal, no proxy)
```

### Installation

```bash
# Install
sudo ln -sf /path/to/transparent-shell-proxy.sh /usr/local/bin/safebash
echo "/usr/local/bin/safebash" | sudo tee -a /etc/shells
chsh -s /usr/local/bin/safebash

# Open new terminal - works exactly like normal bash
npm install  # ✅ Works
git status   # ✅ Works
./your-script.sh  # ✅ Works

# But JumpCloud commands are logged
tail -f /var/log/ares/shell-commands.log
```

### Features

- ✅ **Zero restrictions** - All commands work normally
- ✅ **Selective logging** - Only logs when JumpCloud detected
- ✅ **Real-time alerts** - Sends to Continuum via Unix socket
- ✅ **Fallback logging** - File-based logs if Continuum not running
- ✅ **Non-blocking** - Logging happens in background (fast)
- ✅ **JumpCloud oblivious** - Sees normal bash, no suspicion

### To Disable

```bash
# Switch back to zsh
chsh -s /bin/zsh
# Open new terminal
```

## Architecture

```
monitor-screen-watchers.sh
    ├── Load config (monitor-config.json)
    ├── Load whitelist (whitelist.conf)
    ├── Load threat profiles (threat-profiles.json)
    │
    ├── [Every 30s]
    │   ├── detect_watchers()
    │   │   ├── Method 1: threat-profiles.json patterns
    │   │   ├── Method 2: Explicit screen share patterns
    │   │   └── Method 3: TCC database query
    │   │
    │   ├── check_new_watchers()
    │   │   └── Compare with .known-watchers state
    │   │
    │   └── alert_user()
    │       ├── generate-threat-report.sh
    │       │   ├── Parse processes
    │       │   ├── Categorize with icons
    │       │   ├── Build HTML with logs
    │       │   └── Output: threat-report.html
    │       │
    │       └── terminal-notifier
    │           └── Clickable notification → opens HTML
    │
    └── Loop
```

## Security Considerations

### What This Monitors
- Process names and PIDs
- Screen recording capabilities
- Remote access tools

### What This Doesn't Do
- ❌ Does not block or kill processes
- ❌ Does not decrypt network traffic
- ❌ Does not interfere with system operations

### Privacy
- All data stays local (no external reporting)
- Logs stored in current directory
- No telemetry or analytics

## License

Created for security monitoring and defensive purposes. Use responsibly.

## Credits

Built with:
- Bash 3.2+ (macOS compatible)
- terminal-notifier (clickable notifications)
- jq (JSON parsing)
- SF Mono font (professional monospace)
