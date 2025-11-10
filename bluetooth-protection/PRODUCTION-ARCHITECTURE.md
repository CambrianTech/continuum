# Production Architecture: Bluetooth Protection

**Current**: Bash script (prototype/proof-of-concept)
**Production**: Node.js/Rust/C++ native daemon

## Why Rewrite?

**Bash limitations:**
- No proper event handling
- Can't intercept Bluetooth stack directly
- Polling-based (inefficient)
- Limited data structures
- Fragile text parsing

**Native daemon advantages:**
- ✅ Hook into IOBluetooth framework directly
- ✅ Real event-driven architecture
- ✅ Efficient memory/CPU usage
- ✅ Structured data (SQLite/JSON)
- ✅ Can integrate with Continuum widget

## Language Comparison

### Option 1: **Node.js/TypeScript** (Recommended for speed)
**Pros:**
- Already using in Continuum
- Fast development
- Easy IPC with Electron widget
- Good macOS frameworks support
- SQLite via `better-sqlite3`

**Cons:**
- Slightly higher memory than Rust/C++
- Runtime dependency (Node.js)

**Architecture:**
```typescript
// Main daemon process
import { EventEmitter } from 'events';
import IOBluetooth from 'iokit-bluetooth'; // Native binding

class BluetoothProtectionDaemon extends EventEmitter {
    private db: Database;
    private knownDevices: Set<string>;
    
    constructor() {
        this.db = new Database('~/.continuum/bluetooth.db');
        this.setupBluetoothHooks();
    }
    
    private setupBluetoothHooks() {
        // Hook IOBluetoothDevice inquiry events
        IOBluetooth.on('deviceInquiryStarted', this.handleInquiry);
        IOBluetooth.on('pairingRequested', this.handlePairingRequest);
    }
    
    private async handlePairingRequest(device: BluetoothDevice) {
        // Check if device is known
        const history = await this.db.getDeviceHistory(device.address);
        
        // Calculate risk score
        const risk = this.calculateRisk(device, history);
        
        // Emit event to Continuum widget
        this.emit('pairing-request', {
            device,
            history,
            risk,
            recommendation: risk > 70 ? 'block' : 'review'
        });
        
        // Log incident
        await this.logIncident(device, risk);
    }
}
```

### Option 2: **Rust** (Best performance)
**Pros:**
- Zero-cost abstractions
- Memory safe
- Blazing fast
- Small binary size
- Excellent for daemons

**Cons:**
- Slower development
- macOS frameworks require FFI bindings
- Steeper learning curve

**Architecture:**
```rust
use iokit_bluetooth::*;
use tokio::sync::mpsc;

struct BluetoothProtector {
    db: SqliteConnection,
    known_devices: HashSet<String>,
    event_sender: mpsc::Sender<BluetoothEvent>,
}

impl BluetoothProtector {
    async fn handle_pairing_request(&self, device: BluetoothDevice) {
        // Check device history
        let history = self.db.get_device_history(&device.address).await;
        
        // Calculate risk
        let risk = calculate_risk(&device, &history);
        
        // Send to Continuum widget via IPC
        self.event_sender.send(BluetoothEvent::PairingRequest {
            device,
            risk,
            recommendation: if risk > 70 { "block" } else { "review" }
        }).await;
    }
}
```

### Option 3: **C++** (Maximum control)
**Pros:**
- Direct IOKit access
- Best performance possible
- Full system integration
- Your favorite!

**Cons:**
- Most development time
- Memory management complexity
- More testing needed

**Architecture:**
```cpp
#include <IOBluetooth/IOBluetooth.h>
#include <sqlite3.h>

class BluetoothProtectionDaemon {
private:
    sqlite3* db;
    std::unordered_set<std::string> knownDevices;
    
public:
    BluetoothProtectionDaemon() {
        sqlite3_open("~/.continuum/bluetooth.db", &db);
        setupBluetoothHooks();
    }
    
    void setupBluetoothHooks() {
        // Register for IOBluetooth notifications
        IOBluetoothRegisterForDeviceConnectNotifications(
            deviceInquiryCallback,
            NULL
        );
    }
    
    static void deviceInquiryCallback(void* userRefCon, 
                                      IOBluetoothUserNotificationRef inRef,
                                      IOBluetoothDeviceRef deviceRef) {
        // Handle pairing request
        BluetoothDeviceAddress addr;
        IOBluetoothDeviceGetAddress(deviceRef, &addr);
        
        // Check history, calculate risk, emit event
        // ...
    }
};
```

## Recommended Architecture: **Node.js for MVP, Rust for v2**

### Phase 1: Node.js/TypeScript (Quick deployment)
- Reuse Continuum's existing Node.js stack
- Fast development (2-3 weeks)
- Easy integration with Electron widget
- SQLite for device history

### Phase 2: Rust optimization (After validation)
- Rewrite core daemon in Rust
- Keep Node.js as thin IPC layer
- 10x better performance
- Production-ready daemon

## Integration with Continuum

**Current (Bash script):**
```
Bash script → System logs → Text parsing → Files → User checks manually
```

**Production (Node.js daemon):**
```
IOBluetooth → Node daemon → SQLite → IPC → Continuum Widget → User UI
```

**Flow:**
1. **Bluetooth event** → IOBluetooth framework
2. **Daemon intercepts** → BluetoothProtectionDaemon
3. **Risk analysis** → Calculate risk score + check history
4. **Database log** → SQLite device history
5. **IPC event** → Send to Continuum widget
6. **UI prompt** → Beautiful Continuum dialog (not macOS default)
7. **User decision** → Allow/Block/Learn More
8. **Database update** → Record user action
9. **Learning** → Improve risk model

## Data Storage

**SQLite schema:**
```sql
CREATE TABLE devices (
    address TEXT PRIMARY KEY,
    name TEXT,
    first_seen INTEGER,
    last_seen INTEGER,
    attempt_count INTEGER,
    success_count INTEGER,
    user_action TEXT,  -- 'allowed', 'blocked', 'ignored'
    risk_score REAL,
    notes TEXT
);

CREATE TABLE incidents (
    id INTEGER PRIMARY KEY,
    device_address TEXT,
    timestamp INTEGER,
    event_type TEXT,  -- 'pairing_request', 'connection', 'disconnection'
    risk_score REAL,
    user_action TEXT,
    system_state TEXT,  -- JSON blob of system info
    FOREIGN KEY (device_address) REFERENCES devices(address)
);

CREATE TABLE patterns (
    device_address TEXT,
    pattern_type TEXT,  -- 'frequency', 'time_of_day', 'automated'
    detected_at INTEGER,
    confidence REAL,
    details TEXT,  -- JSON blob
    FOREIGN KEY (device_address) REFERENCES devices(address)
);
```

## IPC Protocol (Daemon ↔ Widget)

**Unix domain socket** or **Named pipe:**

```typescript
// Daemon → Widget
interface PairingRequestEvent {
    type: 'pairing-request';
    device: {
        address: string;
        name: string | null;
        deviceClass: string;
    };
    history: {
        firstSeen: Date;
        lastSeen: Date;
        attemptCount: number;
        neverPaired: boolean;
    };
    risk: {
        score: number;  // 0-100
        level: 'low' | 'medium' | 'high' | 'critical';
        factors: string[];  // ["persistent", "no_name", "automated"]
    };
    recommendation: 'allow' | 'block' | 'review';
}

// Widget → Daemon
interface UserDecisionEvent {
    type: 'user-decision';
    deviceAddress: string;
    action: 'allow' | 'block' | 'ignore';
    permanent: boolean;
}
```

## Deployment

**Location:**
```
/Applications/Continuum.app/Contents/Resources/daemons/bluetooth-protection
```

**Launch:**
- Continuum app starts daemon on launch
- Daemon runs as user process (not root - safer)
- Auto-restart on crash
- Graceful shutdown with Continuum

**Configuration:**
```json
// ~/.continuum/config.json
{
  "bluetoothProtection": {
    "enabled": true,
    "alertOnUnknown": true,
    "autoBlockKnownThreats": false,
    "communityIntelligence": false,  // Opt-in
    "logLevel": "info"
  }
}
```

## Testing Strategy

**Test with your real attack:**
- C08MRSEM2330 is perfect test case
- Multiple pairing attempts
- Verify detection, logging, alerting
- Confirm forensics capture works

**Simulated attacks:**
- Rapid pairing attempts (DoS test)
- Known good devices (false positive test)
- Edge cases (no device name, spoofed address)

## Next Steps

1. **Validate prototype** with your C08MRSEM2330 attacks
2. **Design Continuum UI** for pairing prompts
3. **Build Node.js daemon** (2-3 weeks)
4. **Beta test** with security-conscious users
5. **Consider Rust rewrite** after validation

**We know where our code is** - This integrates cleanly into Continuum's existing architecture!

