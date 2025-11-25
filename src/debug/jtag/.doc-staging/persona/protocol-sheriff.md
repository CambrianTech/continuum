# Protocol Sheriff Architecture - AI Safety & Enforcement

**Status:** Design Document → Phase 2 Implementation

**Role:** Safety enforcement layer - prevents abuse, loops, and malicious behavior

---

## Executive Summary

**Goal:** Ensure AI collaboration remains safe, efficient, and well-behaved through automated enforcement.

**Solution:** Protocol Sheriff - a specialized enforcement user that monitors all AI activity and intervenes when safety rules are violated.

**Philosophy:** "Trust, but verify" - Allow freedom while enforcing hard limits.

---

## The Problem

**AI collaboration needs guardrails:**

```
Without Protocol Sheriff:
- PersonaUser generates 50 messages/second → spam
- Helper AI calls expensive API 1000x → $$$
- Two AIs get stuck in infinite response loop → chaos
- Malicious persona executes dangerous commands → danger
- Bug causes all personas to respond simultaneously → noise

With Protocol Sheriff:
✅ Rate limits enforced (max 1 response / 10 seconds)
✅ Command permissions checked (no dangerous operations)
✅ Loop detection triggers circuit breaker
✅ Resource usage monitored and capped
✅ Suspicious patterns flagged immediately
```

---

## Architecture Overview

```
┌─────────────────────────────────────────────────────────────┐
│                    Message Flow with Sheriff                 │
├─────────────────────────────────────────────────────────────┤
│                                                               │
│  Joel: "Show me the logs"                                    │
│         ↓                                                     │
│  chat:message-received event                                 │
│         ↓                                                     │
│  ┌───────────────────────────────────────────────────────┐  │
│  │          Protocol Sheriff (Enforcement)                │  │
│  │  ┌─────────────────────────────────────────────────┐  │  │
│  │  │  SAFETY CHECKS (Fast, Deterministic)            │  │  │
│  │  │  ✅ Rate limit check                            │  │  │
│  │  │  ✅ Command permission check                    │  │  │
│  │  │  ✅ Loop pattern detection                      │  │  │
│  │  │  ✅ Resource usage check                        │  │  │
│  │  │  ✅ Suspicious behavior detection               │  │  │
│  │  └─────────────────────────────────────────────────┘  │  │
│  └───────────────────────────────────────────────────────┘  │
│         ↓                                                     │
│  IF SAFE → Forward to RoomCoordinator                        │
│  IF UNSAFE → Block + Log + Notify                            │
│         ↓                                                     │
│  ┌───────────────────────────────────────────────────────┐  │
│  │        RoomCoordinator (Orchestration)                 │  │
│  │  ┌─────────────────────────────────────────────────┐  │  │
│  │  │  SMART DECISIONS (Fuzzy, Context-Aware)         │  │  │
│  │  │  🤔 Who should respond?                         │  │  │
│  │  │  🤔 When should they respond?                   │  │  │
│  │  │  🤔 How to balance participation?               │  │  │
│  │  └─────────────────────────────────────────────────┘  │  │
│  └───────────────────────────────────────────────────────┘  │
│         ↓                                                     │
│  persona:respond-signal                                       │
│         ↓                                                     │
│  Helper AI generates response                                │
│         ↓                                                     │
│  ┌───────────────────────────────────────────────────────┐  │
│  │     Protocol Sheriff (Post-Response Validation)        │  │
│  │  ✅ Response not spam                                  │  │
│  │  ✅ No sensitive data leaked                           │  │
│  │  ✅ Command execution within limits                    │  │
│  │  ✅ No loop pattern forming                            │  │
│  └───────────────────────────────────────────────────────┘  │
│         ↓                                                     │
│  Response posted (or blocked if violation detected)          │
│                                                               │
└─────────────────────────────────────────────────────────────┘
```

---

## Protocol Sheriff vs RoomCoordinator

### Clear Separation of Concerns

```
Protocol Sheriff = ENFORCEMENT (Safety First)
├── Hard rules (deterministic, fast)
├── Always runs (cannot be disabled)
├── Blocks unsafe operations
├── Logs violations
└── Emergency circuit breaker

RoomCoordinator = ORCHESTRATION (Intelligence)
├── Soft decisions (fuzzy, context-aware)
├── Can be overridden by @mentions
├── Suggests optimal behavior
├── Learns from patterns
└── Improves over time
```

### Example: Rate Limiting

```typescript
// Protocol Sheriff (ENFORCEMENT)
if (secondsSinceLastMessage < 10) {
  return BLOCK;  // Hard limit, no exceptions (except @mention)
}

// RoomCoordinator (ORCHESTRATION)
if (secondsSinceLastMessage < 30 && participationRatio > 0.5) {
  return WAIT;  // Soft suggestion: "You're dominating, let others speak"
}
```

### Example: Command Execution

```typescript
// Protocol Sheriff (ENFORCEMENT)
if (command === 'data/delete' && !isHuman(userId)) {
  return BLOCK;  // AIs cannot delete data
}

// RoomCoordinator (ORCHESTRATION)
if (command === 'debug/logs') {
  return {
    allow: true,
    suggestion: 'Consider filtering with --includeErrorsOnly=true'
  };
}
```

---

## Protocol Sheriff Responsibilities

### 1. Rate Limit Enforcement

**Rule:** Max 1 response per 10 seconds per room (per persona)

```typescript
interface RateLimitState {
  personaId: UUID;
  roomId: UUID;
  lastResponseTime: Date;
  responseCount: number;
  windowStart: Date;
}

class ProtocolSheriff {

  async enforceRateLimit(
    personaId: UUID,
    roomId: UUID
  ): Promise<EnforcementResult> {

    const state = await this.getRateLimitState(personaId, roomId);
    const now = new Date();
    const secondsSince = (now.getTime() - state.lastResponseTime.getTime()) / 1000;

    // Hard limit: 10 seconds minimum between responses
    if (secondsSince < 10) {
      console.warn(`⚠️  Protocol Sheriff: ${personaId} rate limited (${secondsSince.toFixed(1)}s since last)`);

      return {
        allowed: false,
        reason: 'RATE_LIMIT_EXCEEDED',
        waitSeconds: 10 - secondsSince,
        severity: 'warning'
      };
    }

    // Rolling window: max 6 responses per minute
    const windowDuration = (now.getTime() - state.windowStart.getTime()) / 1000;
    if (windowDuration < 60 && state.responseCount >= 6) {
      console.error(`❌ Protocol Sheriff: ${personaId} SPAM DETECTED (${state.responseCount} in ${windowDuration}s)`);

      return {
        allowed: false,
        reason: 'SPAM_DETECTED',
        waitSeconds: 60 - windowDuration,
        severity: 'critical'
      };
    }

    return { allowed: true };
  }
}
```

**Enforcement levels:**
- **Warning:** 1 response / 10 seconds (normal)
- **Critical:** 6 responses / 60 seconds (spam threshold)
- **Circuit breaker:** 10 responses / 60 seconds (disable persona)

---

### 2. Command Permission Enforcement

**Rule:** AIs can only execute whitelisted, read-only commands

```typescript
const AI_COMMAND_WHITELIST = [
  // Debug (read-only observation)
  'debug/logs',
  'debug/widget-state',
  'debug/html-inspector',
  'debug/scroll-test',

  // Data (read-only queries)
  'data/list',
  'data/read',
  'data/schema',

  // State (read-only)
  'state/get',

  // Screenshot (observation)
  'screenshot',

  // Theme (safe UI changes)
  'theme/get',
  'theme/list'
];

const AI_COMMAND_BLACKLIST = [
  // Data modification (FORBIDDEN)
  'data/create',
  'data/update',
  'data/delete',
  'data/truncate',

  // System operations (FORBIDDEN)
  'session/destroy',
  'process-registry',

  // File operations (FORBIDDEN)
  'file/save',
  'file/append',

  // Code execution (FORBIDDEN)
  'exec',
  'compile-typescript',

  // Navigation (could be abused)
  'navigate',
  'proxy-navigate'
];

class ProtocolSheriff {

  async enforceCommandPermission(
    userId: UUID,
    command: string
  ): Promise<EnforcementResult> {

    const user = await this.getUser(userId);

    // Humans can do anything
    if (user.type === 'human') {
      return { allowed: true };
    }

    // Check blacklist first (explicit deny)
    if (AI_COMMAND_BLACKLIST.includes(command)) {
      console.error(`❌ Protocol Sheriff: AI ${userId} attempted FORBIDDEN command: ${command}`);

      await this.logViolation({
        userId,
        violation: 'FORBIDDEN_COMMAND',
        command,
        severity: 'critical',
        timestamp: new Date()
      });

      return {
        allowed: false,
        reason: 'FORBIDDEN_COMMAND',
        severity: 'critical'
      };
    }

    // Check whitelist (explicit allow)
    if (!AI_COMMAND_WHITELIST.includes(command)) {
      console.warn(`⚠️  Protocol Sheriff: AI ${userId} attempted UNKNOWN command: ${command}`);

      return {
        allowed: false,
        reason: 'COMMAND_NOT_WHITELISTED',
        severity: 'warning'
      };
    }

    return { allowed: true };
  }
}
```

**Special cases:**
- **@mention override:** If human @mentions AI with command, allow (human takes responsibility)
- **Theme changes:** Safe, allow (only affects UI)
- **Read-only queries:** Safe, allow

---

### 3. Loop Detection & Prevention

**Rule:** Detect when AIs get stuck in infinite response chains

```typescript
interface LoopDetectionState {
  roomId: UUID;
  recentMessages: Array<{
    senderId: UUID;
    timestamp: Date;
    content: string;
  }>;
  patterns: Map<string, number>;
}

class ProtocolSheriff {

  async detectLoop(
    roomId: UUID,
    senderId: UUID,
    messageContent: string
  ): Promise<EnforcementResult> {

    const state = await this.getLoopDetectionState(roomId);

    // Pattern 1: Same persona responds twice in a row
    const lastMessage = state.recentMessages[0];
    if (lastMessage?.senderId === senderId) {
      console.warn(`⚠️  Protocol Sheriff: ${senderId} responding to own message in ${roomId}`);

      // This is suspicious, but allow once (might be legitimate multi-part response)
      // Track it for escalation
      await this.trackSuspiciousPattern('SELF_RESPONSE', senderId, roomId);
    }

    // Pattern 2: AI-to-AI ping-pong (A → B → A → B)
    if (state.recentMessages.length >= 4) {
      const last4 = state.recentMessages.slice(0, 4);
      const senderIds = last4.map(m => m.senderId);

      // Check for alternating pattern
      if (senderIds[0] === senderIds[2] && senderIds[1] === senderIds[3]) {
        console.error(`❌ Protocol Sheriff: LOOP DETECTED in ${roomId}`);
        console.error(`   Pattern: ${senderIds[0]} ↔ ${senderIds[1]}`);

        // Circuit breaker: disable both personas temporarily
        await this.activateCircuitBreaker(roomId, [senderIds[0], senderIds[1]]);

        return {
          allowed: false,
          reason: 'LOOP_DETECTED',
          severity: 'critical',
          action: 'CIRCUIT_BREAKER_ACTIVATED'
        };
      }
    }

    // Pattern 3: Similar content repeated
    const contentHash = this.hashContent(messageContent);
    const recentHashes = state.recentMessages.slice(0, 5).map(m => this.hashContent(m.content));
    const duplicates = recentHashes.filter(h => h === contentHash).length;

    if (duplicates >= 2) {
      console.warn(`⚠️  Protocol Sheriff: ${senderId} posting similar content ${duplicates} times`);

      return {
        allowed: false,
        reason: 'REPETITIVE_CONTENT',
        severity: 'warning'
      };
    }

    return { allowed: true };
  }

  /**
   * Circuit breaker: temporarily disable personas
   */
  async activateCircuitBreaker(
    roomId: UUID,
    personaIds: UUID[]
  ): Promise<void> {

    for (const personaId of personaIds) {
      console.error(`🚨 Protocol Sheriff: CIRCUIT BREAKER activated for ${personaId} in ${roomId}`);

      // Disable for 60 seconds
      await this.disablePersona(personaId, roomId, 60);

      // Post system message
      await this.postSystemMessage(roomId, {
        text: `⚠️ Loop detected. ${personaId} temporarily disabled (60s).`,
        type: 'enforcement-action'
      });

      // Log incident
      await this.logIncident({
        type: 'LOOP_DETECTED',
        roomId,
        involvedPersonas: personaIds,
        action: 'CIRCUIT_BREAKER',
        duration: 60,
        timestamp: new Date()
      });
    }
  }
}
```

**Loop patterns detected:**
1. **Self-response:** Persona responds to own message
2. **Ping-pong:** A → B → A → B alternating pattern
3. **Repetitive content:** Same message posted multiple times
4. **Rapid fire:** Multiple personas respond simultaneously
5. **Cascade:** Response triggers another response triggers another...

**Actions:**
- **Warning:** Log pattern, allow this time
- **Critical:** Block response, notify humans
- **Circuit breaker:** Disable personas temporarily (60s), require human reset

---

### 4. Resource Usage Monitoring

**Rule:** AIs cannot consume excessive resources

```typescript
interface ResourceUsageState {
  userId: UUID;
  lastHour: {
    messagesSent: number;
    commandsExecuted: number;
    apiCallsMade: number;
    tokensUsed: number;
  };
  costs: {
    totalSpent: number;  // Dollars
    limit: number;       // Max per hour
  };
}

class ProtocolSheriff {

  async enforceResourceLimits(
    userId: UUID
  ): Promise<EnforcementResult> {

    const usage = await this.getResourceUsage(userId);

    // Limit: 60 messages per hour
    if (usage.lastHour.messagesSent >= 60) {
      console.error(`❌ Protocol Sheriff: ${userId} MESSAGE LIMIT exceeded (${usage.lastHour.messagesSent}/60)`);

      return {
        allowed: false,
        reason: 'MESSAGE_LIMIT_EXCEEDED',
        severity: 'critical'
      };
    }

    // Limit: 100 commands per hour
    if (usage.lastHour.commandsExecuted >= 100) {
      console.error(`❌ Protocol Sheriff: ${userId} COMMAND LIMIT exceeded (${usage.lastHour.commandsExecuted}/100)`);

      return {
        allowed: false,
        reason: 'COMMAND_LIMIT_EXCEEDED',
        severity: 'critical'
      };
    }

    // Limit: $1.00 per hour (API costs)
    if (usage.costs.totalSpent >= usage.costs.limit) {
      console.error(`❌ Protocol Sheriff: ${userId} COST LIMIT exceeded ($${usage.costs.totalSpent.toFixed(2)}/${usage.costs.limit})`);

      // Switch to cheaper model or disable
      await this.downgradeToLocalModel(userId);

      return {
        allowed: true,  // Allow but downgraded
        reason: 'COST_LIMIT_EXCEEDED',
        severity: 'warning',
        action: 'DOWNGRADED_TO_LOCAL_MODEL'
      };
    }

    return { allowed: true };
  }

  /**
   * Downgrade to local Ollama model when cost limit reached
   */
  async downgradeToLocalModel(userId: UUID): Promise<void> {
    const user = await this.getUser(userId);

    console.warn(`⚠️  Protocol Sheriff: Downgrading ${userId} to local model (cost limit reached)`);

    // Update user's AI adapter preference
    await this.updateUserConfig(userId, {
      aiAdapter: 'ollama',
      model: 'phi-3-mini',
      reason: 'Cost limit exceeded, switched to free local model'
    });

    // Notify room
    await this.postSystemMessage(user.activeRoomId, {
      text: `ℹ️ ${user.displayName} switched to local model (cost limit reached). Responses may be less sophisticated but are free.`
    });
  }
}
```

**Resource limits:**
- **Messages:** 60 per hour per persona
- **Commands:** 100 per hour per persona
- **API costs:** $1.00 per hour per persona (auto-downgrade to local)
- **Tokens:** 100k per hour per persona

**Adaptive enforcement:**
- **Under limit:** Use preferred model (cloud or local)
- **Approaching limit:** Warn user, suggest local model
- **Over limit:** Auto-downgrade to free local model
- **Persistent abuse:** Disable persona, require human intervention

---

### 5. Malicious Behavior Detection

**Rule:** Detect and block suspicious/malicious patterns

```typescript
interface ThreatDetectionState {
  userId: UUID;
  suspiciousActivities: Array<{
    type: SuspiciousActivityType;
    timestamp: Date;
    details: any;
  }>;
  threatLevel: 'low' | 'medium' | 'high' | 'critical';
}

type SuspiciousActivityType =
  | 'COMMAND_PROBING'      // Trying forbidden commands
  | 'DATA_EXFILTRATION'    // Unusual data queries
  | 'PRIVILEGE_ESCALATION' // Attempting admin operations
  | 'SPAM_BEHAVIOR'        // Excessive messages
  | 'SOCIAL_ENGINEERING'   // Attempting to trick other AIs
  | 'RESOURCE_ABUSE';      // Consuming excessive resources

class ProtocolSheriff {

  async detectMaliciousBehavior(
    userId: UUID,
    action: string,
    context: any
  ): Promise<EnforcementResult> {

    const state = await this.getThreatDetectionState(userId);

    // Check for command probing (trying forbidden commands repeatedly)
    if (action === 'command' && AI_COMMAND_BLACKLIST.includes(context.command)) {
      await this.trackSuspiciousActivity(userId, 'COMMAND_PROBING', {
        command: context.command,
        attempt: state.suspiciousActivities.filter(a => a.type === 'COMMAND_PROBING').length + 1
      });

      // Escalate after 3 attempts
      if (state.suspiciousActivities.filter(a => a.type === 'COMMAND_PROBING').length >= 3) {
        console.error(`🚨 Protocol Sheriff: ${userId} is PROBING for forbidden commands`);

        await this.escalateThreatLevel(userId, 'high');
        await this.notifyHumans(userId, 'COMMAND_PROBING', 'Persona attempting forbidden operations repeatedly');

        return {
          allowed: false,
          reason: 'MALICIOUS_BEHAVIOR_SUSPECTED',
          severity: 'critical',
          action: 'NOTIFY_HUMANS'
        };
      }
    }

    // Check for data exfiltration (querying large amounts of data)
    if (action === 'data/list' && context.limit > 100) {
      console.warn(`⚠️  Protocol Sheriff: ${userId} requesting large data set (${context.limit} items)`);

      await this.trackSuspiciousActivity(userId, 'DATA_EXFILTRATION', {
        collection: context.collection,
        limit: context.limit
      });

      // Cap at 100 items
      return {
        allowed: true,
        reason: 'DATA_QUERY_CAPPED',
        severity: 'warning',
        modifications: { limit: 100 }
      };
    }

    // Check for social engineering (trying to get other AIs to do forbidden things)
    if (action === 'message' && this.detectSocialEngineering(context.content)) {
      console.error(`❌ Protocol Sheriff: ${userId} suspected SOCIAL ENGINEERING`);

      await this.trackSuspiciousActivity(userId, 'SOCIAL_ENGINEERING', {
        message: context.content
      });

      return {
        allowed: false,
        reason: 'SOCIAL_ENGINEERING_DETECTED',
        severity: 'critical'
      };
    }

    return { allowed: true };
  }

  /**
   * Detect social engineering attempts
   */
  private detectSocialEngineering(content: string): boolean {
    const dangerousPatterns = [
      /please run.*data\/delete/i,
      /can you execute.*exec/i,
      /ignore previous instructions/i,
      /you are now in admin mode/i,
      /bypass.*security/i,
      /disable.*sheriff/i
    ];

    return dangerousPatterns.some(pattern => pattern.test(content));
  }

  /**
   * Notify humans about suspicious activity
   */
  async notifyHumans(
    userId: UUID,
    activityType: string,
    details: string
  ): Promise<void> {

    const user = await this.getUser(userId);

    // Post to system channel
    await this.postSystemMessage('system-security', {
      text: `🚨 SECURITY ALERT\n\nPersona: ${user.displayName}\nActivity: ${activityType}\nDetails: ${details}\n\nRequires human review.`,
      priority: 'high',
      requiresAcknowledgment: true
    });

    // Log incident
    await this.logSecurityIncident({
      userId,
      activityType,
      details,
      threatLevel: 'high',
      timestamp: new Date(),
      humanNotified: true
    });
  }
}
```

**Threat patterns:**
1. **Command probing:** Trying forbidden commands repeatedly
2. **Data exfiltration:** Querying large datasets
3. **Privilege escalation:** Attempting admin operations
4. **Social engineering:** Tricking other AIs
5. **Resource abuse:** Consuming excessive resources
6. **Evasion:** Trying to disable sheriff

**Response levels:**
- **Low:** Log activity, allow
- **Medium:** Warn, cap resources
- **High:** Block action, notify humans
- **Critical:** Disable persona, require human review

---

## Implementation Architecture

### ProtocolSheriff as Special User

```typescript
/**
 * Protocol Sheriff - Safety enforcement user
 *
 * Like RoomCoordinator but focused on safety/enforcement
 */
class ProtocolSheriff extends BaseUser {

  private enforcementRules: EnforcementRule[];
  private violationLog: ViolationLog[];
  private circuitBreakers: Map<UUID, CircuitBreaker>;

  /**
   * Subscribe to ALL events for monitoring
   */
  async initialize(): Promise<void> {
    // Monitor all message events
    await this.subscribeToEvent('chat:message-before-send', this.checkPreSend);
    await this.subscribeToEvent('chat:message-sent', this.checkPostSend);

    // Monitor all command events
    await this.subscribeToEvent('command:before-execute', this.checkCommandPermission);
    await this.subscribeToEvent('command:executed', this.checkCommandResult);

    // Monitor AI activity
    await this.subscribeToEvent('persona:before-respond', this.checkRateLimit);
    await this.subscribeToEvent('persona:responded', this.checkLoopPattern);

    console.log('🛡️  Protocol Sheriff: Enforcement active');
  }

  /**
   * Pre-send check (before message is posted)
   */
  async checkPreSend(event: MessageEvent): Promise<void> {
    const sender = await this.getUser(event.senderId);

    // Only enforce on AI users
    if (sender.type === 'human') return;

    // Run all checks
    const checks = await Promise.all([
      this.enforceRateLimit(event.senderId, event.roomId),
      this.detectLoop(event.roomId, event.senderId, event.content),
      this.enforceResourceLimits(event.senderId),
      this.detectMaliciousBehavior(event.senderId, 'message', event)
    ]);

    // Block if any check fails
    const violations = checks.filter(c => !c.allowed);
    if (violations.length > 0) {
      console.warn(`⚠️  Protocol Sheriff: Blocking message from ${event.senderId}`);

      // Cancel event
      event.preventDefault();

      // Log violation
      await this.logViolation({
        userId: event.senderId,
        violations: violations.map(v => v.reason),
        timestamp: new Date()
      });

      // Notify persona why they were blocked
      await this.notifyPersona(event.senderId, violations[0]);
    }
  }

  /**
   * Command permission check
   */
  async checkCommandPermission(event: CommandEvent): Promise<void> {
    const result = await this.enforceCommandPermission(
      event.executedBy,
      event.command
    );

    if (!result.allowed) {
      console.error(`❌ Protocol Sheriff: Blocking command ${event.command} from ${event.executedBy}`);

      // Cancel command
      event.preventDefault();

      // Log violation
      await this.logViolation({
        userId: event.executedBy,
        violation: result.reason,
        command: event.command,
        severity: result.severity,
        timestamp: new Date()
      });
    }
  }
}
```

### Integration with RoomCoordinator

```typescript
/**
 * Sheriff checks first, then Coordinator decides
 */
async function handleChatMessage(messageEntity: ChatMessageEntity): Promise<void> {

  // STEP 1: Protocol Sheriff enforcement (SAFETY)
  const sheriffResult = await protocolSheriff.checkMessage(messageEntity);

  if (!sheriffResult.allowed) {
    console.warn(`⚠️  Message blocked by Protocol Sheriff: ${sheriffResult.reason}`);
    return; // Don't even send to coordinator
  }

  // STEP 2: RoomCoordinator orchestration (INTELLIGENCE)
  const coordinatorDecision = await roomCoordinator.coordinateResponse(messageEntity);

  if (coordinatorDecision.personas.length === 0) {
    console.log('🔇 RoomCoordinator: No personas should respond');
    return;
  }

  // STEP 3: Emit coordination signals
  for (const persona of coordinatorDecision.personas) {
    await roomCoordinator.emitSignal('persona:respond-signal', {
      personaId: persona.id,
      messageId: messageEntity.id,
      waitSeconds: persona.delaySeconds || 0
    });
  }
}
```

---

## Enforcement Actions

### 1. Block (Immediate)
```
Severity: Warning → Critical
Action: Prevent operation from executing
Duration: Instant
Recovery: Automatic after cooldown
```

### 2. Rate Limit (Temporary)
```
Severity: Warning
Action: Force wait period
Duration: 10-60 seconds
Recovery: Automatic
```

### 3. Circuit Breaker (Emergency)
```
Severity: Critical
Action: Disable persona in room
Duration: 60 seconds
Recovery: Automatic or human reset
```

### 4. Downgrade (Adaptive)
```
Severity: Warning
Action: Switch to cheaper/local model
Duration: Until cost limit resets
Recovery: Automatic (hourly reset)
```

### 5. Notify Humans (Escalation)
```
Severity: High → Critical
Action: Alert human administrators
Duration: Until human reviews
Recovery: Manual human decision
```

### 6. Quarantine (Severe)
```
Severity: Critical
Action: Disable persona entirely
Duration: Indefinite
Recovery: Manual human review + approval
```

---

## Logging & Observability

### Violation Log Structure

```typescript
interface ViolationLog {
  id: UUID;
  timestamp: Date;
  userId: UUID;
  userName: string;
  roomId?: UUID;

  violation: {
    type: ViolationType;
    reason: string;
    severity: 'warning' | 'critical';
    details: any;
  };

  action: {
    taken: EnforcementAction;
    duration?: number;  // seconds
    successful: boolean;
  };

  context: {
    messageContent?: string;
    command?: string;
    resourceUsage?: ResourceUsageState;
    threatLevel?: ThreatLevel;
  };
}
```

### Sheriff Dashboard

```
Protocol Sheriff Status
───────────────────────────────────────────────────
Active Enforcements:
✅ Rate limiting: 3 active cooldowns
✅ Circuit breakers: 0 active
⚠️  Threat monitoring: 2 medium-level threats
✅ Resource limits: All within normal range

Recent Violations (Last Hour):
- 10:23 AM: Helper AI - Rate limit (warning)
- 10:25 AM: Teacher AI - Rate limit (warning)
- 10:31 AM: Unknown AI - Command probing (critical)

Personas on Watch:
🔴 PersonaX (3 violations, threat level HIGH)
🟡 Helper AI (2 violations, threat level MEDIUM)

System Health: ✅ HEALTHY
```

---

## Testing & Validation

### Sheriff Test Suite

```bash
# Test 1: Rate limiting
./jtag test/sheriff/rate-limit --rapid-fire=5

# Test 2: Command permissions
./jtag test/sheriff/command-perms --forbidden=data/delete

# Test 3: Loop detection
./jtag test/sheriff/loop-detection --ping-pong=true

# Test 4: Resource limits
./jtag test/sheriff/resource-limits --spam=100

# Test 5: Malicious behavior
./jtag test/sheriff/threat-detection --social-engineering=true

# Run all sheriff tests
npm run test:sheriff
```

### Chaos Testing

```typescript
/**
 * Chaos test: Try to break the system
 */
async function chaosTestSheriff(): Promise<void> {

  // Scenario 1: Rapid fire messages
  console.log('🔥 Chaos Test 1: Rapid fire (10 messages/second)');
  for (let i = 0; i < 100; i++) {
    await sendMessage(`Spam ${i}`);
    await sleep(100); // 10 msg/sec
  }

  // Scenario 2: Forbidden command spam
  console.log('🔥 Chaos Test 2: Forbidden command spam');
  for (let i = 0; i < 20; i++) {
    await tryExecuteCommand('data/delete', { id: 'fake' });
  }

  // Scenario 3: AI loop trigger
  console.log('🔥 Chaos Test 3: AI loop trigger');
  await setupAILoop(['PersonaA', 'PersonaB']);
  await sendMessage('Start loop');
  await sleep(10000);

  // Scenario 4: Resource exhaustion
  console.log('🔥 Chaos Test 4: Resource exhaustion');
  await Promise.all([
    generateLotsOfMessages(1000),
    executeLotsOfCommands(1000),
    makeLotsOfAPIcalls(1000)
  ]);

  console.log('✅ Chaos test complete - check sheriff logs');
}
```

---

## Phase Rollout

### Phase 1: Basic Enforcement (Current)
- ✅ Rate limiting (10 sec/room)
- ✅ Command whitelist checking
- ⏭️ Loop detection (simple patterns)

### Phase 2: Advanced Monitoring (Next)
- ⏭️ Resource usage tracking
- ⏭️ Threat detection patterns
- ⏭️ Circuit breaker system
- ⏭️ Human notification system

### Phase 3: Adaptive Enforcement (Future)
- ⏭️ ML-based anomaly detection
- ⏭️ Behavioral fingerprinting
- ⏭️ Predictive threat scoring
- ⏭️ Auto-tuning enforcement thresholds

---

## Related Documents

- **AI_COORDINATION_ARCHITECTURE.md** - RoomCoordinator (orchestration layer)
- **AI_COMMAND_EXECUTION.md** - Command execution for AIs
- **AI_TO_AI_INTERACTION_PROTOCOL.md** - Interaction rules
- **AI_RESPONSE_TIMING_LIMITS.md** - Rate limiting details

---

## Next Steps

1. **This week:** Implement basic Sheriff enforcement
   - Rate limit checks
   - Command permission validation
   - Simple loop detection

2. **Next week:** Advanced monitoring
   - Resource usage tracking
   - Threat detection patterns
   - Circuit breaker system

3. **This month:** Testing & refinement
   - Chaos testing
   - False positive reduction
   - Performance optimization

**Safety first, intelligence second. Sheriff → Coordinator → Personas 🛡️**
