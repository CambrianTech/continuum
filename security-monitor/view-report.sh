#!/bin/bash
# Quick script to view the threat report

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPORT_FILE="$SCRIPT_DIR/threat-report.html"

if [[ -f "$REPORT_FILE" ]]; then
    open "$REPORT_FILE"
    echo "Opening report: $REPORT_FILE"
else
    echo "No report found. Generate one with: ./monitor-screen-watchers.sh"
fi
