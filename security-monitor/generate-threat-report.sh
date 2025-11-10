#!/bin/bash
# Generate HTML threat report

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
LOG_FILE="$SCRIPT_DIR/screen-watchers.log"
REPORT_FILE="$SCRIPT_DIR/threat-report.html"

# Get parameters
WATCHER_COUNT=$1
shift
WATCHERS="$@"

# Parse watchers into individual entries
WATCHER_ENTRIES=()
if [[ -n "$WATCHERS" ]]; then
    # Use a temp file to handle the parsing
    echo "$WATCHERS" > /tmp/watchers_raw.txt

    # Extract each "ProcessName (PID 123)" entry
    while read -r line; do
        # Split by closing parens followed by space
        IFS=')' read -ra PARTS <<< "$line"
        for part in "${PARTS[@]}"; do
            if [[ "$part" =~ ([^[:space:]]+)[[:space:]]+\(PID[[:space:]]+([0-9]+) ]]; then
                PROCESS_NAME="${BASH_REMATCH[1]}"
                PID="${BASH_REMATCH[2]}"
                WATCHER_ENTRIES+=("$PROCESS_NAME|$PID")
            fi
        done
    done < /tmp/watchers_raw.txt
    rm -f /tmp/watchers_raw.txt
fi

# Categorize processes and add icons
categorize_process() {
    local name="$1"

    # Remote Access / Screen Sharing
    if [[ "$name" =~ (assist|remote|vnc|teamviewer|anydesk) ]]; then
        echo "🔴|Remote Access"
    # Endpoint Security / Monitoring
    elif [[ "$name" =~ (EndpointSecurity|agent|helper|security) ]]; then
        echo "🟡|Monitoring"
    # Logging / Support
    elif [[ "$name" =~ (log|durt|service) ]]; then
        echo "⚪|Support"
    # Unknown
    else
        echo "⚫|Unknown"
    fi
}

# Build watchers HTML with categorization and icons
WATCHERS_HTML=""
if [[ ${#WATCHER_ENTRIES[@]} -gt 0 ]]; then
    for entry in "${WATCHER_ENTRIES[@]}"; do
        IFS='|' read -r name pid <<< "$entry"

        # Get category
        category_info=$(categorize_process "$name")
        IFS='|' read -r icon category <<< "$category_info"

        WATCHERS_HTML+="                    <div class=\"process-entry\">
                        <span class=\"process-icon\">$icon</span>
                        <div class=\"process-info\">
                            <span class=\"process-name\">$name</span>
                            <span class=\"process-category\">$category</span>
                        </div>
                        <span class=\"process-pid\">PID $pid</span>
                    </div>
"
    done
else
    WATCHERS_HTML="                    <div class=\"no-data\">No active processes detected</div>"
fi

# Parse log file and create structured entries
LOG_ENTRIES=""
if [[ -f "$LOG_FILE" ]]; then
    tail -50 "$LOG_FILE" | while IFS= read -r line; do
        # Escape HTML
        line=$(echo "$line" | sed 's/&/\&amp;/g; s/</\&lt;/g; s/>/\&gt;/g')

        # Style based on content
        if [[ "$line" =~ "NEW DETECTION" ]]; then
            LOG_ENTRIES+="                    <div class=\"log-entry alert\">$line</div>
"
        elif [[ "$line" =~ "started" ]]; then
            LOG_ENTRIES+="                    <div class=\"log-entry info\">$line</div>
"
        else
            LOG_ENTRIES+="                    <div class=\"log-entry\">$line</div>
"
        fi
    done
fi

[[ -z "$LOG_ENTRIES" ]] && LOG_ENTRIES="                    <div class=\"no-data\">No log entries</div>"

# Get timestamp
TIMESTAMP=$(date '+%Y-%m-%d %H:%M:%S')

# Generate HTML report with modern dark mode design
cat > "$REPORT_FILE" << 'EOF'
<!DOCTYPE html>
<html>
<head>
    <meta charset="UTF-8">
    <meta name="viewport" content="width=device-width, initial-scale=1.0">
    <title>Process Monitor</title>
    <style>
        * {
            margin: 0;
            padding: 0;
            box-sizing: border-box;
        }

        body {
            font-family: -apple-system, BlinkMacSystemFont, "Segoe UI", Roboto, "SF Pro Display", sans-serif;
            background: #0a0a0a;
            color: #e8e8e8;
            padding: 24px;
            line-height: 1.6;
        }

        .container {
            max-width: 1200px;
            margin: 0 auto;
        }

        .header {
            display: flex;
            justify-content: space-between;
            align-items: center;
            margin-bottom: 32px;
            padding-bottom: 16px;
            border-bottom: 1px solid #2a2a2a;
        }

        .header-left h1 {
            font-size: 24px;
            font-weight: 600;
            color: #ffffff;
            letter-spacing: -0.5px;
        }

        .header-left .subtitle {
            font-size: 13px;
            color: #707070;
            margin-top: 4px;
            font-family: 'SF Mono', 'Monaco', 'Courier New', monospace;
        }

        .header-right {
            text-align: right;
        }

        .status-badge {
            display: inline-block;
            padding: 6px 12px;
            background: rgba(52, 199, 89, 0.1);
            border: 1px solid rgba(52, 199, 89, 0.2);
            border-radius: 6px;
            font-size: 11px;
            font-weight: 600;
            color: #34c759;
            text-transform: uppercase;
            letter-spacing: 0.5px;
        }

        .timestamp {
            font-size: 11px;
            color: #606060;
            margin-top: 6px;
            font-family: 'SF Mono', 'Monaco', monospace;
        }

        .stats-grid {
            display: grid;
            grid-template-columns: repeat(auto-fit, minmax(200px, 1fr));
            gap: 16px;
            margin-bottom: 32px;
        }

        .stat-card {
            background: linear-gradient(135deg, #1a1a1a 0%, #151515 100%);
            border: 1px solid #2a2a2a;
            border-radius: 12px;
            padding: 20px;
            transition: all 0.2s;
        }

        .stat-card:hover {
            border-color: #3a3a3a;
            transform: translateY(-2px);
        }

        .stat-label {
            font-size: 11px;
            color: #808080;
            text-transform: uppercase;
            letter-spacing: 1px;
            margin-bottom: 8px;
        }

        .stat-value {
            font-size: 32px;
            font-weight: 700;
            color: #ffffff;
            font-variant-numeric: tabular-nums;
        }

        .section {
            background: #141414;
            border: 1px solid #2a2a2a;
            border-radius: 12px;
            padding: 24px;
            margin-bottom: 24px;
        }

        .section-header {
            display: flex;
            justify-content: space-between;
            align-items: center;
            margin-bottom: 20px;
        }

        .section-title {
            font-size: 14px;
            font-weight: 600;
            color: #ffffff;
            text-transform: uppercase;
            letter-spacing: 0.5px;
        }

        .section-badge {
            font-size: 10px;
            padding: 4px 8px;
            background: #252525;
            border-radius: 4px;
            color: #909090;
            font-family: 'SF Mono', monospace;
        }

        .process-list {
            display: grid;
            gap: 8px;
        }

        .process-entry {
            display: flex;
            align-items: center;
            gap: 12px;
            padding: 12px 16px;
            background: #0f0f0f;
            border: 1px solid #252525;
            border-radius: 8px;
            transition: all 0.15s;
        }

        .process-entry:hover {
            background: #1a1a1a;
            border-color: #353535;
        }

        .process-icon {
            font-size: 18px;
            line-height: 1;
            flex-shrink: 0;
        }

        .process-info {
            flex: 1;
            display: flex;
            flex-direction: column;
            gap: 2px;
        }

        .process-name {
            font-family: 'SF Mono', 'Monaco', monospace;
            font-size: 13px;
            color: #d0d0d0;
        }

        .process-category {
            font-size: 10px;
            color: #606060;
            text-transform: uppercase;
            letter-spacing: 0.5px;
        }

        .process-pid {
            font-family: 'SF Mono', 'Monaco', monospace;
            font-size: 11px;
            color: #707070;
            padding: 4px 8px;
            background: #1a1a1a;
            border-radius: 4px;
            flex-shrink: 0;
        }

        .log-viewer {
            background: #0a0a0a;
            border: 1px solid #252525;
            border-radius: 8px;
            padding: 16px;
            max-height: 500px;
            overflow-y: auto;
            font-family: 'SF Mono', 'Monaco', 'Courier New', monospace;
            font-size: 12px;
            line-height: 1.8;
        }

        .log-viewer::-webkit-scrollbar {
            width: 10px;
        }

        .log-viewer::-webkit-scrollbar-track {
            background: #0f0f0f;
            border-radius: 5px;
        }

        .log-viewer::-webkit-scrollbar-thumb {
            background: #2a2a2a;
            border-radius: 5px;
        }

        .log-viewer::-webkit-scrollbar-thumb:hover {
            background: #3a3a3a;
        }

        .log-entry {
            padding: 4px 0;
            color: #808080;
            border-bottom: 1px solid #151515;
        }

        .log-entry:last-child {
            border-bottom: none;
        }

        .log-entry.alert {
            color: #ff9f0a;
        }

        .log-entry.info {
            color: #64d2ff;
        }

        .no-data {
            color: #505050;
            text-align: center;
            padding: 32px;
            font-style: italic;
        }

        .actions {
            display: flex;
            gap: 12px;
            justify-content: center;
            margin-top: 32px;
        }

        button {
            background: #1a1a1a;
            color: #e0e0e0;
            border: 1px solid #2a2a2a;
            padding: 10px 20px;
            border-radius: 8px;
            font-size: 13px;
            font-weight: 500;
            cursor: pointer;
            transition: all 0.2s;
        }

        button:hover {
            background: #252525;
            border-color: #3a3a3a;
            transform: translateY(-1px);
        }

        button:active {
            transform: translateY(0);
        }

        .footer {
            text-align: center;
            margin-top: 32px;
            padding-top: 24px;
            border-top: 1px solid #2a2a2a;
            color: #505050;
            font-size: 11px;
            font-family: 'SF Mono', monospace;
        }

        .meta-info {
            margin-top: 16px;
            padding: 12px;
            background: #0f0f0f;
            border-radius: 6px;
            font-size: 11px;
            color: #606060;
            font-family: 'SF Mono', monospace;
        }
    </style>
</head>
<body>
    <div class="container">
        <div class="header">
            <div class="header-left">
                <h1>Process Monitor</h1>
                <div class="subtitle">Real-time system monitoring</div>
            </div>
            <div class="header-right">
                <div class="status-badge">● Active</div>
                <div class="timestamp">TIMESTAMP_PLACEHOLDER</div>
            </div>
        </div>

        <div class="stats-grid">
            <div class="stat-card">
                <div class="stat-label">Active Monitors</div>
                <div class="stat-value">COUNT_PLACEHOLDER</div>
            </div>
            <div class="stat-card">
                <div class="stat-label">Scan Interval</div>
                <div class="stat-value">30<span style="font-size:16px;color:#707070;">s</span></div>
            </div>
            <div class="stat-card">
                <div class="stat-label">Auto Refresh</div>
                <div class="stat-value">30<span style="font-size:16px;color:#707070;">s</span></div>
            </div>
        </div>

        <div class="section">
            <div class="section-header">
                <div class="section-title">Monitored Processes</div>
                <div class="section-badge">COUNT_PLACEHOLDER active</div>
            </div>
            <div class="process-list">
WATCHERS_PLACEHOLDER
            </div>
        </div>

        <div class="section">
            <div class="section-header">
                <div class="section-title">Activity Log</div>
                <div class="section-badge">Last 50 entries</div>
            </div>
            <div class="log-viewer" id="logViewer">
LOG_CONTENT_PLACEHOLDER
            </div>
            <div class="meta-info">
                Full log: LOG_PATH_PLACEHOLDER
            </div>
        </div>

        <div class="actions">
            <button onclick="window.location.reload()">↻ Refresh</button>
            <button onclick="window.close()">✕ Close</button>
        </div>

        <div class="footer">
            system-monitor v1.0 // auto-refresh enabled
        </div>
    </div>

    <script>
        // Auto-refresh every 30 seconds
        setTimeout(() => window.location.reload(), 30000);

        // Auto-scroll log to bottom
        const logViewer = document.getElementById('logViewer');
        if (logViewer) {
            logViewer.scrollTop = logViewer.scrollHeight;
        }
    </script>
</body>
</html>
EOF

# Replace placeholders
sed -i '' "s/TIMESTAMP_PLACEHOLDER/$TIMESTAMP/g" "$REPORT_FILE"
sed -i '' "s/COUNT_PLACEHOLDER/$WATCHER_COUNT/g" "$REPORT_FILE"
sed -i '' "s|LOG_PATH_PLACEHOLDER|$LOG_FILE|g" "$REPORT_FILE"

# Replace watchers placeholder with actual content
perl -i -0pe "s|WATCHERS_PLACEHOLDER|$WATCHERS_HTML|g" "$REPORT_FILE" 2>/dev/null

# Replace log content placeholder
# Read log entries into variable
LOG_CONTENT_FINAL=""
if [[ -f "$LOG_FILE" ]]; then
    while IFS= read -r line; do
        # Escape for HTML
        line=$(echo "$line" | sed 's/&/\&amp;/g; s/</\&lt;/g; s/>/\&gt;/g')

        # Add appropriate styling
        if [[ "$line" =~ "NEW DETECTION" ]]; then
            LOG_CONTENT_FINAL+="                    <div class=\"log-entry alert\">$line</div>
"
        elif [[ "$line" =~ "started" ]]; then
            LOG_CONTENT_FINAL+="                    <div class=\"log-entry info\">$line</div>
"
        else
            LOG_CONTENT_FINAL+="                    <div class=\"log-entry\">$line</div>
"
        fi
    done < <(tail -50 "$LOG_FILE")
fi

[[ -z "$LOG_CONTENT_FINAL" ]] && LOG_CONTENT_FINAL="                    <div class=\"no-data\">No log entries</div>"

# Replace log content
perl -i -0pe "s|LOG_CONTENT_PLACEHOLDER|$LOG_CONTENT_FINAL|g" "$REPORT_FILE" 2>/dev/null

echo "$REPORT_FILE"
