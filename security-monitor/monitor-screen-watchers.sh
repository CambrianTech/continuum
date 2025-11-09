#!/bin/bash
# Continuous Screen Watcher Monitor
# Runs every 30 seconds, detects NEW screen watchers, uses threat-profiles.json

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
LOG_FILE="$SCRIPT_DIR/screen-watchers.log"
STATE_FILE="$SCRIPT_DIR/.known-watchers"
THREAT_PROFILES="$SCRIPT_DIR/threat-profiles.json"
CONFIG_FILE="$SCRIPT_DIR/monitor-config.json"

# Load config
if [[ -f "$CONFIG_FILE" ]]; then
    CODE_WORD=$(cat "$CONFIG_FILE" | jq -r '.code_word // "System Update Available"')
    ALERT_MODE=$(cat "$CONFIG_FILE" | jq -r '.alert_mode // "silent"')
    NOTIFICATION_SOUND=$(cat "$CONFIG_FILE" | jq -r '.notification_sound // "Tink"')
    ALERT_ONCE=$(cat "$CONFIG_FILE" | jq -r '.alert_once // true')
    SCAN_INTERVAL=$(cat "$CONFIG_FILE" | jq -r '.scan_interval // 30')
    SHOW_POPUP=$(cat "$CONFIG_FILE" | jq -r '.show_popup // false')
else
    CODE_WORD="System Update Available"
    ALERT_MODE="silent"
    NOTIFICATION_SOUND="Tink"
    ALERT_ONCE=true
    SCAN_INTERVAL=30
    SHOW_POPUP=false
fi

# ============================================================================
# Load whitelists from threat-profiles.json
# ============================================================================

load_whitelists() {
    if [[ ! -f "$THREAT_PROFILES" ]]; then
        return 1
    fi

    # Extract trusted process patterns from config
    # (You can add a "trusted_processes" section to threat-profiles.json)
    TRUSTED_PATTERNS=(
        "continuum"
        "claude"
        "screencapture"
        "Screenshot"
        "WindowServer"
        "Dock"
        "SystemUIServer"
        "monitord"  # System daemons
        "OBS"
        "QuickTime"
    )
}

is_whitelisted() {
    local process="$1"

    for pattern in "${TRUSTED_PATTERNS[@]}"; do
        if [[ "$process" == *"$pattern"* ]]; then
            return 0
        fi
    done

    return 1
}

# ============================================================================
# Detection logic
# ============================================================================

detect_watchers() {
    # Use temp file instead of arrays (bash 3.2 compatible)
    local temp_file=$(mktemp)

    # METHOD 1: Known threat processes from threat-profiles.json
    if [[ -f "$THREAT_PROFILES" ]]; then
        cat "$THREAT_PROFILES" | jq -r '.profiles[].indicators.process_names[]?' 2>/dev/null | while IFS= read -r threat_name; do
            [[ -z "$threat_name" ]] && continue

            # Check if this threat process is running
            ps aux | grep -v grep | grep -i "$threat_name" | while read line; do
                PID=$(echo "$line" | awk '{print $2}')
                CMD=$(echo "$line" | awk '{print $11}')
                PROC_NAME=$(basename "$CMD")

                if is_whitelisted "$PROC_NAME"; then
                    continue
                fi

                echo "$PROC_NAME (PID $PID)" >> "$temp_file"
            done
        done
    fi

    # METHOD 2: Remote assist / screen share apps (explicit patterns)
    ps aux | grep -iE "jumpcloud|remote.*assist|vnc|teamviewer|anydesk" | grep -v grep | while read line; do
        PID=$(echo "$line" | awk '{print $2}')
        CMD=$(echo "$line" | awk '{print $11}')
        PROC_NAME=$(basename "$CMD")

        if is_whitelisted "$PROC_NAME"; then
            continue
        fi

        echo "$PROC_NAME (PID $PID)" >> "$temp_file"
    done

    # METHOD 3: TCC database - who has screen recording permission AND is running
    if [[ -f "$HOME/Library/Application Support/com.apple.TCC/TCC.db" ]]; then
        sqlite3 "$HOME/Library/Application Support/com.apple.TCC/TCC.db" \
            "SELECT client FROM access WHERE service='kTCCServiceScreenCapture' AND auth_value=2" 2>/dev/null | while IFS= read -r app; do
            [[ -z "$app" ]] && continue

            if is_whitelisted "$app"; then
                continue
            fi

            # Check if running
            if ps aux | grep -v grep | grep -i "$app" >/dev/null; then
                echo "$app (TCC permission)" >> "$temp_file"
            fi
        done
    fi

    # Remove duplicates and output
    if [[ -f "$temp_file" ]]; then
        sort -u "$temp_file"
        rm -f "$temp_file"
    fi
}

# ============================================================================
# Compare with known watchers
# ============================================================================

check_new_watchers() {
    local current_watchers=("$@")

    # Load known watchers (bash 3.2 compatible)
    local known_watchers=()
    if [[ -f "$STATE_FILE" ]]; then
        while IFS= read -r line; do
            [[ -z "$line" ]] && continue
            known_watchers+=("$line")
        done < "$STATE_FILE"
    fi

    # Find new watchers (not in known list)
    local new_watchers=()

    for watcher in "${current_watchers[@]}"; do
        local is_known=false

        for known in "${known_watchers[@]}"; do
            if [[ "$watcher" == "$known" ]]; then
                is_known=true
                break
            fi
        done

        if [[ "$is_known" == "false" ]]; then
            new_watchers+=("$watcher")
        fi
    done

    echo "${new_watchers[@]}"
}

# ============================================================================
# Alert user
# ============================================================================

alert_user() {
    local watcher_count=$1
    shift
    local new_watchers="$@"

    if [[ $watcher_count -eq 0 ]]; then
        return
    fi

    # Log the new watchers (always)
    echo "[$(date '+%Y-%m-%d %H:%M:%S')] 🚨 NEW DETECTION:" >> "$LOG_FILE"
    echo "$new_watchers" | while IFS= read -r watcher; do
        [[ -z "$watcher" ]] && continue
        echo "[$(date '+%Y-%m-%d %H:%M:%S')] 🔴 $watcher" >> "$LOG_FILE"
    done

    # Only alert if alert_once is false OR this is first detection
    if [[ "$ALERT_ONCE" == "true" ]] && [[ -f "$STATE_FILE" ]]; then
        # Already alerted, just log silently
        return
    fi

    # Generate HTML report first (always, so it's ready)
    REPORT_GENERATOR="$SCRIPT_DIR/generate-threat-report.sh"
    REPORT_FILE=""
    if [[ -x "$REPORT_GENERATOR" ]]; then
        REPORT_FILE=$("$REPORT_GENERATOR" "$watcher_count" "$new_watchers")
    fi

    # Use terminal-notifier for clickable notification that opens webpage
    if command -v terminal-notifier >/dev/null 2>&1; then
        # terminal-notifier allows clicking notification to open the report
        terminal-notifier \
            -title "$CODE_WORD" \
            -message "Check available" \
            -sound "$NOTIFICATION_SOUND" \
            -open "file://$REPORT_FILE" \
            >/dev/null 2>&1 &
    else
        # Fallback to osascript if terminal-notifier not available
        osascript -e "display notification \"Check available\" with title \"$CODE_WORD\" sound name \"$NOTIFICATION_SOUND\"" 2>/dev/null &

        # Auto-open if SHOW_POPUP is true
        if [[ "$SHOW_POPUP" == "true" ]] && [[ -n "$REPORT_FILE" ]]; then
            sleep 1
            open "$REPORT_FILE" &
        fi
    fi
}

# ============================================================================
# Main monitoring loop
# ============================================================================

log_event() {
    echo "[$(date '+%Y-%m-%d %H:%M:%S')] $1" >> "$LOG_FILE"
}

load_whitelists
log_event "Screen watcher monitor started (scan interval: ${SCAN_INTERVAL}s)"

echo "=========================================="
echo "Screen Watcher Monitor"
echo "=========================================="
echo "Monitoring for screen watchers..."
echo "Log: $LOG_FILE"
echo "Press Ctrl+C to stop"
echo ""

SCAN_COUNT=0

while true; do
    ((SCAN_COUNT++))

    echo -n "[Scan #$SCAN_COUNT $(date '+%H:%M:%S')] Checking... "

    # Detect current watchers (write to temp file)
    CURRENT_TEMP=$(mktemp)
    detect_watchers > "$CURRENT_TEMP"

    # Check for new watchers
    NEW_TEMP=$(mktemp)
    check_new_watchers $(cat "$CURRENT_TEMP") > "$NEW_TEMP"

    new_count=$(cat "$NEW_TEMP" | wc -l | tr -d ' ')
    current_count=$(cat "$CURRENT_TEMP" | wc -l | tr -d ' ')

    # Remove empty lines
    [[ -z "$(cat "$NEW_TEMP")" ]] && new_count=0
    [[ -z "$(cat "$CURRENT_TEMP")" ]] && current_count=0

    if [[ $new_count -gt 0 ]]; then
        echo "🔴 $new_count NEW WATCHER(S) DETECTED!"

        while IFS= read -r watcher; do
            echo "  🔴 $watcher"
        done < "$NEW_TEMP"

        alert_user $new_count $(cat "$NEW_TEMP")

        # Update known watchers file
        cp "$CURRENT_TEMP" "$STATE_FILE"

    elif [[ $current_count -gt 0 ]]; then
        echo "⚠️  $current_count known watcher(s) (no new)"
    else
        echo "✅ Clear"
    fi

    rm -f "$CURRENT_TEMP" "$NEW_TEMP"
    sleep $SCAN_INTERVAL
done
