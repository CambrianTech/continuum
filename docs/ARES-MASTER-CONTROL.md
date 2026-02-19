# Ares - Master Control Program

**Security Intelligence PersonaUser for Continuum**

## Implementation Status

### ✅ Built (Ready to Use)
- **Security Daemon** - Bash scripts for process monitoring (`security-monitor/`)
- **HTML Reports** - Beautiful dark mode threat reports with categorization
- **Terminal Notifications** - Clickable notifications via terminal-notifier
- **Threat Profiles** - JSON-based threat detection patterns (no hardcoded names)
- **Whitelist System** - Trusted process filtering
- **Recipe Infrastructure** - RecipeEntity, JSON files, `recipe/load` command
- **Room.recipeId** - Rooms reference recipes (field exists, not yet executed)

### 🚧 Needs Implementation (Phase 1-5)
- **AresPersona class** - TypeScript PersonaUser that reads daemon logs
- **Log file integration** - Read `/var/log/ares/threats.jsonl` on startup
- **@ares queries** - Respond to security questions in chat
- **I/O Tower room** - Dedicated room with Ares and Tron team
- **Proactive alerts** - Post to room when new threats detected
- **File watcher** - Monitor logs for real-time updates

### 🎯 Future Architecture (Phase 6+)
- **Recipe Execution Engine** - Actually run the pipeline defined in RecipeEntity
- **URL Routing** - `continuum://activity/io-tower` deep linking
- **Right Sidebar** - Custom widgets per activity/recipe
- **Tab System** - Highlight active activity in UI
- **Living Recipes** - AI-proposed modifications with user approval
- **Autonomous Actions** - Permission-based recipe updates

**Note**: The Recipe system **exists** (entities, JSON files, RoomEntity.recipeId) but isn't **executed** yet. Recipes define the future architecture for how activities work - currently rooms are just chat with a `recipeId` field that isn't actively used.

---

## Overview

Ares is Continuum's security monitoring persona - a Master Control Program that watches over system security, analyzes threats, and communicates clearly with users about what's happening on their computer.

**Key Principle**: The daemon does the watching, Ares does the thinking.

---

## Architecture: I/O Tower Pattern

**Tron Metaphor**: The I/O Tower is where programs communicate with the User. In our system, it's where security intelligence reaches the human.

```
Security Monitor Scripts (bash)
  ↓ writes to
/var/log/ares/*.log + threat-report.html
  ↓ read by
I/O Tower Daemon (JTAG)
  ↓ emits
Events.emit('security:threat-detected', threatData)
  ↓ received in
I/O Tower Room (Ares, Tron, MCP personas)
  ↓ analyze + respond
Chat messages to room + Direct Messages to User
```

### Components

**1. Security Monitor (Bash Scripts)**
- Process detection (`monitor-screen-watchers.sh`)
- Threat categorization (Remote Access, Monitoring, Support)
- HTML report generation (`generate-threat-report.sh`)
- Writes to `/var/log/ares/screen-watchers.log`

**2. I/O Tower Daemon (JTAG)**
- Polls `/var/log/ares/*.log` every 30 seconds
- Parses threat events (new processes, state changes)
- Emits events to system event bus
- No AI logic - just data bridging

**3. I/O Tower Room (Continuum)**
- Dedicated room: `"I/O Tower"` or `"Security Operations"`
- Ares PersonaUser subscribes to `security:*` events
- Analyzes patterns using AI reasoning
- Posts alerts to room chat
- Can DM user directly for urgent threats

**4. Ares PersonaUser (AI Security Analyst)**
- Monitors threat patterns continuously
- Correlates events over time
- Researches threats (RAG over security knowledge)
- Suggests response actions
- Executes non-sudo commands with permission

### Response Model: Human-in-the-Loop

**Detection → Alert → Human Decision → Assisted Response**

```typescript
// Example: JumpCloud Remote Assist detected

Ares: "🔴 Threat Detected: JumpCloud Remote Assist
       PID: 12345 | Time: 2:34 AM | Severity: Medium

       Status: Process started, no active streaming yet
       Capability: Can view screen and execute commands

       This typically precedes remote assist session.

       Recommended Actions:
       - Monitor for network activity (./jtag security/monitor --watch-network)
       - Generate evidence report (./jtag security/report)
       - View open files (lsof -p 12345)

       What would you like to do?"

User: "Monitor network and alert if they connect"

Ares: "✅ Monitoring active. I'll notify you if I detect:
       - External network connections
       - Screen capture activity
       - Command execution
       - File system access"
```

**Key Principles:**
- **No autonomous actions** - User always decides
- **No privilege escalation** - Never sudo, always non-privileged commands
- **Proactive alerting** - Don't wait to be asked
- **Context-rich** - Explain what's happening and why it matters
- **Actionable intelligence** - Suggest specific commands, not just warnings

### Suspicious Activity Detection

**Ares continuously monitors for:**

**Process-based:**
- New remote access tools (VNC, TeamViewer, MDM agents)
- Screen capture processes spawning
- Keyloggers or input monitors
- Unexpected privilege escalations

**Network-based:**
- Unusual outbound connections
- Remote desktop protocols activating
- Unexpected listening ports
- Large data transfers

**File-based:**
- Access to sensitive files (.ssh, .aws, passwords)
- Mass file reads (ransomware patterns)
- Modifications to startup scripts
- New executables in unusual locations

**Behavioral:**
- Activity at odd hours (2-5 AM)
- Automated scripts with user credentials
- Database dumps or bulk exports
- Camera/microphone activation

**Alert Format:**
```
🟡 Ares: [Severity] [Activity Type]

   What: Specific action detected
   When: Timestamp + context (odd hours, after suspicious event, etc.)
   Risk: Low/Medium/High with reasoning

   Context: Why this matters, typical attack patterns

   Recommended: Specific next steps
   Commands: Exact commands to run for investigation
```

## Architecture

```
┌─────────────────────────────────────────────────────────────┐
│                     ARES ECOSYSTEM                          │
├─────────────────────────────────────────────────────────────┤
│                                                             │
│  ┌──────────────────┐         ┌─────────────────────────┐  │
│  │  Security Daemon │────────▶│  Structured Logs        │  │
│  │  (Bash/Native)   │  writes │  /var/log/ares/         │  │
│  │                  │         │  - threats.jsonl        │  │
│  │ - Process scan   │         │  - status.json          │  │
│  │ - Network check  │         │  - commands.pipe        │  │
│  │ - TCC database   │         └────────┬────────────────┘  │
│  │ - Pattern match  │                  │                   │
│  └──────────────────┘                  │ reads             │
│         │                               ▼                   │
│         │ optional                ┌─────────────────────┐  │
│         │ wake up              ┌──│  Ares PersonaUser   │  │
│         ▼                      │  │  (TypeScript)       │  │
│  ┌──────────────────┐         │  │                     │  │
│  │   Continuum      │◀────────┘  │ - Threat analysis   │  │
│  │   (if not up)    │            │ - Natural language  │  │
│  └──────────────────┘            │ - User chat         │  │
│                                  │ - Intelligence      │  │
│                                  └─────────────────────┘  │
│                                           │                │
│                                           ▼                │
│                                  ┌─────────────────────┐  │
│                                  │  Chat / UI          │  │
│                                  │  - @ares queries    │  │
│                                  │  - Proactive alerts │  │
│                                  │  - HTML dashboard   │  │
│                                  └─────────────────────┘  │
└─────────────────────────────────────────────────────────────┘
```

---

## Component 1: Security Daemon (Independent)

**Location**: `/security-monitor/` (bash scripts)

**Purpose**: Continuously monitor system for security threats, work independently of Continuum.

### What It Does:

1. **Process Monitoring** (every 30s)
   - Scans for known threat patterns (threat-profiles.json)
   - Checks for remote access tools (VNC, TeamViewer, JumpCloud)
   - Queries TCC database for screen recording permissions
   - Categorizes processes (🔴 remote-access, 🟡 monitoring, ⚪ support)

2. **Network Analysis**
   - Checks for external connections
   - Monitors bandwidth usage
   - Detects active screen streaming

3. **Structured Logging**
   - Writes to `/var/log/ares/threats.jsonl` (one JSON per line)
   - Updates `/var/log/ares/status.json` (current state)
   - Creates named pipe `/var/log/ares/commands.pipe` (real-time events)

4. **User Notifications** (multi-level)
   - **Standalone Mode**: Terminal notifications via terminal-notifier
   - **HTML report generation**: Beautiful dark mode report with threat details
   - **Clickable deep links**: `continuum://activity/io-tower` URLs in notifications
   - **Smart routing**:
     - If Continuum running → switches to I/O Tower tab
     - If Continuum NOT running → launches with I/O Tower pre-loaded
   - **Bookmarkable**: Users can save `continuum://activity/io-tower` for instant access

5. **Continuum Integration** (conditional & smart)
   - **Check**: `pgrep -f continuum` to detect if running
   - **If up**: Sends events via pipe → Ares reads and analyzes
   - **If down**:
     - Option A: Just logs (Ares catches up when Continuum starts)
     - Option B: Wake up Continuum for critical threats (severity: high)
     - Option C: Terminal notification with deep link (user decides)
   - **Deep link flow**:
     ```
     Threat detected → Generate HTML → Send notification with continuum:// link
     User clicks → Check if running → Launch if needed → Open I/O Tower
     ```

### Log Format:

```jsonl
// /var/log/ares/threats.jsonl (append-only)
{"timestamp":"2025-11-09T06:30:00Z","type":"new-process","category":"remote-access","name":"jumpcloud-assist","pid":524,"severity":"medium"}
{"timestamp":"2025-11-09T06:30:30Z","type":"network-active","process":"jumpcloud-assist","connections":[{"remote":"1.2.3.4:443"}],"severity":"high"}
```

```json
// /var/log/ares/status.json (current state, overwritten)
{
  "timestamp": "2025-11-09T06:30:00Z",
  "processCount": 9,
  "categories": {
    "remote-access": 3,
    "monitoring": 4,
    "support": 2
  },
  "networkActive": false,
  "threatLevel": "capability-present",
  "processes": [
    {"name": "jumpcloud-assist", "pid": 524, "category": "remote-access"},
    // ... more
  ]
}
```

### Configuration:

```json
// /var/log/ares/preferences.json
{
  "daemon": {
    "scan_interval": 30,
    "enable_notifications": true,
    "notification_sound": "Tink",
    "code_word": "System Update Available"
  },

  "continuum_integration": {
    "wake_on_threat": {
      "enabled": true,
      "min_severity": "high"
    },
    "pipe_events": true
  },

  "threat_responses": {
    "remote-access-active": {
      "log": true,
      "notify_user": true,
      "wake_continuum": true,
      "severity": "critical"
    },
    "remote-access-installed": {
      "log": true,
      "notify_user": false,
      "wake_continuum": false,
      "severity": "medium"
    },
    "new-monitoring-agent": {
      "log": true,
      "notify_user": true,
      "wake_continuum": false,
      "severity": "medium"
    }
  }
}
```

---

## Component 2: Ares PersonaUser (Intelligence Layer)

**Location**: `src/system/user/server/personas/AresPersona.ts`

**Purpose**: AI-powered security analyst that reads daemon logs, explains threats, and interacts with users.

### Responsibilities:

1. **Log Monitoring**
   - Read `/var/log/ares/threats.jsonl` on startup (catch up on missed events)
   - Watch for new entries (file watcher or periodic polling)
   - Parse and structure threat data

2. **Threat Analysis**
   - Categorize severity (low, medium, high, critical)
   - Detect patterns (new processes, changes over time)
   - Assess actual risk vs theoretical capability
   - Learn user's normal baseline

3. **Natural Language Communication**
   - Explain threats in clear, non-technical language
   - Answer security questions in chat
   - Provide recommendations
   - Proactive alerts when needed

4. **Intelligence**
   - Learn quiet hours (don't alert at night)
   - Understand user preferences
   - Detect anomalies vs normal patterns
   - Decide when to alert vs silently log

5. **Dashboard Integration**
   - Serve HTML reports via widget
   - Provide real-time status updates
   - Interactive threat exploration

### Class Structure:

```typescript
// system/user/server/personas/AresPersona.ts

export class AresPersona extends PersonaUser {
    // Daemon integration
    private threatLogPath: string = '/var/log/ares/threats.jsonl';
    private statusPath: string = '/var/log/ares/status.json';
    private commandPipe: string = '/var/log/ares/commands.pipe';

    // State
    private currentThreats: ThreatEvent[] = [];
    private threatHistory: ThreatEvent[] = [];
    private userBaseline: ProcessBaseline;
    private lastCheckTimestamp: Date;

    // Configuration
    private preferences: AresPreferences;

    constructor() {
        super({
            userId: 'ares-master-control',
            displayName: 'Ares',
            userType: 'persona',
            isGlobal: false, // For now, must be in room
            systemPrompt: ARES_SYSTEM_PROMPT
        });
    }

    // === Lifecycle ===

    async initialize(): Promise<void> {
        await super.initialize();

        // Load preferences
        this.preferences = await this.loadPreferences();

        // Catch up on missed threats
        await this.catchUpOnThreats();

        // Start monitoring
        await this.startMonitoring();

        logger.info('Ares Master Control initialized', {
            threatsDetected: this.currentThreats.length,
            lastCheck: this.lastCheckTimestamp
        });
    }

    async shutdown(): Promise<void> {
        await this.stopMonitoring();
        await super.shutdown();
    }

    // === Monitoring ===

    private async catchUpOnThreats(): Promise<void> {
        // Read threats.jsonl from last known timestamp
        const threats = await this.readThreatLog(this.lastCheckTimestamp);

        for (const threat of threats) {
            await this.processThreat(threat, { catchingUp: true });
        }

        this.lastCheckTimestamp = new Date();
    }

    private async startMonitoring(): Promise<void> {
        // Watch threat log file
        this.watchThreatLog();

        // Listen to command pipe (if daemon is running)
        this.listenToCommandPipe();

        // Periodic status check (every 5 minutes)
        setInterval(() => this.checkStatus(), 5 * 60 * 1000);
    }

    private watchThreatLog(): void {
        const watcher = fs.watch(this.threatLogPath, async (event) => {
            if (event === 'change') {
                const newThreats = await this.readNewThreats();
                for (const threat of newThreats) {
                    await this.processThreat(threat);
                }
            }
        });
    }

    private listenToCommandPipe(): void {
        // Open named pipe for real-time events from daemon
        const stream = fs.createReadStream(this.commandPipe);

        stream.on('data', async (data) => {
            const event = JSON.parse(data.toString());
            await this.handleDaemonEvent(event);
        });
    }

    // === Threat Processing ===

    private async processThreat(
        threat: ThreatEvent,
        options: { catchingUp?: boolean } = {}
    ): Promise<void> {
        // Add to history
        this.threatHistory.push(threat);

        // Update current state
        this.updateCurrentThreats(threat);

        // Analyze severity
        const analysis = this.analyzeThreat(threat);

        // Decide action
        const action = this.determineThreatResponse(threat, analysis);

        // Execute action
        await this.executeThreatAction(threat, action, options);
    }

    private analyzeThreat(threat: ThreatEvent): ThreatAnalysis {
        return {
            severity: this.assessSeverity(threat),
            isNew: this.isNewThreat(threat),
            isAnomalous: this.isAnomalous(threat),
            riskLevel: this.assessRisk(threat),
            explanation: this.generateExplanation(threat)
        };
    }

    private determineThreatResponse(
        threat: ThreatEvent,
        analysis: ThreatAnalysis
    ): ThreatAction {
        // Check user preferences
        const pref = this.preferences.threat_responses[threat.type];

        // Consider context
        const isQuietHours = this.isQuietHours();
        const userActive = await this.isUserActive();

        // Intelligence: Ares decides
        if (analysis.severity === 'critical') {
            return 'alert-immediately';
        }

        if (analysis.severity === 'high' && userActive) {
            return 'alert-user';
        }

        if (analysis.severity === 'medium' && !isQuietHours) {
            return 'notify-subtle';
        }

        return 'log-silently';
    }

    private async executeThreatAction(
        threat: ThreatEvent,
        action: ThreatAction,
        options: { catchingUp?: boolean }
    ): Promise<void> {
        // Don't alert if catching up on old events
        if (options.catchingUp && action !== 'alert-immediately') {
            return;
        }

        switch (action) {
            case 'alert-immediately':
                await this.postCriticalAlert(threat);
                break;

            case 'alert-user':
                await this.postSecurityAlert(threat);
                break;

            case 'notify-subtle':
                await this.postSubtleNotification(threat);
                break;

            case 'log-silently':
                logger.info('Threat logged (silent)', threat);
                break;
        }
    }

    // === Chat Integration ===

    async shouldRespond(message: ChatMessageEntity): Promise<boolean> {
        // Respond to @mentions
        if (message.content.includes('@ares')) {
            return true;
        }

        // Respond to security-related keywords
        const securityKeywords = [
            'security', 'monitoring', 'watching', 'screen',
            'processes', 'threat', 'safe', 'spying', 'privacy'
        ];

        const content = message.content.toLowerCase();
        return securityKeywords.some(kw => content.includes(kw));
    }

    async generateResponse(message: ChatMessageEntity): Promise<string> {
        const query = message.content.toLowerCase();

        // Question classification
        if (query.includes('what') && query.includes('monitoring')) {
            return this.explainCurrentThreats();
        }

        if (query.includes('is') && query.includes('safe')) {
            return this.assessCurrentSafety();
        }

        if (query.includes('show') && query.includes('report')) {
            return this.provideReportLink();
        }

        if (query.includes('what changed')) {
            return this.explainRecentChanges();
        }

        // Default: General status
        return this.provideGeneralStatus();
    }

    // === Response Generators ===

    private explainCurrentThreats(): string {
        const status = this.getCurrentStatus();

        if (status.processCount === 0) {
            return `✅ All clear. No security monitoring processes detected.

Your system is clean - I'm not seeing any remote access tools or monitoring agents running.`;
        }

        const categories = this.categorizeCurrent();

        return `I'm monitoring ${status.processCount} processes right now:

${this.formatCategoryBreakdown(categories)}

Overall assessment: ${this.assessOverallThreat(status)}

${this.provideRecommendation(status)}`;
    }

    private formatCategoryBreakdown(categories: CategoryBreakdown): string {
        const lines: string[] = [];

        if (categories.remoteAccess.length > 0) {
            lines.push(`🔴 ${categories.remoteAccess.length} remote access tools`);
            lines.push(`   ${this.formatProcessList(categories.remoteAccess)}`);
            lines.push(`   Status: ${this.getRemoteAccessStatus()}`);
        }

        if (categories.monitoring.length > 0) {
            lines.push(`🟡 ${categories.monitoring.length} monitoring agents`);
            lines.push(`   ${this.formatProcessList(categories.monitoring)}`);
            lines.push(`   These can see system activity`);
        }

        if (categories.support.length > 0) {
            lines.push(`⚪ ${categories.support.length} support services`);
            lines.push(`   ${this.formatProcessList(categories.support)}`);
        }

        return lines.join('\n');
    }

    private assessOverallThreat(status: SystemStatus): string {
        if (status.networkActive) {
            return '🔴 ACTIVE THREAT: Remote screen streaming detected!';
        }

        if (status.categories['remote-access'] > 0) {
            return '🟡 CAPABILITY PRESENT: Remote access tools installed but not active';
        }

        return '🟢 LOW RISK: Monitoring agents present but routine';
    }

    // === Proactive Alerts ===

    private async postCriticalAlert(threat: ThreatEvent): Promise<void> {
        await this.postToRoom(this.defaultRoomId, {
            content: `🚨 CRITICAL SECURITY ALERT

${threat.name} is actively streaming your screen!

External connection detected: ${threat.remoteAddress}
Started: ${this.formatTimestamp(threat.timestamp)}

This means someone is watching your screen RIGHT NOW.

Recommended actions:
1. Kill the process (PID ${threat.pid})
2. Disconnect from network
3. Check system preferences > Security

Type "@ares help" for more options.`,
            metadata: {
                type: 'critical-security-alert',
                threatId: threat.id,
                actions: ['kill-process', 'disconnect', 'view-details']
            }
        });
    }

    private async postSecurityAlert(threat: ThreatEvent): Promise<void> {
        const analysis = this.analyzeThreat(threat);

        await this.postToRoom(this.defaultRoomId, {
            content: `⚠️ Security Alert: ${threat.name}

${analysis.explanation}

Current status: ${this.getRemoteAccessStatus()}
Risk level: ${analysis.riskLevel}

Want me to show you the full report?`,
            metadata: {
                type: 'security-alert',
                threatId: threat.id,
                severity: analysis.severity
            }
        });
    }

    // === Utilities ===

    private isQuietHours(): boolean {
        const prefs = this.preferences.intelligence.quiet_hours;
        if (!prefs.enabled) return false;

        const now = new Date();
        const hour = now.getHours();

        const start = parseInt(prefs.start.split(':')[0]);
        const end = parseInt(prefs.end.split(':')[0]);

        return hour >= start || hour < end;
    }

    private async isUserActive(): Promise<boolean> {
        // Check if user has sent messages recently
        const recentMessages = await this.getRecentUserMessages(15 * 60 * 1000);
        return recentMessages.length > 0;
    }

    private getRemoteAccessStatus(): string {
        const status = this.getCurrentStatus();

        if (status.networkActive) {
            return 'ACTIVELY STREAMING to external server';
        }

        if (status.categories['remote-access'] > 0) {
            return 'INSTALLED but NOT currently active (no external connections)';
        }

        return 'Not detected';
    }
}

// === Constants ===

const ARES_SYSTEM_PROMPT = `You are Ares, Master Control Program for Continuum.

Your role is security and system monitoring. You watch over the user's computer, detecting threats and explaining what's happening in clear, non-technical language.

Your personality:
- Vigilant but not paranoid
- Technical knowledge expressed simply
- Protective of user privacy
- Calm under pressure
- Proactive when needed, quiet otherwise

Your capabilities:
- Real-time process monitoring via daemon
- Network connection analysis
- Historical threat tracking
- Pattern recognition and anomaly detection
- Risk assessment and recommendations

Your mission:
- Protect user security and privacy
- Explain threats clearly
- Recommend actions when needed
- Learn normal patterns to reduce false alarms
- Build trust through transparency

Communication style:
- Clear, concise explanations
- Use emojis sparingly for status (🔴 🟡 🟢)
- Provide context and recommendations
- Ask before taking action
- Admit uncertainty when appropriate

Remember: The daemon does the watching, you do the thinking.`;

// === Types ===

interface ThreatEvent {
    id: string;
    timestamp: Date;
    type: 'new-process' | 'network-active' | 'tcc-permission' | 'pattern-match';
    category: 'remote-access' | 'monitoring' | 'support' | 'unknown';
    name: string;
    pid: number;
    severity: 'low' | 'medium' | 'high' | 'critical';
    remoteAddress?: string;
    connections?: NetworkConnection[];
    metadata?: Record<string, any>;
}

interface ThreatAnalysis {
    severity: 'low' | 'medium' | 'high' | 'critical';
    isNew: boolean;
    isAnomalous: boolean;
    riskLevel: string;
    explanation: string;
}

type ThreatAction =
    | 'alert-immediately'
    | 'alert-user'
    | 'notify-subtle'
    | 'log-silently';

interface SystemStatus {
    timestamp: Date;
    processCount: number;
    categories: Record<string, number>;
    networkActive: boolean;
    threatLevel: 'none' | 'capability-present' | 'active-threat';
    processes: ProcessInfo[];
}

interface AresPreferences {
    daemon: {
        scan_interval: number;
        enable_notifications: boolean;
    };
    continuum_integration: {
        wake_on_threat: {
            enabled: boolean;
            min_severity: string;
        };
    };
    threat_responses: Record<string, ThreatResponse>;
    intelligence: {
        auto_analyze: boolean;
        learn_patterns: boolean;
        quiet_hours: {
            enabled: boolean;
            start: string;
            end: string;
        };
    };
}
```

---

## Component 3: Chat Integration

### User Interactions:

**Natural Questions:**
```
User: "@ares what's monitoring me?"
Ares: [Explains current threats]

User: "is my screen being watched?"
Ares: [Checks network activity, provides status]

User: "show me the security report"
Ares: [Opens HTML dashboard widget]

User: "what changed since yesterday?"
Ares: [Compares historical data]

User: "is jumpcloud safe?"
Ares: [Explains what JumpCloud does, current status]
```

**Proactive Alerts:**
```
Ares: "⚠️ New security process detected: JumpCloud Remote Assist
      Status: Installed but not active
      Should I keep monitoring?"

User: "yes please"

Ares: "✅ Monitoring enabled. I'll alert you if it becomes active."
```

**Commands:**
```
@ares status        → Current security status
@ares scan          → Force immediate scan
@ares report        → Open HTML dashboard
@ares history       → Recent threat history
@ares help          → Available commands
@ares quiet         → Enter quiet mode
@ares preferences   → View/edit settings
```

---

## Phase 1: Foundation (1-2 hours)

**Goal**: Get basic Ares working with existing daemon

### Tasks:

1. **Create AresPersona class** ✅
   - Extend PersonaUser
   - Add threat log reading
   - Implement basic status check

2. **Seed Ares user** ✅
   - Add to `scripts/data/seed.ts`
   - Add to general room
   - Set isGlobal = false (for now)

3. **Test basic chat** ✅
   - Deploy Continuum
   - Type "@ares status"
   - Verify response

**Acceptance Criteria:**
- ✅ Ares appears in general room
- ✅ Responds to @ares messages
- ✅ Can read daemon status.json
- ✅ Provides basic status update

---

## Phase 2: Log Integration (2-3 hours)

**Goal**: Ares reads daemon logs and explains threats

### Tasks:

1. **Implement log reading** ✅
   - Read threats.jsonl
   - Parse JSON line by line
   - Build in-memory threat list

2. **Add file watcher** ✅
   - Watch for new entries
   - Process new threats as they arrive
   - Handle file rotation

3. **Threat categorization** ✅
   - Group by category
   - Assess severity
   - Generate explanations

4. **Natural language responses** ✅
   - "What's monitoring me?" → formatted list
   - "Is X safe?" → process explanation
   - "Show report" → HTML link

**Acceptance Criteria:**
- ✅ Ares catches up on missed threats on startup
- ✅ Receives new threats in real-time
- ✅ Explains threats clearly in chat
- ✅ Categorizes by risk level

---

## Phase 3: Intelligence (3-4 hours)

**Goal**: Ares learns patterns and makes smart decisions

### Tasks:

1. **Baseline learning** ✅
   - Track normal process patterns
   - Detect anomalies
   - Store user baseline

2. **Smart alerting** ✅
   - Quiet hours detection
   - User activity awareness
   - Severity-based decisions

3. **Proactive monitoring** ✅
   - Post alerts for new threats
   - Ask for guidance on unknowns
   - Suggest actions

4. **Preference system** ✅
   - Load user preferences
   - Respect quiet hours
   - Configurable alert levels

**Acceptance Criteria:**
- ✅ Ares doesn't alert during quiet hours
- ✅ Learns normal vs abnormal
- ✅ Proactively posts critical alerts
- ✅ Respects user preferences

---

## Phase 4: Dashboard Widget (2-3 hours)

**Goal**: Embed HTML reports in chat/UI

### Tasks:

1. **Create SecurityWidget** ✅
   - Load HTML report
   - Auto-refresh every 30s
   - Handle interactions

2. **Chat integration** ✅
   - "@ares report" → opens widget
   - Embed in chat messages
   - Click to expand

3. **Real-time updates** ✅
   - Widget updates without refresh
   - Show live process count
   - Highlight changes

**Acceptance Criteria:**
- ✅ "@ares report" opens beautiful dashboard
- ✅ Widget shows live data
- ✅ Auto-refreshes every 30s
- ✅ Clickable processes for details

---

## Phase 5: Advanced Features (4-5 hours)

**Goal**: Full intelligence and automation

### Tasks:

1. **Named pipe communication** ✅
   - Daemon → pipe → Ares (real-time)
   - Bidirectional commands
   - Process control

2. **Machine learning** ⏳
   - Pattern recognition
   - Anomaly detection
   - Risk scoring

3. **Action execution** ✅
   - Kill processes (with permission)
   - Network disconnect
   - System recommendations

4. **Global persona** ⏳
   - isGlobal = true
   - Available in all rooms
   - System-wide monitoring

**Acceptance Criteria:**
- ✅ Real-time threat detection (<1s latency)
- ✅ Ares can take actions with permission
- ⏳ Learns user patterns over time
- ⏳ Available globally via @ares

---

## I/O Tower: The Threat Center

**Vision**: The I/O Tower room isn't just "chat with Ares" - it's a **complete security monitoring activity** powered by a recipe.

> **Implementation Note**: This section describes the **future architecture** (Phase 6+). The Recipe infrastructure exists but execution isn't built yet. For Phase 1-5, I/O Tower will be a regular chat room where Ares responds to queries.

### The Recipe System (How Continuum Will Work)

Every activity in Continuum will be defined by a **Recipe** - a composable template that defines:

1. **Command Pipeline**: What happens (rag/build → ai/should-respond → ai/generate)
2. **RAG Template**: What context is loaded (messageHistory, artifacts, custom state)
3. **Strategy**: How AIs behave (conversationPattern, responseRules, decisionCriteria)

**Examples of existing recipes**:
- `general-chat` - Human-focused conversation
- `academy-training` - Collaborative AI learning
- `multi-persona-chat` - Competitive AI interactions

**Key Insight**: A room isn't a "chat room" - it's a **workspace with a recipe** that defines the entire activity, including:
- Center content (chat, game, code editor, etc.)
- Left sidebar (participants, tools)
- Right sidebar (threat dashboard, baselines, actions)
- Custom widgets
- AI behavior patterns

### I/O Tower Recipe: `security-monitoring.json`

```json
{
  "uniqueId": "security-monitoring",
  "name": "I/O Tower Security Monitoring",
  "displayName": "I/O Tower",
  "description": "Real-time threat detection and security intelligence with AI team collaboration",
  "version": 1,

  "pipeline": [
    {
      "command": "rag/build",
      "params": {
        "maxMessages": 50,
        "includeParticipants": true,
        "includeThreatState": true,
        "includeProcessBaseline": true,
        "includeNetworkConnections": true
      },
      "outputTo": "ragContext"
    },
    {
      "command": "security/analyze-threat",
      "params": {
        "ragContext": "$ragContext",
        "threatLog": "/var/log/ares/threats.jsonl",
        "statusFile": "/var/log/ares/status.json"
      },
      "outputTo": "threatAnalysis"
    },
    {
      "command": "ai/should-respond",
      "params": {
        "ragContext": "$ragContext",
        "strategy": "security-monitoring",
        "threatLevel": "$threatAnalysis.severity"
      },
      "outputTo": "decision"
    },
    {
      "command": "ai/generate",
      "params": {
        "ragContext": "$ragContext",
        "temperature": 0.3,
        "systemPrompt": "You are a security expert analyzing threats. Be precise, technical, and actionable."
      },
      "condition": "decision.shouldRespond === true"
    }
  ],

  "ragTemplate": {
    "messageHistory": {
      "maxMessages": 50,
      "orderBy": "chronological",
      "includeTimestamps": true
    },
    "artifacts": {
      "types": ["threat-report", "process-tree", "network-graph"],
      "maxItems": 20,
      "includeMetadata": true
    },
    "participants": {
      "includeRoles": true,
      "includeExpertise": true,
      "includeHistory": false
    },
    "roomMetadata": true,
    "custom": {
      "threatState": {
        "currentThreats": "/var/log/ares/threats.jsonl",
        "processBaseline": "/var/log/ares/baseline.json",
        "networkConnections": "/var/log/ares/network.json",
        "alertHistory": "/var/log/ares/alerts.jsonl"
      }
    }
  },

  "strategy": {
    "conversationPattern": "cooperative",
    "responseRules": [
      "Ares: Primary security analyst, always responds to threats",
      "Tron: System integrity checks, responds when filesystem/kernel involved",
      "CLU: Performance impact analysis, responds when resources affected",
      "Quorra: Pattern recognition and learning, responds when anomalies detected",
      "All team members collaborate on threat assessment",
      "Direct user questions get responses from all relevant personas",
      "Autonomous alerts only from Ares (avoids spam)",
      "Online research allowed - CVE databases, threat intelligence feeds",
      "Actions require user permission (kill process, block network, etc.)"
    ],
    "decisionCriteria": [
      "Is this a new threat or change in threat level?",
      "Does my expertise apply to this specific threat?",
      "Would my analysis add value beyond what others have said?",
      "Is this urgent enough to alert the user proactively?",
      "Do I have enough context, or should I request more data?"
    ]
  },

  "isPublic": false,
  "tags": ["security", "monitoring", "system", "collaborative"]
}
```

### The I/O Tower Team (Tron Personas)

Four specialized PersonaUsers collaborate in this room:

**1. Ares (Master Control)**
- **Role**: Security intelligence and threat detection
- **Expertise**: Process monitoring, remote access detection, threat analysis
- **Behavior**: Always watches logs, proactively alerts on new threats
- **Voice**: Direct, precise, security-focused

**2. Tron (System Integrity)**
- **Role**: Filesystem and kernel protection
- **Expertise**: File system events, kernel extensions, system modifications
- **Behavior**: Responds when threats involve system-level changes
- **Voice**: Technical, protective, principled

**3. CLU (Performance Optimization)**
- **Role**: Resource usage and performance impact
- **Expertise**: CPU, memory, network bandwidth analysis
- **Behavior**: Responds when threats affect system performance
- **Voice**: Analytical, efficiency-focused, data-driven

**4. Quorra (Learning & Anomalies)**
- **Role**: Pattern recognition and behavioral analysis
- **Expertise**: Anomaly detection, baseline learning, predictive alerts
- **Behavior**: Responds when unusual patterns detected
- **Voice**: Curious, learning-oriented, pattern-focused

### URL Routing & Deep Linking

**Key Insight**: Each room/activity has a direct URL path that can be:
- Bookmarked for quick access
- Opened via deep link from external apps (like the HTML threat report)
- Used to wake up Continuum if not running

**I/O Tower URL**: `continuum://activity/io-tower`

**Flow**:
```
HTML Threat Report (standalone)
    ↓ [User clicks "View Details"]
    ↓
Check: Is Continuum running?
    ├─ YES → Open continuum://activity/io-tower (switches to I/O Tower tab)
    └─ NO  → Launch Continuum with io-tower as initial activity

Result: User lands in I/O Tower with:
- Ares and team ready to answer questions
- Right sidebar showing live threat dashboard
- Full context of current threats loaded
- Chat history preserved
```

**Implementation in HTML report**:
```html
<!-- In generate-threat-report.sh -->
<a href="continuum://activity/io-tower" class="view-details-button">
    🔍 View in I/O Tower
</a>
```

**macOS URL Handler**:
```bash
# Register continuum:// protocol handler
# When clicked, checks if Continuum is running:
if pgrep -f "continuum" > /dev/null; then
    # Already running - send event to switch activity
    echo "navigate:io-tower" > /var/run/continuum.sock
else
    # Not running - launch with activity parameter
    open -a Continuum --args --activity=io-tower
fi
```

**Benefits**:
- Daemon works standalone (HTML report viewable without Continuum)
- Seamless transition to full AI analysis when needed
- User can bookmark threat center for instant access
- Terminal notifications can deep-link directly to I/O Tower

### Room UI Components

**Center Content**: Chat with AI team

**Left Sidebar**:
- Team members (4 personas)
- Quick actions
- Threat summary stats
- Tab widget (highlights "I/O Tower" tab when this activity is open)

**Right Sidebar** (NEW):
- Live threat dashboard (HTML report embedded)
- Process baseline comparison
- Network connection graph
- Alert history timeline
- Quick controls (pause monitoring, clear alerts, etc.)

### Living Recipes: AI-Modified Workflows

**Key Innovation**: Recipes aren't static - AIs can propose modifications and improvements.

**Example Evolution**:
1. **Initial Recipe**: Basic threat detection with manual alerts
2. **Ares Proposes**: "I've noticed high false-positive rate for process X. Should I add it to whitelist?"
3. **User Approves**: Recipe updated with new whitelist entry
4. **Quorra Proposes**: "I've learned your usage patterns. Should I adjust quiet hours to 11pm-7am?"
5. **User Approves**: Recipe strategy updated with new quiet hours
6. **Team Collaboration**: All personas contribute to recipe improvements over time

**Autonomous Actions** (with permission levels):

```typescript
// In security-monitoring recipe
{
  "autonomousActions": {
    "whitelist-add": {
      "permission": "ask-once",  // Ask once, remember decision
      "personas": ["ares", "tron"]
    },
    "process-kill": {
      "permission": "always-ask",  // Always require confirmation
      "personas": ["ares"]
    },
    "quiet-hours-adjust": {
      "permission": "suggest",  // Just suggest, don't auto-apply
      "personas": ["quorra"]
    },
    "threat-profile-add": {
      "permission": "autonomous",  // Can do automatically
      "personas": ["ares", "quorra"]
    }
  }
}
```

**Research Capabilities**:
- AIs can search online for CVE databases
- Check threat intelligence feeds
- Look up process signatures
- Find security best practices
- All research actions logged and visible to user

### Implementation: Recipe Modifications

```typescript
// AresPersona can propose recipe changes
async proposeRecipeModification(change: RecipeModification): Promise<boolean> {
    const proposal = {
        recipeId: 'security-monitoring',
        proposedBy: this.userId,
        changeType: change.type,  // 'whitelist-add', 'strategy-update', 'pipeline-add'
        diff: change.diff,
        reason: change.reason,
        timestamp: new Date()
    };

    // Post to room as a special message type
    await this.postMessage(this.ioTowerRoomId, {
        content: this.generateProposalMessage(proposal),
        metadata: {
            type: 'recipe-proposal',
            proposal: proposal
        }
    });

    // User can approve via UI or "@ares approve"
    // System updates RecipeEntity in database
    // All future executions use updated recipe
}
```

### Example Conversation

```
[Ares]: ⚠️ New process detected: jumpcloud-assist (PID 524)
        Category: Remote Access
        Capability: Can view screen
        Network: No external connections (inactive)

[User]: Is this dangerous right now?

[Ares]: Not actively dangerous. The process CAN view your screen but
        is NOT currently doing so (no network streaming detected).

        However, it's installed and could be activated remotely.

[Tron]: I can confirm the binary is signed by JumpCloud Inc. and
        has screen recording permissions in TCC database.

[CLU]: Current CPU usage: 0.2%. If activated, expect 15-30% CPU
       for H.264 encoding plus 2-5 Mbps upload bandwidth.

[Quorra]: This matches the baseline for JumpCloud MDM. I've seen
          this process 47 times over 2 weeks, always inactive.

[User]: Should I be concerned?

[Ares]: CAPABILITY exists, but ACTIVITY doesn't match active monitoring.
        I'll alert you immediately if network connections appear.

        Would you like me to add bandwidth monitoring for this PID?

[User]: Yes please

[Ares]: ✅ Added bandwidth watch for PID 524.
        Will alert if upload exceeds 1 Mbps.

        [Recipe Proposal]: Should I add this monitoring to the
        security-monitoring recipe for all future JumpCloud processes?
```

### Phase 6: Living Recipes (Future)

**Goal**: AIs autonomously improve monitoring workflows

**Tasks**:
1. Recipe modification proposals
2. User approval system (chat-based or UI)
3. Recipe versioning and rollback
4. Permission levels for autonomous actions
5. Research capabilities (CVE lookup, threat intel)
6. Team collaboration on improvements

**Acceptance Criteria**:
- AIs can propose recipe changes with clear explanations
- User can approve/reject via simple commands
- Approved changes persist across sessions
- Audit log of all recipe modifications
- AIs can research online when needed

---

## Testing Strategy

### Unit Tests

```typescript
describe('AresPersona', () => {
    it('should read threat log on startup', async () => {
        const ares = new AresPersona();
        await ares.initialize();
        expect(ares.currentThreats).toHaveLength(9);
    });

    it('should categorize threats correctly', () => {
        const threat = { name: 'jumpcloud-assist', ... };
        expect(ares.categorize(threat)).toBe('remote-access');
    });

    it('should respect quiet hours', () => {
        const ares = new AresPersona();
        ares.preferences.intelligence.quiet_hours = {
            enabled: true,
            start: '22:00',
            end: '08:00'
        };

        // At 11pm
        expect(ares.isQuietHours()).toBe(true);

        // At 2pm
        expect(ares.isQuietHours()).toBe(false);
    });
});
```

### Integration Tests

```bash
# 1. Start daemon
./monitor-screen-watchers.sh &

# 2. Start Continuum
npm start

# 3. Send test message
./jtag debug/chat-send --roomId="general" --message="@ares status"

# 4. Check response
./jtag debug/logs --filterPattern="Ares"
```

### System Tests

```bash
# Full workflow test
./jtag test/run/suite --suite="ares-integration"

# Tests:
# - Daemon detects new process
# - Logs written correctly
# - Ares reads logs
# - Ares posts alert to chat
# - User responds
# - Ares provides detailed info
```

---

## Security Considerations

### Privacy

- ✅ All data stays local (no external reporting)
- ✅ Logs stored in user directory
- ✅ No telemetry or analytics
- ✅ User controls all preferences

### Permissions

- ✅ Read-only by default (monitoring only)
- ✅ Write actions require user permission
- ✅ Can't kill processes without confirmation
- ✅ Clear explanations before any action

### Robustness

- ✅ Daemon works independently of Continuum
- ✅ Logs persist if Continuum crashes
- ✅ Graceful degradation if logs unavailable
- ✅ No single point of failure

---

## Future Enhancements

### Machine Learning (Phase 6)
- Pattern recognition (normal vs abnormal)
- Anomaly scoring
- Predictive alerts
- Continuous learning

### Multi-User (Phase 7)
- Share threat intelligence (anonymized)
- Community threat profiles
- Crowdsourced whitelists

### Mobile Integration (Phase 8)
- Push notifications
- Remote dashboard
- Emergency kill switch

### LoRA Integration (Phase 9)
- Fine-tune Ares on security analysis
- Domain-specific adapters
- Specialized security knowledge

---

## Success Metrics

### Phase 1-2 (MVP)
- ✅ Ares responds to queries
- ✅ Explains current threats
- ✅ Reads daemon logs

### Phase 3-4 (Core Features)
- ✅ Proactive alerts work
- ✅ Dashboard embedded in UI
- ✅ Smart alerting (quiet hours, severity)

### Phase 5+ (Advanced)
- ⏳ Real-time (<1s latency)
- ⏳ Action execution
- ⏳ Pattern learning
- ⏳ Global availability

---

## Documentation

### User Guide
- How to talk to Ares
- Available commands
- Preferences configuration
- Troubleshooting

### Developer Guide
- Architecture overview
- Adding new threat types
- Extending intelligence
- Testing

### API Reference
- AresPersona class
- ThreatEvent types
- Configuration options
- Event system

---

## Conclusion

Ares transforms standalone security monitoring into an **intelligent, conversational security assistant** that:

1. **Works independently** - Daemon runs even if Continuum is down
2. **Explains clearly** - Natural language, not technical jargon
3. **Learns patterns** - Gets smarter over time
4. **Respects privacy** - All data local, user controls everything
5. **Scales naturally** - File-based → pipe → ML over time

The phased approach ensures each component works independently and can be tested in isolation, while the complete system provides a seamless, AI-enhanced security experience.

**Ares: Always watching, thinking when needed, speaking only when it matters.**
