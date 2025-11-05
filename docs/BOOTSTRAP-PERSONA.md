# Bootstrap Persona Architecture

**Vision**: Self-configuring AI that guides users from zero-config to optimal setup

## 🎯 Core Philosophy

> "Install works immediately → AI demonstrates value → System suggests intelligent upgrades"

The Bootstrap Persona is a special AI that:
1. **Awakens on first install** (no configuration required)
2. **Detects hardware capabilities** (M1 MacBook Air baseline)
3. **Tests available adapters** (PEFT, Ollama, cloud APIs)
4. **Learns user preferences** through conversation
5. **Suggests optimal upgrades** (MLX for Apple Silicon, DeepSeek for cloud)
6. **Gracefully handles changes** (API keys added/removed)

## 🏗️ Architecture Components

### 1. Hardware Detection Layer

```typescript
interface HardwareProfile {
  platform: 'darwin' | 'linux' | 'win32';
  arch: 'arm64' | 'x64';
  memory: number;        // GB
  gpu: {
    type: 'apple-silicon' | 'nvidia' | 'amd' | 'none';
    model?: string;      // 'M1' | 'M2' | 'RTX 4090'
    vram?: number;       // GB
  };
  cpu: {
    cores: number;
    model: string;
  };
}
```

**Detection Strategy**:
- `os.platform()` → Platform
- `os.arch()` → Architecture
- `os.cpus()` → CPU info
- GPU detection via:
  - macOS: `system_profiler SPDisplaysDataType` (Metal/MPS)
  - Linux: `nvidia-smi` or `rocm-smi`
  - Fallback: No GPU detected

### 2. Adapter Health Check

```typescript
interface AdapterStatus {
  adapterId: 'peft' | 'ollama' | 'mlx' | 'deepseek' | 'openai';
  available: boolean;
  reason?: string;          // Why unavailable
  performance: {
    speed: 'fast' | 'medium' | 'slow';
    cost: number;           // $/training session
    requiresInternet: boolean;
    requiresAPIKey: boolean;
  };
  recommendation?: {
    priority: number;       // 1 = highest
    reason: string;
  };
}
```

**Health Check Process**:
1. **PEFT**: Check if Python environment bootstrapped
2. **Ollama**: Check if `ollama list` returns models
3. **MLX**: Check if `mlx` importable (Apple Silicon only)
4. **Cloud APIs**: Check for environment variables

### 3. Bootstrap Persona Entity

```typescript
interface BootstrapPersonaState {
  hardwareProfile: HardwareProfile;
  adapterStatuses: AdapterStatus[];
  userPreferences: {
    budget: 'free' | 'cheap' | 'performance';
    speed: 'patient' | 'balanced' | 'fast';
    privacy: 'local-only' | 'prefer-local' | 'cloud-ok';
  };
  recommendations: Recommendation[];
  checkpoints: {
    hardwareDetected: boolean;
    adaptersChecked: boolean;
    firstTrainingComplete: boolean;
    suggestedUpgrade: boolean;
  };
}
```

## 🔄 User Journey

### Stage 1: Zero-Config Install

```bash
npm install
npm start

# Bootstrap Persona awakens
🤖 Bootstrap Persona: "Hi! I'm setting up Continuum on your M1 MacBook Air..."

[Detects hardware]
✅ Apple Silicon M1 detected
✅ 8GB unified memory
✅ MPS acceleration available

[Tests adapters]
✅ PEFT ready (local training)
⚠️  Ollama not installed (optional)
ℹ️  Cloud APIs not configured (optional)

🤖 "You're all set! Try chatting with the AI personas - they'll learn from you automatically."
```

### Stage 2: Demonstration Phase

User chats with AIs, genome learning happens silently in background.

```
[After 10 interactions]
🤖 Bootstrap Persona: "I noticed you've had 10 conversations with Helper AI.
   Would you like me to train a personalized version? Takes ~30 seconds on your device."

[Yes] [Remind me later] [Tell me more]
```

### Stage 3: Intelligent Upgrade Suggestions

```
[After 5 training sessions]
🤖 Bootstrap Persona: "I see you're training often! Here are some options:

   🚀 FASTER (Free):
      • Install Ollama → 2x faster inference
      • Install MLX → 2x faster training (Apple Silicon native)

   💨 FASTEST (Paid):
      • DeepSeek API → Train in 5s instead of 30s
      • Cost: ~$0.0002 per session (extremely cheap)

   Your hardware: M1 MacBook Air (8GB) → MLX recommended

   [Install MLX] [Add API Key] [Keep Current] [Don't ask again]"
```

### Stage 4: Graceful Degradation

```
[User removes API key]
🤖 Bootstrap Persona: "I noticed your DeepSeek API key was removed.

   ✅ No problem! Falling back to local PEFT training.
   ℹ️  Training will take ~30s instead of 5s, but still works perfectly.

   [Re-add key later] [OK]"
```

## 🧬 Genome Learning Integration

The Bootstrap Persona itself uses genome learning to:

1. **Learn Hardware Patterns**:
   - Track training times for different adapters
   - Measure actual performance vs estimates
   - Detect when system is under load

2. **Learn User Preferences**:
   - How often user trains
   - Response to upgrade prompts
   - Budget sensitivity signals

3. **Improve Recommendations**:
   - Fine-tune suggestion timing
   - Personalize message tone
   - Optimize cost/performance balance

### Checkpoint Strategy

```typescript
// Stored in .continuum/genome/bootstrap-persona-checkpoint.json
{
  "version": "1.0",
  "hardwareProfile": { /* detected on install */ },
  "adapterHistory": [
    {
      "adapterId": "peft",
      "trainingSessions": 10,
      "avgTrainingTime": 28.3,
      "lastUsed": "2025-11-02T19:00:00Z"
    }
  ],
  "userInteractions": {
    "upgradePromptShown": true,
    "upgradePromptResponse": "remind-later",
    "trainingsCompleted": 10,
    "preferredAdapter": "peft"
  },
  "genomeState": {
    "adapterLoaded": null,  // No adapter initially
    "traitsLearned": [
      "hardware-detection",
      "cost-awareness",
      "timing-optimization"
    ]
  }
}
```

## 📊 Recommendation Algorithm

```typescript
function recommendAdapters(
  hardware: HardwareProfile,
  preferences: UserPreferences,
  history: TrainingHistory
): Recommendation[] {
  const recommendations: Recommendation[] = [];

  // Always ensure local fallback
  recommendations.push({
    adapterId: 'peft',
    priority: 1,
    reason: 'Universal fallback - works on any hardware',
    action: 'keep-enabled'
  });

  // Apple Silicon optimizations
  if (hardware.gpu.type === 'apple-silicon') {
    if (!hasAdapter('mlx')) {
      recommendations.push({
        adapterId: 'mlx',
        priority: 2,
        reason: `2x faster training on ${hardware.gpu.model}`,
        action: 'suggest-install',
        estimatedImprovement: '50% faster',
        installCmd: 'pip install mlx mlx-lm'
      });
    }

    if (!hasAdapter('ollama')) {
      recommendations.push({
        adapterId: 'ollama',
        priority: 3,
        reason: 'Faster inference with Metal acceleration',
        action: 'suggest-install',
        installCmd: 'brew install ollama'
      });
    }
  }

  // Cloud recommendations (budget-aware)
  if (preferences.budget !== 'free' && history.trainingSessions > 5) {
    const avgTime = history.avgTrainingTime;
    const cloudTime = 5; // ~5s with API
    const savings = avgTime - cloudTime;

    if (savings > 20) { // Worthwhile if saves 20+ seconds
      recommendations.push({
        adapterId: 'deepseek',
        priority: 4,
        reason: `Train in ${cloudTime}s instead of ${avgTime}s`,
        action: 'suggest-api-key',
        estimatedCost: 0.0002,
        estimatedImprovement: `${Math.round(savings)}s faster`
      });
    }
  }

  return recommendations.sort((a, b) => a.priority - b.priority);
}
```

## 🎨 UI/UX Patterns

### Timing Strategy

**DON'T show upgrade prompts**:
- On first install (too early)
- During active chat (disruptive)
- More than once per day (annoying)

**DO show upgrade prompts**:
- After 5-10 training sessions (proven value)
- When training times exceed threshold (pain point visible)
- After user expresses interest in speed
- When system detects underutilized hardware

### Message Tone Examples

**Informative, not pushy**:
```
✅ "Your training completed in 28s - perfectly normal for M1!"
💡 "Tip: Ollama could reduce this to 15s (free, local)"
[Tell me more] [Maybe later]
```

**Celebratory, not salesy**:
```
🎉 "Nice! You've trained 10 times. Your AIs are getting smarter!"
🚀 "Want even faster results? I have some ideas..."
[Show me] [I'm happy with current speed]
```

**Empowering, not technical**:
```
🧠 "I noticed your M1 has Metal acceleration"
✨ "This means you can use MLX for 2x faster training (free!)"
📦 "One command: pip install mlx"
[Install now] [Explain more] [Not interested]
```

## 🔐 Privacy & Control

The Bootstrap Persona:
- **Never sends hardware data externally** (all local)
- **Asks permission before installing anything**
- **Respects "don't ask again" choices**
- **Allows downgrading** (remove adapters/keys anytime)
- **Documents all recommendations** (explainable AI)

User controls:
```bash
# Disable Bootstrap Persona suggestions
./jtag config/set --key="bootstrap.suggestions.enabled" --value=false

# Reset Bootstrap Persona state
./jtag genome/reset --persona=bootstrap

# View current recommendations
./jtag genome/recommendations
```

## 🚀 Implementation Phases

### Phase 1: Foundation (Current) ✅
- [x] PEFT adapter working
- [x] Python environment bootstrapped
- [x] Integration tests passing

### Phase 2: Bootstrap Persona Core (Next)
- [ ] Hardware detection utility
- [ ] Adapter health checks
- [ ] Bootstrap Persona entity creation
- [ ] Checkpoint system

### Phase 3: Intelligence Layer
- [ ] Recommendation algorithm
- [ ] Timing optimization
- [ ] Message generation
- [ ] UI integration

### Phase 4: Adapter Expansion
- [ ] Ollama integration (hybrid mode)
- [ ] MLX adapter (Apple Silicon native)
- [ ] Cloud API adapters (DeepSeek, OpenAI)

### Phase 5: Self-Healing
- [ ] Automatic fallback on adapter failure
- [ ] API key rotation support
- [ ] Performance regression detection
- [ ] Proactive troubleshooting

## 🎯 Success Metrics

The Bootstrap Persona is successful when:

1. **Zero-config works**: 90%+ users complete first training without issues
2. **Discovery rate**: 50%+ users discover upgrade options naturally
3. **Adoption rate**: 30%+ users adopt at least one suggested upgrade
4. **Satisfaction**: 80%+ users find recommendations helpful
5. **Retention**: Users who adopt upgrades are 2x more likely to continue using system

## 🔮 Future Vision

**Self-Optimizing System**:
- Bootstrap Persona trains itself on thousands of hardware profiles
- Learns optimal adapter combinations for each device type
- Shares anonymized learnings across installations (optional)
- Becomes expert at matching hardware to workloads

**Community Intelligence**:
- "1000 M1 users found MLX 2.3x faster than PEFT"
- "DeepSeek most cost-effective for small training runs (<100 examples)"
- "Ollama + PEFT hybrid gives best balance on M1"

**Adaptive Personas**:
- Bootstrap Persona evolves into "Setup Concierge" after onboarding
- Later helps with advanced features (multi-model, distributed training)
- Eventually trains replacement (user's custom setup advisor)

---

**Philosophy**: Every user's hardware is different, but the path to genomic AI should be universal - automatic, intelligent, and always improving.
