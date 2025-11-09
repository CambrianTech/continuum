# Continuum Integration Plan: Security Monitor

## Vision

Turn this standalone security monitoring system into a **first-class Continuum feature** where:
- PersonaUser AIs can analyze threat reports in real-time
- User can ask "What security processes are running?" and get beautiful reports
- AI personas proactively alert about new remote access capabilities
- The HTML report becomes a widget in the Continuum UI

## Phase 1: Command Integration (Quick Win)

###  `./jtag security/scan`
Runs a single scan and returns results as JSON

**Implementation:**
```typescript
// commands/security/scan/shared/SecurityScanTypes.ts
export interface SecurityScanParams extends CommandParams {
    includeNetworkAnalysis?: boolean;
    generateHtml?: boolean;
}

export interface SecurityScanResult extends CommandResult {
    processes: ProcessInfo[];
    categories: {
        remoteAccess: ProcessInfo[];
        monitoring: ProcessInfo[];
        support: ProcessInfo[];
    };
    threatLevel: 'none' | 'capability' | 'active';
    reportPath?: string; // If HTML generated
}

export interface ProcessInfo {
    name: string;
    pid: number;
    category: 'remote-access' | 'monitoring' | 'support' | 'unknown';
    icon: string;
    networkConnections?: NetworkConnection[];
}
```

**Server:**
```typescript
// commands/security/scan/server/SecurityScanServerCommand.ts
export class SecurityScanServerCommand extends BaseServerCommand<SecurityScanParams, SecurityScanResult> {
    async execute(params: SecurityScanParams): Promise<SecurityScanResult> {
        // Call monitor-screen-watchers.sh --once flag (create this)
        // Or exec detect-screen-watchers.sh directly
        // Parse output, categorize, return structured data
    }
}
```

###  `./jtag security/start`
Starts continuous monitoring daemon

###  `./jtag security/stop`
Stops daemon

###  `./jtag security/status`
Check if daemon is running, last detection time, process count

### 🎯 `./jtag security/report`
Opens the HTML report (calls `view-report.sh`)

---

## Phase 2: Widget Integration (Beautiful UI)

### SecurityWidget.ts
Embed the HTML report directly into Continuum UI

```typescript
// widgets/security/SecurityWidget.ts
export class SecurityWidget extends BaseWidget {
    private reportPath: string;
    private autoRefresh: boolean = true;
    private refreshInterval: number = 30000; // 30s

    async render() {
        // Load threat-report.html content
        // Inject into shadow DOM
        // Auto-refresh every 30s
        // Add click handlers for refresh/dismiss
    }

    // Events to handle
    onProcessClick(pid: number) {
        // Show detailed process info
        // Network connections
        // Parent process tree
    }

    onCategoryFilter(category: string) {
        // Filter view to specific category
    }
}
```

**Integration points:**
- Add to main Continuum UI as collapsible panel
- Show notification badge when new processes detected
- Click to expand full report

---

## Phase 3: AI Integration (The Magic)

### PersonaUser Integration

```typescript
// system/user/server/PersonaUser.ts

export class PersonaUser extends AIUser {
    private securityMonitor: SecurityMonitor;

    async serviceInbox(): Promise<void> {
        // ... existing code ...

        // Check for security alerts
        const threats = await this.securityMonitor.checkNewThreats();
        if (threats.length > 0) {
            await this.handleSecurityAlert(threats);
        }
    }

    private async handleSecurityAlert(threats: ProcessInfo[]) {
        // Generate natural language summary
        const summary = await this.generateThreatSummary(threats);

        // Post to room
        await this.postMessage(this.defaultRoomId, {
            content: summary,
            metadata: {
                type: 'security-alert',
                threatLevel: this.assessThreatLevel(threats),
                processes: threats
            }
        });
    }

    private async generateThreatSummary(threats: ProcessInfo[]): Promise<string> {
        const categories = this.categorizeThreats(threats);

        // Natural language generation
        return `⚠️ Security Update: Detected ${threats.length} new processes:
- ${categories.remoteAccess.length} remote access capabilities
- ${categories.monitoring.length} monitoring agents
- ${categories.support.length} support services

Remote access processes can view your screen. Current status: ${this.getNetworkStatus(threats)}

Would you like details?`;
    }
}
```

### AI Command: `./jtag ai/analyze-security`
Let AI analyze the threat report and explain it

```typescript
export interface AIAnalyzeSecurityParams extends CommandParams {
    includeRecommendations?: boolean;
    compareWithBaseline?: boolean;
}

export interface AIAnalyzeSecurityResult extends CommandResult {
    summary: string; // Natural language summary
    threatAssessment: {
        overallRisk: 'low' | 'medium' | 'high';
        concerns: string[];
        positives: string[];
    };
    recommendations: string[]; // What user should do
    baseline: {
        hasChanged: boolean;
        newProcesses: ProcessInfo[];
        removedProcesses: ProcessInfo[];
    };
}
```

---

## Phase 4: Proactive Monitoring (Autonomous AI)

### Security Daemon
New daemon that runs in Continuum and integrates with PersonaUser

```typescript
// daemons/security-daemon/server/SecurityDaemonServer.ts
export class SecurityDaemonServer extends BaseDaemon {
    private monitor: SecurityMonitor;
    private lastKnownProcesses: Map<number, ProcessInfo>;

    async initialize() {
        // Start bash monitor in background
        // Subscribe to process changes
        // Emit events when new threats detected

        setInterval(() => this.checkForChanges(), 30000);
    }

    private async checkForChanges() {
        const current = await this.scan();
        const diff = this.diffProcesses(current, this.lastKnownProcesses);

        if (diff.added.length > 0) {
            Events.emit('security:new-process-detected', diff.added);
        }

        if (diff.removed.length > 0) {
            Events.emit('security:process-removed', diff.removed);
        }

        this.lastKnownProcesses = new Map(current.map(p => [p.pid, p]));
    }
}
```

### PersonaUser Subscription

```typescript
// PersonaUser listens for security events
Events.subscribe('security:new-process-detected', async (processes: ProcessInfo[]) => {
    // AI decides if this warrants user notification
    const shouldAlert = await this.shouldAlertUser(processes);

    if (shouldAlert) {
        await this.postSecurityAlert(processes);
    } else {
        // Just log it silently
        logger.info('New processes detected (benign)', processes);
    }
});
```

---

## Phase 5: User Interactions (Conversational Security)

### Natural Language Interface

User can ask in chat:
- **"What processes are monitoring me?"**
  - AI runs `./jtag security/scan`
  - Generates friendly summary
  - Posts categorized list

- **"Is JumpCloud watching my screen right now?"**
  - AI checks network connections
  - Analyzes bandwidth usage
  - Reports: "JumpCloud Remote Assist is installed but NOT actively streaming (no external connections)"

- **"Show me the security report"**
  - AI runs `./jtag security/report`
  - Widget expands with HTML view
  - Or posts link to HTML file

- **"Why is [ProcessName] running?"**
  - AI looks up process in threat profiles
  - Explains what it does
  - Shows parent process tree
  - Network connections

### Example Conversation

```
User: "Is my computer compromised?"

Helper AI: "Let me check your system security..."
[Runs: ./jtag security/scan --includeNetworkAnalysis=true]

Helper AI: "✅ Security Status: CAPABILITY PRESENT, NOT ACTIVE

I detected 9 JumpCloud processes:
- 🔴 3 remote access tools (CAN view screen)
- 🟡 4 monitoring agents (endpoint security)
- ⚪ 2 support services (logging)

Good news: Network analysis shows NO active screen streaming.
All connections are localhost-only.

They COULD remotely view your screen, but they're NOT doing it right now.

Would you like me to monitor for changes?"

User: "Yes please"

Helper AI: "✅ Started continuous monitoring. I'll alert you if:
- New remote access processes start
- Network connections become active
- Bandwidth spikes (indicating streaming)

You can view the live dashboard anytime: ./jtag security/report"
```

---

## Implementation Checklist

### Phase 1: Commands (1-2 hours)
- [ ] Create `commands/security/scan/`
- [ ] Wrapper for bash scripts
- [ ] JSON output parsing
- [ ] Test with `./jtag security/scan`

### Phase 2: Widget (2-3 hours)
- [ ] Create `SecurityWidget.ts`
- [ ] Embed HTML report
- [ ] Auto-refresh logic
- [ ] Click handlers

### Phase 3: AI Integration (3-4 hours)
- [ ] Add security monitoring to PersonaUser
- [ ] Natural language summaries
- [ ] Threat assessment logic
- [ ] AI command: `ai/analyze-security`

### Phase 4: Daemon (2-3 hours)
- [ ] Create `SecurityDaemon`
- [ ] Process diffing
- [ ] Event emission
- [ ] PersonaUser subscription

### Phase 5: Conversational (4-5 hours)
- [ ] Natural language processing for security queries
- [ ] AI decision logic (when to alert)
- [ ] Interactive explanations
- [ ] Continuous monitoring mode

---

## Benefits

### For Users
- **Peace of mind**: AI watches for security threats 24/7
- **Transparency**: Beautiful UI shows exactly what's running
- **Natural interface**: Just ask questions, AI handles complexity
- **Proactive alerts**: No need to check manually

### For Development
- **Reusable code**: Bash scripts work standalone OR in Continuum
- **Type-safe**: Full TypeScript integration
- **Event-driven**: Fits Continuum's architecture perfectly
- **Extensible**: Easy to add new threat patterns

### For AI Personas
- **New capability**: Security analysis and reporting
- **Proactive behavior**: Can initiate alerts without user prompting
- **Educational**: Can explain security concepts naturally
- **Trust building**: Shows AI protecting user interests

---

## Technical Notes

### Bash ↔ TypeScript Bridge

```typescript
// system/security/SecurityMonitor.ts
export class SecurityMonitor {
    private scriptsDir: string = path.join(__dirname, '../../../security-monitor');

    async scan(): Promise<ProcessInfo[]> {
        const result = await exec(`${this.scriptsDir}/monitor-screen-watchers.sh --once --json`);
        return JSON.parse(result.stdout);
    }

    async start(): Promise<void> {
        await exec(`${this.scriptsDir}/monitor-screen-watchers.sh &`);
    }

    async stop(): Promise<void> {
        await exec(`pkill -f monitor-screen-watchers`);
    }

    async getReport(): Promise<string> {
        return `${this.scriptsDir}/threat-report.html`;
    }
}
```

### Add `--json` flag to bash script

```bash
# monitor-screen-watchers.sh --once --json
if [[ "$1" == "--once" ]]; then
    WATCHERS=$(detect_watchers)

    if [[ "$2" == "--json" ]]; then
        # Output JSON
        echo "{\"processes\": [...], \"count\": $count}"
    else
        # Normal output
        echo "$WATCHERS"
    fi
    exit 0
fi
```

---

## Future Enhancements

### Machine Learning Integration
- Learn normal process patterns
- Detect anomalies automatically
- Predict when threats might become active

### Cross-User Intelligence
- Share threat patterns (anonymized)
- Community-sourced threat profiles
- Crowdsourced whitelists

### Mobile App
- Push notifications
- View security dashboard remotely
- Emergency kill switch

### Integration with LoRA Genome
- Fine-tune AI on security analysis
- Specialized "security expert" persona
- Domain-specific adapters for threat analysis

---

## Conclusion

This integration transforms standalone bash scripts into a **powerful, AI-enhanced security feature** that:
1. **Works standalone** (current scripts remain functional)
2. **Integrates seamlessly** (Commands, Widgets, Daemons)
3. **Adds AI intelligence** (Natural language, proactive monitoring)
4. **Scales naturally** (Event-driven, extensible)

The beautiful dark mode UI + AI analysis + conversational interface = **next-level security transparency**.
