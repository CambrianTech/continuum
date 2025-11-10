#!/bin/bash
# Continuum Bluetooth Protection - Easy Installer
# User can opt-in or opt-out anytime

FEATURE_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
LOG_DIR="$HOME/.continuum/bluetooth-protection"
LAUNCH_AGENT="$HOME/Library/LaunchAgents/com.continuum.bluetooth-protection.plist"

echo "══════════════════════════════════════════════════════"
echo "  Continuum Bluetooth Protection"
echo "══════════════════════════════════════════════════════"
echo ""
echo "Protects you from suspicious Bluetooth pairing attacks"
echo "by monitoring and alerting on unknown devices."
echo ""

# Check if already installed
if [[ -f "$LAUNCH_AGENT" ]]; then
    echo "Status: Currently INSTALLED"
    echo ""
    echo "What would you like to do?"
    echo "  1) Keep running (do nothing)"
    echo "  2) Uninstall (remove protection)"
    echo "  3) View logs"
    echo ""
    read -p "Choice (1-3): " choice
    
    case $choice in
        2)
            echo ""
            echo "Uninstalling Bluetooth Protection..."
            launchctl unload "$LAUNCH_AGENT" 2>/dev/null
            rm "$LAUNCH_AGENT"
            echo "✓ Uninstalled"
            echo ""
            echo "Your logs are preserved at: $LOG_DIR"
            echo "Delete manually if desired: rm -rf $LOG_DIR"
            exit 0
            ;;
        3)
            echo ""
            if [[ -f "$LOG_DIR/bluetooth-monitor.log" ]]; then
                tail -50 "$LOG_DIR/bluetooth-monitor.log"
            else
                echo "No logs found yet."
            fi
            exit 0
            ;;
        *)
            echo "No changes made."
            exit 0
            ;;
    esac
fi

# New installation
echo "This feature will:"
echo "  ✓ Monitor Bluetooth pairing requests"
echo "  ✓ Alert you to suspicious devices"
echo "  ✓ Remember device history"
echo "  ✓ Capture evidence automatically"
echo ""
echo "Privacy:"
echo "  ✓ 100% local (no cloud, no internet)"
echo "  ✓ Data stays on your Mac"
echo "  ✓ You control everything"
echo ""
echo "Performance:"
echo "  ✓ Minimal CPU usage"
echo "  ✓ Runs silently in background"
echo "  ✓ No impact on daily use"
echo ""
read -p "Install Bluetooth Protection? (y/n): " install

if [[ "$install" != "y" ]]; then
    echo "Installation cancelled."
    exit 0
fi

# Create log directory
mkdir -p "$LOG_DIR"

# Create LaunchAgent
cat > "$LAUNCH_AGENT" << PLIST_EOF
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" 
          "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
    <key>Label</key>
    <string>com.continuum.bluetooth-protection</string>
    <key>ProgramArguments</key>
    <array>
        <string>$FEATURE_DIR/bluetooth-monitor.sh</string>
    </array>
    <key>RunAtLoad</key>
    <true/>
    <key>KeepAlive</key>
    <true/>
    <key>StandardOutPath</key>
    <string>$LOG_DIR/bluetooth-monitor.log</string>
    <key>StandardErrorPath</key>
    <string>$LOG_DIR/bluetooth-monitor-error.log</string>
    <key>WorkingDirectory</key>
    <string>$LOG_DIR</string>
</dict>
</plist>
PLIST_EOF

# Load LaunchAgent
launchctl load "$LAUNCH_AGENT"

echo ""
echo "══════════════════════════════════════════════════════"
echo "  ✓ Bluetooth Protection INSTALLED"
echo "══════════════════════════════════════════════════════"
echo ""
echo "Status:"
echo "  • Running in background"
echo "  • Monitoring all Bluetooth activity"
echo "  • Will alert on suspicious devices"
echo ""
echo "Logs & Evidence:"
echo "  $LOG_DIR/"
echo ""
echo "View activity:"
echo "  tail -f $LOG_DIR/bluetooth-monitor.log"
echo ""
echo "To uninstall later:"
echo "  bash $FEATURE_DIR/install.sh"
echo ""
echo "You're protected! 🛡️"
echo ""

