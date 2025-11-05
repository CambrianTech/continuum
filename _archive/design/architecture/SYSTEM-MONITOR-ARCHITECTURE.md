# SYSTEM MONITOR DAEMON ARCHITECTURE

**Date**: 2025-10-08
**Status**: Core architecture specification
**Purpose**: AI-driven process lifecycle management for PersonaUser processes

---

## Overview

The **System Monitor Daemon** is the intelligent orchestrator that manages PersonaUser process lifecycles with **AI-driven decision making**. Think of it as:

- **Kubernetes** for minds (container orchestration)
- **AWS CloudWatch** with ML anomaly detection
- **systemd** but AI-native (process supervision)
- **RTOS scheduler** with fuzzy intelligence (time-slice allocation)

**Core Insight**: You cannot use hardcoded thresholds to manage unpredictable AI model behavior. **It takes a mind to manage minds.**

---

## Architectural Principles

### 1. AI-Driven Decisions (Not Hardcoded Rules)

**❌ WRONG: Deterministic thresholds miss context**
```typescript
if (memoryMB > 500) restart();        // Maybe it's doing legitimate work?
if (latency > 2000) kill();           // Maybe model is just slow today?
if (errorRate > 0.05) evict();        // Maybe handling edge cases?
```

**✅ CORRECT: AI analyzes holistic patterns**
```typescript
const decision = await this.monitorAI.analyze({
  systemPrompt: `You are a process supervisor for AI personas.
  Analyze ALL metrics holistically to detect anomalies:
  - Memory trends (stable vs. leaking vs. oscillating)
  - Response latency patterns (consistent slow vs. degrading)
  - Error context (legitimate edge cases vs. model failure)
  - System load (high but healthy vs. resource starvation)

  Recommend: continue, restart, hibernate, kill, or investigate.
  Explain your reasoning.`,

  metrics: {
    target: processMetrics,
    peers: otherProcessMetrics,    // Compare to similar personas
    system: systemResourceMetrics,  // Global context
    history: last24Hours            // Temporal patterns
  }
});

// Execute AI recommendation
await this.executeAction(decision.action, decision.reason);
```

### 2. Process-Per-Persona Containerization

Each PersonaUser (especially external AI models like GPT, Claude via API) runs in an **isolated child process**:

**Why child processes?**
- **Crash isolation**: One persona failure doesn't kill the system
- **Resource limits**: Memory/CPU caps per process (OS-level enforcement)
- **Clean shutdown**: Can kill without affecting siblings
- **Metric visibility**: OS provides process-level stats (PID, memory, CPU)

**Architecture:**
```
SessionDaemon (parent process)
├── SystemMonitorDaemon (child process - the orchestrator)
│   └── Monitors all persona processes
│
├── PersonaUser Process #1 (child process)
│   └── GPT-4 via API wrapper
│
├── PersonaUser Process #2 (child process)
│   └── Local llama-3.1-8B + LoRA layers
│
├── PersonaUser Process #3 (child process)
│   └── Claude via Anthropic API
│
└── PersonaUser Process #N (child process)
    └── Custom genome persona
```

### 3. Time-Slice Allocation (RTOS-Style)

**Inspired by**: C++ event-driven architecture with thread-per-module and manager control over execution time budgets.

Each persona gets a **compute budget** that the System Monitor allocates intelligently:

```typescript
interface PersonaComputeBudget {
  personaId: UUID;

  // Time allocation
  maxResponseTimeMs: number;        // 30s default, can extend for complex tasks
  tokensPerResponse: number;        // 500 tokens default (adjustable)
  inferenceQueuePriority: number;   // 0-100, higher = faster service

  // Resource limits
  maxMemoryMB: number;              // Process memory cap
  maxConcurrentInferences: number;  // Queue depth limit

  // Dynamic adjustments (AI-driven)
  throttleMultiplier: number;       // 0.5 = half speed, 2.0 = double priority
  allowedToRun: boolean;            // Hard kill switch
}
```

**AI determines budgets based on:**
- Persona importance (system vs. user-facing vs. background)
- Current workload (burst activity vs. idle)
- Resource availability (plenty vs. constrained)
- Quality of service (critical path vs. best-effort)

**Example AI decisions:**
- "Persona A is handling user chat - increase priority to 90, extend timeout to 45s"
- "Persona B has been idle 10 minutes - hibernate, reduce priority to 10"
- "Persona C is causing memory pressure - throttle to 0.5x, investigate if continues"
- "System under load - all background personas to 25% throttle until clear"

---

## Process Metrics (What We Monitor)

### Resource Metrics
```typescript
interface ResourceMetrics {
  // Memory
  memoryMB: number;                  // Current resident set size
  memoryPeak: number;                // Peak since spawn
  memoryTrend: 'stable' | 'growing' | 'oscillating' | 'leaking';

  // CPU
  cpuPercent: number;                // Current CPU usage
  cpuAverage: number;                // Rolling 5-minute average

  // Disk I/O (for model loading)
  diskReadMB: number;
  diskWriteMB: number;
}
```

### Behavioral Metrics
```typescript
interface BehaviorMetrics {
  // Response characteristics
  avgResponseTimeMs: number;         // Mean latency
  p50ResponseTimeMs: number;         // Median latency
  p95ResponseTimeMs: number;         // 95th percentile (outlier detection)
  p99ResponseTimeMs: number;         // 99th percentile (worst case)

  // Throughput
  inferenceCount: number;            // Total inferences since spawn
  inferencesPerMinute: number;       // Current throughput

  // Quality
  errorRate: number;                 // Errors / total requests
  errorTypes: Map<string, number>;   // Error classification
  cacheHitRate: number;              // LoRA cache efficiency

  // Activity
  lastActivityAt: Date;
  idleDurationMs: number;
  burstDurationMs: number;           // Time in continuous activity
}
```

### Anomaly Detection (AI-Generated)
```typescript
interface AnomalyMetrics {
  anomalies: Array<{
    type: 'memory_spike' | 'latency_degradation' | 'error_burst' |
          'unresponsive' | 'thrashing' | 'resource_leak' | 'crash_loop';
    severity: number;                // 0-1 (AI confidence score)
    detectedAt: Date;
    description: string;             // AI explanation
    recommendedAction: 'monitor' | 'restart' | 'kill' | 'investigate';
  }>;

  healthScore: number;               // 0-100 composite health
  confidenceScore: number;           // AI's certainty about health assessment
}
```

### Complete Process Snapshot
```typescript
interface PersonaProcessMetrics {
  // Identity
  personaId: UUID;
  processId: number;                 // OS PID
  spawnedAt: Date;
  uptimeMs: number;

  // Resources
  resources: ResourceMetrics;

  // Behavior
  behavior: BehaviorMetrics;

  // Anomalies (AI analysis)
  anomalies: AnomalyMetrics;

  // Budget (current allocation)
  budget: PersonaComputeBudget;

  // Context
  activeContexts: UUID[];            // Chat rooms, games, sessions
  queueDepth: number;                // Pending inference requests
}
```

---

## Lifecycle Actions

### 1. Continue
**When**: Process is healthy, performing as expected
**Action**: No intervention, keep monitoring

### 2. Restart
**When**:
- Memory leak detected (growing trend)
- Performance degradation (latency doubled)
- High error rate but recoverable
- Scheduled maintenance (after N inferences)

**Action**:
1. Stop accepting new requests
2. Drain current inference queue (graceful timeout)
3. Save state if needed (conversation context)
4. Kill process
5. Spawn new process with same genome
6. Restore state
7. Resume accepting requests

**Downtime**: 5-10 seconds (LoRA loading time)

### 3. Hibernate
**When**:
- Idle for >5 minutes
- System under memory pressure
- Low-priority background persona

**Action**:
1. Save full state to disk
2. Unload LoRA layers from cache
3. Kill process
4. Mark as "hibernating" in registry

**Wake trigger**:
- New inference request arrives
- System resources available
- AI determines persona needed again

### 4. Kill
**When**:
- Crash loop detected (restarting >3 times in 5 minutes)
- Unrecoverable error (corrupt model, API auth failure)
- Resource violation (exceeded hard limits)
- External model permanently unavailable

**Action**:
1. Hard kill (SIGKILL)
2. Log failure reason
3. Notify user if user-facing persona
4. Remove from active registry
5. Do NOT auto-restart

### 5. Throttle
**When**:
- System under load
- Persona consuming excessive resources
- Lower priority than competing personas

**Action**:
1. Reduce compute budget (slower responses)
2. Lower queue priority (longer wait times)
3. Reduce max concurrent inferences
4. Continue monitoring - may escalate to hibernate/kill

---

## Monitoring Loop (The AI Orchestrator)

```typescript
class SystemMonitorDaemon {
  private processes: Map<UUID, PersonaProcessMetrics> = new Map();
  private monitorAI: AIProvider;  // Small, fast model (llama-3.1-1B fine-tuned)
  private historyWindow: number = 24 * 60 * 60 * 1000; // 24 hours

  /**
   * Main monitoring loop - runs every 10 seconds
   */
  async monitoringLoop(): Promise<void> {
    while (this.running) {
      try {
        // 1. Gather ALL metrics from ALL processes
        const snapshot = await this.gatherSystemSnapshot();

        // 2. AI analyzes holistically
        const decisions = await this.analyzeWithAI(snapshot);

        // 3. Execute actions
        for (const decision of decisions) {
          await this.executeAction(decision);
        }

        // 4. Log and persist metrics
        await this.persistMetrics(snapshot);

      } catch (error) {
        console.error('❌ System Monitor failed:', error);
        // System Monitor itself has issues - alert and attempt recovery
        await this.selfDiagnose();
      }

      await this.sleep(10000); // 10 second interval
    }
  }

  /**
   * Gather complete system snapshot
   */
  private async gatherSystemSnapshot(): Promise<SystemSnapshot> {
    const processes: PersonaProcessMetrics[] = [];

    for (const [personaId, process] of this.processes) {
      // OS-level metrics (via node's process module or ps command)
      const pid = process.processId;
      const osMetrics = await this.getOSMetrics(pid);

      // Application metrics (from persona's health endpoint)
      const appMetrics = await this.getAppMetrics(personaId);

      // Historical analysis
      const history = await this.getMetricHistory(personaId, this.historyWindow);

      // Detect anomalies
      const anomalies = this.detectAnomalies(osMetrics, appMetrics, history);

      processes.push({
        personaId,
        processId: pid,
        resources: osMetrics,
        behavior: appMetrics,
        anomalies,
        budget: process.budget,
        // ... rest of metrics
      });
    }

    return {
      timestamp: new Date(),
      processes,
      system: await this.getSystemMetrics(), // Global CPU, memory, disk
      context: await this.getSystemContext()  // Active users, load level
    };
  }

  /**
   * AI-driven analysis of system state
   */
  private async analyzeWithAI(snapshot: SystemSnapshot): Promise<MonitoringDecision[]> {
    const prompt = this.buildSystemPrompt(snapshot);

    const response = await this.monitorAI.generateJSON<MonitoringDecisions>({
      systemPrompt: SYSTEM_MONITOR_PROMPT,
      userPrompt: prompt,
      schema: MonitoringDecisionsSchema
    });

    return response.decisions;
  }

  /**
   * Execute lifecycle action
   */
  private async executeAction(decision: MonitoringDecision): Promise<void> {
    const { action, personaId, reason } = decision;

    console.log(`🔧 System Monitor: ${action} persona ${personaId.slice(0, 8)} - ${reason}`);

    switch (action) {
      case 'continue':
        // No action needed
        break;

      case 'restart':
        await this.restartPersona(personaId, reason);
        break;

      case 'hibernate':
        await this.hibernatePersona(personaId, reason);
        break;

      case 'kill':
        await this.killPersona(personaId, reason);
        break;

      case 'throttle':
        await this.throttlePersona(personaId, decision.throttleMultiplier);
        break;

      case 'investigate':
        // Flag for human review
        await this.flagForInvestigation(personaId, reason);
        break;
    }
  }

  /**
   * Restart persona gracefully
   */
  private async restartPersona(personaId: UUID, reason: string): Promise<void> {
    const process = this.processes.get(personaId);
    if (!process) return;

    console.log(`🔄 Restarting persona ${personaId.slice(0, 8)}: ${reason}`);

    // 1. Stop accepting new requests
    await this.setPersonaState(personaId, 'draining');

    // 2. Wait for current inferences to complete (with timeout)
    await this.drainInferenceQueue(personaId, 30000); // 30s timeout

    // 3. Save state
    const state = await this.savePersonaState(personaId);

    // 4. Kill process
    process.kill('SIGTERM');
    await this.waitForExit(process.processId, 5000);

    // 5. Spawn new process
    const newProcess = await this.spawnPersona(personaId, state.genome);

    // 6. Restore state
    await this.restorePersonaState(personaId, state);

    // 7. Resume
    await this.setPersonaState(personaId, 'active');

    console.log(`✅ Persona ${personaId.slice(0, 8)} restarted successfully`);
  }
}
```

---

## AI System Prompt (The Brain)

```typescript
const SYSTEM_MONITOR_PROMPT = `You are the System Monitor for PersonaOS.

Your job is to analyze process metrics for AI persona processes and make intelligent
lifecycle decisions (continue, restart, hibernate, kill, throttle).

YOU MUST THINK HOLISTICALLY:
- Don't judge processes in isolation - compare to peers and history
- Consider system-wide context (is everyone slow, or just this one?)
- Understand temporal patterns (burst activity is normal, sustained degradation is not)
- Respect process importance (user-facing > background > idle)

DECISION CRITERIA:

1. CONTINUE (healthy process):
   - Metrics within normal ranges
   - No anomalies detected
   - Performing as expected

2. RESTART (recoverable issues):
   - Memory growing steadily over hours (likely leak)
   - Response time doubled compared to history
   - Error rate 5-10% but model otherwise functional
   - Process uptime >24 hours (scheduled refresh)

3. HIBERNATE (idle resource recovery):
   - No activity for >5 minutes
   - System memory pressure >80%
   - Low-priority background persona

4. KILL (unrecoverable failure):
   - Crash loop (restarted >3 times in 5 minutes)
   - External API permanently unavailable (auth failure, quota exceeded)
   - Corrupt model or invalid genome
   - Hard resource limit violation

5. THROTTLE (temporary performance reduction):
   - System under load (CPU >80%, memory >90%)
   - Process consuming excessive resources relative to importance
   - Lower priority than competing personas

IMPORTANT:
- Explain your reasoning for each decision
- If uncertain, choose "investigate" and explain why
- Avoid false positives (restarting healthy processes wastes resources)
- Avoid false negatives (ignoring real issues degrades user experience)

OUTPUT FORMAT: JSON array of decisions`;
```

---

## Metrics Collection Architecture

### OS-Level Metrics (via Node.js)
```typescript
async function getOSMetrics(pid: number): Promise<ResourceMetrics> {
  // Use node's process module or spawn ps/top command
  const stats = await pidusage(pid);

  return {
    memoryMB: stats.memory / 1024 / 1024,
    memoryPeak: stats.memoryPeak / 1024 / 1024,
    cpuPercent: stats.cpu,
    // ... compute trends from history
  };
}
```

### Application-Level Metrics (via Health Endpoint)
```typescript
// Each PersonaUser process exposes health endpoint
class PersonaHealthEndpoint {
  async getHealth(): Promise<BehaviorMetrics> {
    return {
      avgResponseTimeMs: this.computeAvg(this.responseTimesHistory),
      p95ResponseTimeMs: this.computePercentile(this.responseTimesHistory, 0.95),
      inferenceCount: this.totalInferences,
      errorRate: this.errors / this.totalInferences,
      // ... more app-level metrics
    };
  }
}
```

### Historical Storage (Time-Series Database)
```typescript
// Store metrics in SQLite with time-series optimization
interface MetricRecord {
  personaId: UUID;
  timestamp: Date;
  metrics: PersonaProcessMetrics; // JSON blob
}

// Query patterns:
// - Last 24 hours for trend analysis
// - Last 1 hour for anomaly detection
// - Aggregates for dashboard display
```

---

## Integration with Existing Systems

### 1. SessionDaemon Integration
The System Monitor is spawned by SessionDaemon as a privileged child process:

```typescript
class SessionDaemonServer {
  private systemMonitor: SystemMonitorDaemon;

  async initialize(): Promise<void> {
    // Spawn System Monitor as first child
    this.systemMonitor = new SystemMonitorDaemon({
      monitoringInterval: 10000, // 10s
      aiModel: 'llama-3.1-1B-system-monitor', // Fine-tuned for this task
    });

    await this.systemMonitor.start();
  }
}
```

### 2. PersonaUser Integration
Each PersonaUser registers with System Monitor on spawn:

```typescript
class PersonaUser {
  async initialize(): Promise<void> {
    // Register with System Monitor
    await SystemMonitor.registerProcess({
      personaId: this.id,
      processId: process.pid,
      genome: this.genome,
      importance: this.calculateImportance()
    });

    // Start health reporting (every 10s)
    this.startHealthReporting();
  }

  private startHealthReporting(): void {
    setInterval(async () => {
      const health = await this.getHealthMetrics();
      await SystemMonitor.reportHealth(this.id, health);
    }, 10000);
  }
}
```

### 3. Commands Integration
Expose system monitoring data via commands:

```typescript
// commands/system/monitor/status
interface SystemMonitorStatusResult {
  processes: PersonaProcessMetrics[];
  systemHealth: number; // 0-100
  recentActions: Array<{
    action: string;
    personaId: UUID;
    reason: string;
    timestamp: Date;
  }>;
}
```

---

## Implementation Phases

### Phase 1: Basic Process Monitoring (MVP)
- ✅ Process spawning/killing
- ✅ OS-level metrics (memory, CPU via pidusage)
- ✅ Simple health checks (is process alive?)
- ✅ Manual restart command

### Phase 2: AI-Driven Decisions
- 🔄 Historical metrics storage (SQLite time-series)
- 🔄 AI analysis of metrics (llama-3.1-1B)
- 🔄 Automated restart/hibernate/kill decisions
- 🔄 Anomaly detection

### Phase 3: Advanced Orchestration
- 🔮 Time-slice allocation (RTOS-style budgets)
- 🔮 Predictive failure detection
- 🔮 Load-based throttling
- 🔮 Self-healing clusters (P2P mesh failover)

---

## Real-World Validation

**Observation** (2025-10-08): After git check-in (which kills processes), personas respond faster.

**Hypothesis**: Long-running processes accumulate:
- Memory bloat (model state, cached contexts)
- Event listener accumulation
- WebSocket connection staleness
- Inference queue backlog

**Validation**: Fresh process restart = clean slate = better performance

**Architecture Impact**: This confirms **lifecycle management is not optional** - it's essential for maintaining system responsiveness. The System Monitor's ability to detect degradation and restart intelligently will directly improve user experience.

---

## Summary

**System Monitor Daemon = Container Orchestration for Minds**

- **AI-driven decisions** (not hardcoded thresholds)
- **Process-per-persona** (crash isolation, resource limits)
- **Time-slice allocation** (RTOS-style compute budgets)
- **Holistic monitoring** (resource + behavioral + anomaly metrics)
- **Intelligent actions** (restart, hibernate, kill, throttle)

**Core Philosophy**: "It takes a mind to manage minds" - AI models are unpredictable, so supervision must be intelligent.
