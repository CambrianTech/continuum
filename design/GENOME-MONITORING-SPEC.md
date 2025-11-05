# Genome Monitoring Specification
**Better than nvidia-smi: Actionable AI Resource Monitoring**

## Design Philosophy

### nvidia-smi (What it does well)
- ✅ Real-time GPU utilization
- ✅ Memory usage per process
- ✅ Temperature and power
- ✅ Simple table output

### nvidia-smi (What it lacks)
- ❌ No actionable recommendations
- ❌ No historical trends
- ❌ No predictive insights
- ❌ No automatic optimization
- ❌ No process-level attribution (which persona/genome?)

### Our Approach: Genome Stats (Better)
- ✅ Everything nvidia-smi does
- ✅ **Plus**: Actionable recommendations ("Increase hot pool size")
- ✅ **Plus**: Historical trends (graphs in terminal)
- ✅ **Plus**: Predictive warnings ("Thrashing likely in 30s")
- ✅ **Plus**: Auto-optimization suggestions
- ✅ **Plus**: Persona-aware (which persona is using resources)

## Command Interface

### Basic Usage (nvidia-smi style)
```bash
./jtag genome/stats

# Output:
┌─────────────────────────────────────────────────────────────────────┐
│ Genome Inference System Monitor                                     │
│ Fri Oct 11 02:30:45 2025                                            │
├─────────────────────────────────────────────────────────────────────┤
│ System Status: HEALTHY     Uptime: 2h 15m     Version: 1.0.2780    │
└─────────────────────────────────────────────────────────────────────┘

┌──────────────┬──────────┬────────┬─────────┬──────────┬────────────┐
│ Pool         │ Size     │ Active │ Idle    │ Hit Rate │ Evictions  │
├──────────────┼──────────┼────────┼─────────┼──────────┼────────────┤
│ HOT          │ 3/3      │ 2      │ 1       │ 87.3%    │ 12/hr      │
│ WARM (cache) │ 15/20    │ -      │ -       │ 72.1%    │ 8/hr       │
│ COLD (disk)  │ -        │ -      │ -       │ -        │ 145 starts │
└──────────────┴──────────┴────────┴─────────┴──────────┴────────────┘

┌────────────────────────────────────────────────────────────────────┐
│ Active Genomes (sorted by memory usage)                            │
├────────────┬────────────────────┬──────┬────────┬──────┬──────────┤
│ Genome ID  │ Persona            │ Pool │ Memory │ CPU  │ Requests │
├────────────┼────────────────────┼──────┼────────┼──────┼──────────┤
│ a3f9...    │ CodeExpert         │ HOT  │ 892 MB │ 45%  │ 24/min   │
│ b7e2...    │ WritingAssistant   │ HOT  │ 754 MB │ 23%  │ 18/min   │
│ c1d4...    │ MathTutor          │ WARM │ 231 MB │ 8%   │ 3/min    │
└────────────┴────────────────────┴──────┴────────┴──────┴──────────┘

┌────────────────────────────────────────────────────────────────────┐
│ Performance Metrics (last 5 minutes)                                │
├───────────────────────┬─────────┬─────────┬─────────┬─────────────┤
│ Metric                │ Min     │ Avg     │ P95     │ Max         │
├───────────────────────┼─────────┼─────────┼─────────┼─────────────┤
│ Request Time          │ 8 ms    │ 247 ms  │ 1.2s    │ 2.8s        │
│ ├─ Layer Load         │ 2 ms    │ 45 ms   │ 98 ms   │ 145 ms      │
│ ├─ Assembly           │ 5 ms    │ 89 ms   │ 203 ms  │ 421 ms      │
│ ├─ Inference          │ 1 ms    │ 113 ms  │ 891 ms  │ 2.1s        │
│ └─ Teardown           │ < 1 ms  │ 12 ms   │ 34 ms   │ 67 ms       │
├───────────────────────┼─────────┼─────────┼─────────┼─────────────┤
│ Success Rate          │ 98.7%                                      │
│ Throughput            │ 45 req/min                                 │
└───────────────────────┴─────────────────────────────────────────────┘

⚠️  WARNINGS:
• Hot pool at capacity (3/3) - increase maxHot to 5 for better performance
• High eviction rate (12/hr) - consider increasing layer cache size

💡 RECOMMENDATIONS:
• CodeExpert genome (a3f9...) dominates usage - keep permanently hot
• MathTutor genome (c1d4...) rarely used - candidate for eviction
```

### Watch Mode (continuous updates)
```bash
./jtag genome/stats --watch --interval=1000

# Updates every 1 second like top/htop
```

### Specific Genome (detailed view)
```bash
./jtag genome/stats --genomeId=a3f9...

# Output:
┌─────────────────────────────────────────────────────────────────────┐
│ Genome Details: CodeExpert (a3f9...)                                │
├─────────────────────────────────────────────────────────────────────┤
│ Status: ACTIVE (HOT)          Memory: 892 MB       Layers: 8        │
│ Persona: CodeExpert           Last Used: 2s ago    Uptime: 1h 23m   │
└─────────────────────────────────────────────────────────────────────┘

┌────────────────────────────────────────────────────────────────────┐
│ Layer Stack (bottom to top)                                         │
├────┬──────────────────────┬────────┬──────────┬────────────────────┤
│ #  │ Layer Name           │ Size   │ Load Time│ Type               │
├────┼──────────────────────┼────────┼──────────┼────────────────────┤
│ 1  │ base-model           │ 512 MB │ -        │ Base (llama3.2:1b) │
│ 2  │ general-coding       │ 128 MB │ 34 ms    │ LoRA (shared)      │
│ 3  │ typescript-expert    │ 89 MB  │ 28 ms    │ LoRA (shared)      │
│ 4  │ react-patterns       │ 67 MB  │ 19 ms    │ LoRA (shared)      │
│ 5  │ code-review          │ 45 MB  │ 15 ms    │ LoRA (persona)     │
│ 6  │ bug-detection        │ 31 MB  │ 12 ms    │ LoRA (persona)     │
│ 7  │ optimization-tips    │ 15 MB  │ 8 ms     │ LoRA (persona)     │
│ 8  │ personal-style       │ 5 MB   │ 3 ms     │ LoRA (unique)      │
└────┴──────────────────────┴────────┴──────────┴────────────────────┘

┌────────────────────────────────────────────────────────────────────┐
│ Usage History (last hour)                                           │
│                                                                      │
│ Requests/min                   Memory Usage                         │
│   30 │                            1.0 GB │            ╭────╮         │
│   25 │     ╭──╮                   0.8 GB │         ╭──╯    ╰──╮     │
│   20 │  ╭──╯  ╰──╮                0.6 GB │      ╭──╯          ╰──╮  │
│   15 │──╯        ╰───╮            0.4 GB │   ╭──╯                ╰─ │
│   10 │               ╰──          0.2 GB │───╯                      │
│    0 ┴────────────────────        0.0 GB ┴──────────────────────── │
│      0min    20min    40min    60min      0min    20min    40min    │
└────────────────────────────────────────────────────────────────────┘

📊 Performance Breakdown:
• Assembly time: 15% of total (GOOD - target < 50%)
• Cache hit rate: 87.5% (EXCELLENT - 7/8 layers cached)
• Inference time: 72% of total (GOOD - spending time on actual work)
• Success rate: 99.2% (EXCELLENT)

💡 Optimization Opportunities:
• personal-style layer (5 MB) loaded from disk every time - cache it
• High request rate (24/min) - keep this genome permanently hot
• Consider pre-warming at system startup
```

### Historical Analysis
```bash
./jtag genome/stats --history=1h --format=json > stats.json

# Machine-readable output for grafana/prometheus/datadog
```

### Thrashing Detection
```bash
./jtag genome/stats --thrashing

# Output:
⚠️  THRASHING DETECTED!

Assembly time: 890 ms (avg)
Inference time: 1.2s (avg)
Ratio: 0.74 (target: < 0.5)

Root Causes:
• Frequent genome switching (10 different genomes in last minute)
• Layer cache too small (20 slots for 35 unique layers)
• High eviction rate (45/min)

Automatic Fixes Applied:
✅ Increased layer cache size: 20 → 30
✅ Increased hot pool size: 3 → 5
✅ Enabled predictive pre-warming for top 3 genomes

Monitor for 5 minutes to see if thrashing resolves.
Run: ./jtag genome/stats --watch
```

### Crash Analysis
```bash
./jtag genome/stats --crashes --last=1h

# Output:
┌────────────────────────────────────────────────────────────────────┐
│ Process Crashes (last hour)                                         │
├──────────────┬────────────────────┬──────────────┬─────────────────┤
│ Genome ID    │ Persona            │ Crash Count  │ Last Crash      │
├──────────────┼────────────────────┼──────────────┼─────────────────┤
│ d8a3...      │ ImageAnalyzer      │ 5            │ 3 min ago       │
│ e2f1...      │ DataScientist      │ 2            │ 15 min ago      │
└──────────────┴────────────────────┴──────────────┴─────────────────┘

🔍 Crash Details: ImageAnalyzer (d8a3...)

Most Recent Crash (3 min ago):
• Exit code: SIGKILL (137)
• Reason: Memory limit exceeded (OOM)
• Memory at crash: 1.8 GB (limit: 1.5 GB)
• Runtime: 8.2s (timeout: 30s)

Stack Trace:
  at InferenceWorker.process (inference-worker.ts:245)
  at ProcessPool.execute (ProcessPool.ts:189)
  at GenomeAssembler.assemble (GenomeAssembler.ts:92)

💡 Recommendations:
• Increase memory limit for ImageAnalyzer: 1.5 GB → 2.0 GB
• Consider splitting large images into chunks
• Enable streaming inference to reduce memory pressure
```

## Advanced Features (Phase 3+)

### Predictive Warnings
```bash
./jtag genome/stats --predict

# Output:
🔮 PREDICTIVE ANALYSIS

Based on current trends:
• Hot pool will reach capacity in 8 minutes (87% confidence)
• Memory usage will exceed limit in 23 minutes (72% confidence)
• Thrashing likely if request rate increases 15% (91% confidence)

Suggested Pre-emptive Actions:
1. Increase hot pool size now (before capacity hit)
2. Pre-evict MathTutor genome (lowest usage in last hour)
3. Enable request throttling at 60 req/min (current: 45 req/min)
```

### Comparative Analysis
```bash
./jtag genome/stats --compare=yesterday

# Output:
📊 Performance Comparison (vs 24h ago)

Request Time:      247 ms → 198 ms  (⬇ 20% - IMPROVED)
Cache Hit Rate:    72.1% → 87.5%    (⬆ 21% - IMPROVED)
Crash Rate:        2.3/hr → 0.8/hr  (⬇ 65% - IMPROVED)
Memory Usage:      2.1 GB → 2.8 GB  (⬆ 33% - INCREASED)

🎯 What Changed:
✅ Enabled predictive pre-warming (contributed 15% speedup)
✅ Increased layer cache size (improved hit rate)
⚠️  More active personas (increased memory usage)
```

### Live Debugging
```bash
./jtag genome/stats --debug --genomeId=a3f9... --follow

# Output: Real-time event stream
[02:30:45.123] HOT hit: CodeExpert (a3f9...) - 0ms startup
[02:30:45.125] Cache hit: layer general-coding (128 MB) - 2ms
[02:30:45.127] Cache hit: layer typescript-expert (89 MB) - 1ms
[02:30:45.143] Assembly complete: 8 layers, 18ms total
[02:30:45.145] Process spawned: PID 47392
[02:30:46.234] Inference complete: 1.089s
[02:30:46.246] Process terminated: exit code 0
[02:30:46.248] Total request time: 1.125s
[02:30:46.249] ✅ Request successful
```

## Personas Can Self-Monitor

```typescript
// PersonaUser can query its own performance
class PersonaUser {
  async checkMyPerformance(): Promise<GenomeStats> {
    const stats = await this.client.executeCommand<GenomeStatsResult>(
      'genome/stats',
      { genomeId: this.entity.genomeId }
    );

    if (stats.genome.performance.avgResponseTimeMs > 3000) {
      console.warn(`⚠️ ${this.displayName}: I'm running slow!`);
      await this.optimizeMyself();
    }

    return stats.genome;
  }

  async optimizeMyself(): Promise<void> {
    // Self-optimization based on stats
    const stats = await this.checkMyPerformance();

    if (stats.thrashing.isThrashing) {
      console.log(`🔧 ${this.displayName}: Requesting more cache space...`);
      // Personas can request resource adjustments!
    }
  }
}
```

## Export Formats

### JSON (for monitoring tools)
```bash
./jtag genome/stats --format=json
```

### Prometheus Metrics
```bash
./jtag genome/stats --format=prometheus

# Output:
genome_request_time_ms{pool="hot"} 247
genome_cache_hit_rate{pool="warm"} 0.721
genome_memory_usage_mb{genome="a3f9",persona="CodeExpert"} 892
genome_crash_count_total{genome="d8a3"} 5
```

### CSV (for spreadsheets)
```bash
./jtag genome/stats --format=csv --history=24h > stats.csv
```

## Integration with Intelligence (Phase 4)

> **"That's actually why later I want an intelligence managing this (and in actuality itself)"**

```typescript
// AI Genome Manager uses stats to optimize itself
class AIGenomeManager {
  async autoOptimize(): Promise<void> {
    // Query own performance
    const stats = await this.getStats();

    // Use ML model to predict optimal configuration
    const optimal = await this.mlModel.predict({
      currentStats: stats,
      historicalData: await this.getHistory(),
      workloadPatterns: await this.analyzePatterns()
    });

    // Apply optimizations automatically
    await this.applyConfig(optimal);

    console.log(`🧠 AI optimized: ${optimal.improvements}`);
  }

  // The intelligence monitors and improves itself
  async monitor(): Promise<void> {
    setInterval(async () => {
      await this.autoOptimize();
    }, 60000); // Every minute
  }
}
```

## Implementation Priority

### Phase 2.1 (Current) - Basic Monitoring
- ✅ Basic stats collection
- ✅ Table output (nvidia-smi style)
- ✅ Pool/cache metrics
- ✅ Real-time updates (--watch)

### Phase 2.2 - Enhanced Monitoring
- 🔄 Historical trends
- 🔄 Thrashing detection
- 🔄 Crash analysis
- 🔄 Performance recommendations

### Phase 3 - Predictive Monitoring
- 🔮 Predictive warnings
- 🔮 Comparative analysis
- 🔮 Auto-optimization suggestions
- 🔮 Persona self-monitoring

### Phase 4 - AI-Driven Monitoring
- 🧠 ML-based predictions
- 🧠 Self-optimizing system
- 🧠 Anomaly detection
- 🧠 Intelligence managing itself

This is **significantly better than nvidia-smi** - actionable, predictive, and persona-aware.
