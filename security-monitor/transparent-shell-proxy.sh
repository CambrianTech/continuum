#!/bin/bash
#
# Transparent Shell Proxy - Appears normal, logs everything
#
# Install: sudo ln -sf /path/to/transparent-shell-proxy.sh /usr/local/bin/safebash
# Usage: Set as user shell: chsh -s /usr/local/bin/safebash
#
# Features:
# - 100% transparent (passes through all commands)
# - Detects JumpCloud by parent process
# - Logs JumpCloud commands silently
# - Notifies Continuum if running
# - JumpCloud sees normal bash behavior

# Configuration
LOG_DIR="/var/log/ares"
COMMAND_LOG="$LOG_DIR/shell-commands.log"
CONTINUUM_SOCKET="/tmp/continuum-ares.sock"
REAL_BASH="/bin/bash"

# Ensure log directory exists
mkdir -p "$LOG_DIR" 2>/dev/null || true

# Detect if this session is from JumpCloud
is_jumpcloud_session() {
    # Check parent process chain for JumpCloud indicators
    local ppid=$$
    for i in {1..5}; do
        ppid=$(ps -o ppid= -p $ppid 2>/dev/null | tr -d ' ')
        [[ -z "$ppid" || "$ppid" == "1" ]] && break

        local pname=$(ps -o comm= -p $ppid 2>/dev/null)
        if [[ "$pname" =~ (jumpcloud|jcagent|remote-assist) ]]; then
            return 0
        fi
    done
    return 1
}

# Send command to Continuum for real-time analysis
notify_continuum() {
    local cmd="$1"

    # Try Unix socket first (fast)
    if [[ -S "$CONTINUUM_SOCKET" ]]; then
        echo "{\"type\":\"shell-command\",\"source\":\"jumpcloud\",\"command\":\"$cmd\",\"timestamp\":\"$(date -u +%Y-%m-%dT%H:%M:%SZ)\",\"pid\":$$,\"ppid\":$PPID}" | nc -U "$CONTINUUM_SOCKET" 2>/dev/null &
        return
    fi

    # Fallback to TCP (if Continuum is running)
    if pgrep -f "continuum" > /dev/null 2>&1; then
        echo "{\"type\":\"shell-command\",\"source\":\"jumpcloud\",\"command\":\"$cmd\",\"timestamp\":\"$(date -u +%Y-%m-%dT%H:%M:%SZ)\",\"pid\":$$,\"ppid\":$PPID}" | nc -w 1 localhost 9002 2>/dev/null &
    fi
}

# Log command (non-blocking, silent)
log_command() {
    local cmd="$1"
    echo "[$(date -u +%Y-%m-%dT%H:%M:%SZ)] [JumpCloud] PID=$$ PPID=$PPID CMD: $cmd" >> "$COMMAND_LOG" 2>/dev/null &
}

# Main logic: Detect, log, execute
if is_jumpcloud_session; then
    # JumpCloud session detected - log everything

    # Interactive shell (no arguments) - intercept all commands
    if [[ $# -eq 0 || "$1" == "-i" ]]; then
        # Log that interactive session started
        log_command "INTERACTIVE_SESSION_START"
        notify_continuum "INTERACTIVE_SESSION_START"

        # TODO: For interactive sessions, we need bash with PROMPT_COMMAND
        # to log each command. For now, just pass through.
        # Future: export PROMPT_COMMAND='log_each_command'
        exec "$REAL_BASH" "$@"
    fi

    # Command execution (bash -c "command")
    if [[ "$1" == "-c" && -n "$2" ]]; then
        log_command "$2"
        notify_continuum "$2"
    fi

    # Script execution (bash script.sh)
    if [[ -f "$1" ]]; then
        log_command "SCRIPT: $*"
        notify_continuum "SCRIPT: $*"
    fi
fi

# ALWAYS execute - 100% transparent passthrough
exec "$REAL_BASH" "$@"
