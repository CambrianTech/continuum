# Bluetooth Monitor - Real-time Pairing Attack Detection

## Quick Start

Run the monitor in background:
```bash
# From scripts directory
./bluetooth-monitor.sh &

# Or run in a dedicated Terminal window:
./bluetooth-monitor.sh
```

## What It Does

**Real-time monitoring:**
- Watches all Bluetooth connection attempts
- Identifies unknown/suspicious devices
- Auto-captures full system state
- Creates incident reports
- Sends macOS notifications

**When C08MRSEM2330 (your attacker) tries to connect:**
- 🔴 Immediate alert notification
- Full log capture
- Incident report created
- All evidence preserved

## Logs and Evidence

**Main log:** `bluetooth-monitor.log`
**Evidence directory:** `bluetooth-evidence/`
  - `bluetooth-state-TIMESTAMP.txt` - Full system state
  - `incident-DEVICEID-TIMESTAMP.md` - Incident reports

## Customize Known Devices

Edit `bluetooth-monitor.sh` line ~25:
```bash
KNOWN_DEVICES=(
    "C8:69:CD:59:EF:C4"  # Basement (2)
    "78:2B:64:9F:42:72"  # Ed phones
    # Add your devices here
)
```

## Run on Startup

Make it auto-start when you log in:

### Option 1: Terminal at Login
1. System Settings → General → Login Items
2. Add Terminal.app
3. Configure Terminal to run: `cd /Volumes/.../scripts && ./bluetooth-monitor.sh`

### Option 2: LaunchAgent (More reliable)
```bash
# Create launch agent
cat > ~/Library/LaunchAgents/com.user.bluetooth-monitor.plist << 'PLIST'
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" 
          "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
    <key>Label</key>
    <string>com.user.bluetooth-monitor</string>
    <key>ProgramArguments</key>
    <array>
        <string>/Volumes/FlashGordon/scripts/bluetooth-monitor.sh</string>
    </array>
    <key>RunAtLoad</key>
    <true/>
    <key>KeepAlive</key>
    <true/>
    <key>StandardOutPath</key>
    <string>/tmp/bluetooth-monitor.out</string>
    <key>StandardErrorPath</key>
    <string>/tmp/bluetooth-monitor.err</string>
</dict>
</plist>
PLIST

# Load it
launchctl load ~/Library/LaunchAgents/com.user.bluetooth-monitor.plist
```

## Check If Running

```bash
# Check process
ps aux | grep bluetooth-monitor

# Check recent logs
tail -20 bluetooth-monitor.log

# List captured incidents
ls -lt bluetooth-evidence/
```

## Stop Monitoring

```bash
# If running in foreground: Ctrl+C

# If running in background:
pkill -f bluetooth-monitor.sh

# If LaunchAgent:
launchctl unload ~/Library/LaunchAgents/com.user.bluetooth-monitor.plist
```

## What You'll See

**Normal (known device):**
```
[2025-11-08 22:30:15] Known device activity: F4:21:CA:BF:6E:17
```

**⚠️ Unknown device:**
```
[2025-11-08 22:30:45] ⚠️  UNKNOWN DEVICE DETECTED: AA:BB:CC:DD:EE:FF
[2025-11-08 22:30:45] State captured: bluetooth-state-1762662045.txt
[2025-11-08 22:30:45] Incident report: incident-AA:BB:CC:DD:EE:FF-1762662045.md
```

**🔴 C08MRSEM2330 (your attacker):**
```
[2025-11-08 22:31:00] 🔴 ATTACKER DEVICE C08MRSEM2330 DETECTED
[2025-11-08 22:31:00] State captured: bluetooth-state-1762662060.txt
```

Plus macOS notification: "🚨 KNOWN ATTACKER - C08MRSEM2330 is trying to connect!"

## Evidence for Investigation

Each incident captures:
- Exact timestamp
- Device identifier
- Full Bluetooth device list
- Recent Bluetooth system logs
- Current pairing state
- Markdown incident report

Perfect for:
- Security investigation
- Legal evidence
- Pattern analysis
- Attacker profiling

