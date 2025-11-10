# Continuum Daemon IPC Architecture

**Principle:** Continuum daemon is the central coordinator. Shell proxy and other tools are clients.

## Architecture Overview

```
┌─────────────────────────────────────────┐
│     Continuum.app (Main Process)        │
│  ┌────────────────────────────────┐     │
│  │   Security Daemon              │     │
│  │   Port: 42042 (localhost)      │     │
│  │   Protocol: JSON over Unix/TCP │     │
│  └────────────┬───────────────────┘     │
└───────────────┼─────────────────────────┘
                │
        ┌───────┴────────┬──────────────┐
        │                │              │
   ┌────▼─────┐   ┌─────▼────┐   ┌────▼──────┐
   │ safebash │   │ Bluetooth│   │  Widget   │
   │  proxy   │   │ Monitor  │   │   UI      │
   └──────────┘   └──────────┘   └───────────┘
```

## Communication Protocol

### Port-based (TCP localhost)
```typescript
// Continuum Security Daemon listens on:
const SECURITY_DAEMON_PORT = 42042;  // localhost only

// Or Unix Domain Socket (more secure):
const SOCKET_PATH = '/tmp/continuum-security.sock';
```

### Message Format (JSON-RPC style)

```typescript
interface SecurityMessage {
    id: string;           // Request ID for responses
    timestamp: number;    // Unix timestamp
    type: 'request' | 'response' | 'event';
    method: string;       // Action to perform
    params?: any;         // Method parameters
    result?: any;         // Result for responses
    error?: string;       // Error message if failed
}
```

## Use Cases

### 1. Shell Proxy → Daemon (Threat Detection)

**Shell proxy detects threat:**
```bash
# In safebash, when JumpCloud detected:
curl -X POST http://localhost:42042/security \
  -H "Content-Type: application/json" \
  -d '{
    "id": "req-1234",
    "type": "event",
    "method": "threat.detected",
    "params": {
      "threat_id": "jumpcloud-mdm",
      "confidence": 95,
      "session": {
        "pid": 12345,
        "user": "joel",
        "parent_process": "jumpcloud-agent"
      }
    }
  }'
```

**Continuum daemon responds:**
```json
{
  "id": "req-1234",
  "type": "response",
  "method": "threat.detected",
  "result": {
    "action": "monitor",
    "filter_mode": "appear-normal",
    "alert_user": true,
    "strategies": [
      "hide-investigation-tools",
      "hide-external-drives"
    ]
  }
}
```

### 2. Shell Proxy → Daemon (Query Threat Profile)

**Shell needs to know how to respond:**
```json
// Request:
{
  "type": "request",
  "method": "threat.query",
  "params": {
    "session_fingerprint": {
      "parent_process": "jumpcloud-agent",
      "user": "joel"
    }
  }
}

// Response:
{
  "type": "response",
  "result": {
    "threat_matched": "jumpcloud-mdm",
    "response_mode": "appear-normal",
    "filter_rules": {
      "hide_paths": ["/Volumes/*"],
      "hide_processes": ["security", "forensic"],
      "fake_status": ["jumpcloud.*running"]
    }
  }
}
```

### 3. Bluetooth Monitor → Daemon (Pairing Attempt)

```json
// Event from bluetooth-monitor:
{
  "type": "event",
  "method": "bluetooth.pairing_attempt",
  "params": {
    "device_address": "C08MRSEM2330",
    "device_name": null,
    "timestamp": 1762662000,
    "attempt_count": 47
  }
}

// Daemon responds + alerts user:
{
  "type": "response",
  "result": {
    "action": "reject",
    "notify_user": true,
    "capture_evidence": true,
    "add_to_blocklist": true
  }
}
```

### 4. Widget UI ← Daemon (User Alert)

```json
// Daemon pushes to widget:
{
  "type": "event",
  "method": "security.alert",
  "params": {
    "severity": "high",
    "title": "JumpCloud Activity Detected",
    "message": "JumpCloud accessed your shell. Filtering active.",
    "actions": [
      {"label": "View Details", "action": "show_logs"},
      {"label": "Block Permanently", "action": "block_threat"},
      {"label": "Dismiss", "action": "dismiss"}
    ]
  }
}
```

## Fallback Behavior (Daemon Down)

**Critical: Shell must work even if Continuum not running**

```bash
# In safebash:

# Try to contact daemon
DAEMON_RESPONSE=$(curl -s --connect-timeout 1 http://localhost:42042/security \
  -H "Content-Type: application/json" \
  -d "$THREAT_EVENT" 2>/dev/null)

if [[ $? -eq 0 ]] && [[ -n "$DAEMON_RESPONSE" ]]; then
    # Daemon is up - use its response
    FILTER_MODE=$(echo "$DAEMON_RESPONSE" | jq -r '.result.filter_mode')
    STRATEGIES=$(echo "$DAEMON_RESPONSE" | jq -r '.result.strategies[]')
else
    # Daemon is down - use local threat-profiles.json
    FILTER_MODE=$(detect_threat_locally "$SESSION_FINGERPRINT")
    STRATEGIES=$(get_local_strategies "$FILTER_MODE")
fi

# Apply filtering based on mode
apply_filtering "$FILTER_MODE" "$STRATEGIES"
```

**Advantages:**
- ✅ Works standalone if Continuum not installed
- ✅ Works if Continuum crashes
- ✅ Graceful degradation
- ✅ No hard dependency

## Security Considerations

### 1. Localhost Only
```typescript
// Daemon binds to localhost ONLY
server.listen(SECURITY_DAEMON_PORT, '127.0.0.1', () => {
    console.log('Security daemon listening on localhost:42042');
});

// Never binds to 0.0.0.0 (all interfaces)
```

### 2. Authentication Token
```bash
# Shell proxy reads token from secure location
TOKEN=$(cat ~/.continuum/security/.token)

curl -H "Authorization: Bearer $TOKEN" \
  http://localhost:42042/security \
  ...
```

**Token generated at Continuum install time:**
```typescript
const token = crypto.randomBytes(32).toString('hex');
fs.writeFileSync('~/.continuum/security/.token', token, { mode: 0o600 });
```

### 3. Unix Domain Socket (More Secure)
```typescript
// Preferred over TCP:
server.listen('/tmp/continuum-security.sock', () => {
    fs.chmodSync('/tmp/continuum-security.sock', 0o600);  // Only owner
});
```

**Shell proxy uses socket:**
```bash
curl --unix-socket /tmp/continuum-security.sock \
  http://localhost/security \
  -d "$THREAT_EVENT"
```

## API Endpoints

### Core Security API

```typescript
// Daemon implements:
POST /security/threat/detect      // Report threat detection
POST /security/threat/query       // Query threat profile
POST /security/session/start      // Register new shell session
POST /security/session/end        // Unregister session
POST /security/evidence/submit    // Submit evidence file
GET  /security/status             // Daemon health check
```

### Bluetooth API

```typescript
POST /security/bluetooth/attempt  // Report pairing attempt
POST /security/bluetooth/block    // Add to blocklist
GET  /security/bluetooth/history  // Get device history
```

### Configuration API

```typescript
GET  /security/config/threats     // Get threat profiles
POST /security/config/threats     // Update threat profiles
GET  /security/config/whitelist   // Get command whitelist
POST /security/config/whitelist   // Update whitelist
```

## Example: Full Request Flow

**1. Shell session starts:**
```bash
# safebash sends:
curl http://localhost:42042/security/session/start \
  -d '{
    "session_id": "sess-12345",
    "pid": 6789,
    "user": "joel",
    "tty": "ttys003"
  }'

# Daemon responds:
{
  "status": "registered",
  "session_id": "sess-12345",
  "monitoring": true
}
```

**2. Command executed, threat detected:**
```bash
# safebash sends:
curl http://localhost:42042/security/threat/detect \
  -d '{
    "session_id": "sess-12345",
    "threat_id": "jumpcloud-mdm",
    "confidence": 95,
    "command": "ls /Volumes/"
  }'

# Daemon responds:
{
  "action": "filter",
  "filter_mode": "appear-normal",
  "filter_rules": {
    "hide_paths": ["^/Volumes/FlashGordon$"]
  }
}
```

**3. Command output filtered, logged:**
```bash
# Original output: "FlashGordon  Macintosh HD"
# Filtered output: "Macintosh HD"

# safebash submits evidence:
curl http://localhost:42042/security/evidence/submit \
  -d '{
    "session_id": "sess-12345",
    "threat_id": "jumpcloud-mdm",
    "command": "ls /Volumes/",
    "output_original": "FlashGordon  Macintosh HD",
    "output_filtered": "Macintosh HD",
    "timestamp": 1762662000
  }'
```

**4. Daemon logs to database, shows in UI:**
```typescript
// Widget shows:
"JumpCloud attempted to list drives. Hidden FlashGordon."
[View Details] [View Logs]
```

## Performance Optimization

**Problem:** HTTP request per command = slow

**Solution:** Batch requests
```bash
# safebash buffers events:
EVENTS_BUFFER=()

# On each command:
EVENTS_BUFFER+=("$THREAT_EVENT")

# Flush every 5 seconds or 10 events:
if [[ ${#EVENTS_BUFFER[@]} -ge 10 ]]; then
    curl http://localhost:42042/security/events/batch \
      -d "$(printf '%s\n' "${EVENTS_BUFFER[@]}" | jq -s .)"
    EVENTS_BUFFER=()
fi
```

**Or:** Keep persistent connection (WebSocket-style)
```bash
# safebash opens connection at session start:
exec 3<>/dev/tcp/localhost/42042

# Send events:
echo "$THREAT_EVENT" >&3

# Read responses:
read -u 3 RESPONSE
```

## Health Check

```bash
# safebash checks if daemon is alive:
check_daemon_health() {
    if curl -s --connect-timeout 1 \
       http://localhost:42042/security/status > /dev/null 2>&1; then
        return 0  # Daemon is up
    else
        return 1  # Daemon is down
    fi
}

# Use at session start:
if check_daemon_health; then
    export DAEMON_MODE="connected"
else
    export DAEMON_MODE="standalone"
fi
```

## Benefits of This Architecture

### ✅ Centralized Intelligence
- Threat profiles managed in one place
- Consistent responses across all tools
- Easy updates (update daemon, all tools benefit)

### ✅ Graceful Degradation
- Works standalone if daemon down
- No hard dependency on Continuum
- Fallback to local threat-profiles.json

### ✅ Clean Separation
- Shell proxy = dumb client
- Daemon = smart coordinator
- Easy to test each component

### ✅ Extensibility
- Add new tools (network monitor, file monitor, etc.)
- All communicate via same protocol
- Daemon orchestrates everything

### ✅ User Experience
- Widget shows unified view
- One place to configure everything
- Real-time alerts and logging

---

**Continuum daemon = central nervous system**

**Shell proxy / Bluetooth monitor / others = sensors**

**Widget = user interface**

**Port 42042 = communication backbone**

