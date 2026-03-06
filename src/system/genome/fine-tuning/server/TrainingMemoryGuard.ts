/**
 * TrainingMemoryGuard — Memory-aware wrapper for training processes.
 *
 * Solves the "rogue training process kills the machine" problem by:
 * 1. Pre-flight: Fresh system memory check, estimate training footprint, refuse if insufficient
 * 2. Registration: Register training with GPU eviction registry (visible to entire system)
 * 3. Monitoring: Poll system memory during training, kill process if critical
 * 4. Cleanup: Unregister on completion or failure (guaranteed via try/finally)
 *
 * On Apple Silicon, VRAM IS system RAM. A 3B QLoRA training session can consume 4-8GB
 * of unified memory (model weights + optimizer states + gradient buffers). Without
 * tracking, this is completely invisible to the memory management system.
 */

import { RustCoreIPCClient } from '../../../../workers/continuum-core/bindings/RustCoreIPC';
import { Logger } from '@system/core/logging/Logger';
import { Events } from '@system/core/shared/Events';

const log = Logger.create('training-memory-guard', 'genome');

// Model size estimates for QLoRA 4-bit training (model + optimizer + gradients)
// Conservative multipliers: actual usage varies with batch size, sequence length, etc.
const TRAINING_MEMORY_MULTIPLIERS: Record<string, number> = {
  'SmolLM2-135M':      0.5,   // ~0.5 GB
  'SmolLM2-360M':      1.0,   // ~1.0 GB
  'SmolLM2-1.7B':      3.0,   // ~3.0 GB
  'Llama-3.2-1B':      2.5,   // ~2.5 GB
  'Llama-3.2-3B':      6.0,   // ~6.0 GB
  'Qwen3-0.6B':        1.5,   // ~1.5 GB
  'Qwen3-1.7B':        3.5,   // ~3.5 GB
  'Qwen3-4B':          8.0,   // ~8.0 GB
  'Phi-4-mini':        7.0,   // ~7.0 GB
  'DeepSeek-R1-1.5B':  3.0,   // ~3.0 GB
  'Gemma-3-1B':        2.5,   // ~2.5 GB
  'Gemma-3-4B':        8.0,   // ~8.0 GB
};

const DEFAULT_TRAINING_GB = 6.0; // Conservative default if model unknown
const SAFETY_MARGIN_GB = 2.0;    // Keep 2GB headroom for system stability
const BYTES_PER_GB = 1024 * 1024 * 1024;

// Memory monitoring during training
const MONITOR_INTERVAL_MS = 5_000;       // Check every 5s
const CRITICAL_AVAILABLE_GB = 1.0;       // Kill training if <1GB available
const WARNING_AVAILABLE_GB = 2.0;        // Log warning if <2GB available

export interface TrainingMemoryEstimate {
  modelName: string;
  estimatedBytes: number;
  estimatedGb: number;
  availableBytes: number;
  availableGb: number;
  sufficient: boolean;
  reason?: string;
}

export class TrainingMemoryGuard {
  private _consumerId: string;
  private _estimatedBytes: number;
  private _actualBytes: number | null = null;
  private _monitorTimer: ReturnType<typeof setInterval> | null = null;
  private _trainingPid: number | null = null;
  private _registered = false;
  private _sentinelHandle: string | null = null;
  private _memoryReportUnsub: (() => void) | null = null;

  constructor(
    private readonly modelName: string,
    private readonly personaName: string,
    private readonly traitType: string,
  ) {
    this._consumerId = `training:${personaName}:${traitType}:${Date.now()}`;
    this._estimatedBytes = this.estimateTrainingMemory();
  }

  get consumerId(): string { return this._consumerId; }
  get estimatedBytes(): number { return this._estimatedBytes; }
  get actualBytes(): number | null { return this._actualBytes; }

  /**
   * Pre-flight check: is there enough memory to train?
   * Does a FRESH system memory query (not cached).
   */
  async preflight(): Promise<TrainingMemoryEstimate> {
    const estimatedGb = this._estimatedBytes / BYTES_PER_GB;

    let availableBytes: number;
    try {
      const rustClient = RustCoreIPCClient.getInstance();
      const memStats = await rustClient.systemMemory();
      availableBytes = memStats.availableBytes;
    } catch (e) {
      // If IPC fails, refuse training — we can't safely proceed without knowing memory state
      return {
        modelName: this.modelName,
        estimatedBytes: this._estimatedBytes,
        estimatedGb,
        availableBytes: 0,
        availableGb: 0,
        sufficient: false,
        reason: `Cannot check system memory: ${e}. Refusing training to prevent OOM.`,
      };
    }

    const availableGb = availableBytes / BYTES_PER_GB;
    const requiredGb = estimatedGb + SAFETY_MARGIN_GB;
    const sufficient = availableGb >= requiredGb;

    const result: TrainingMemoryEstimate = {
      modelName: this.modelName,
      estimatedBytes: this._estimatedBytes,
      estimatedGb,
      availableBytes,
      availableGb,
      sufficient,
    };

    if (!sufficient) {
      result.reason = `Training ${this.modelName} needs ~${estimatedGb.toFixed(1)}GB + ${SAFETY_MARGIN_GB}GB safety = ${requiredGb.toFixed(1)}GB, ` +
        `but only ${availableGb.toFixed(1)}GB available. Free memory by stopping Ollama models or other processes.`;
    }

    log.info(`Preflight: model=${this.modelName}, need=${requiredGb.toFixed(1)}GB, available=${availableGb.toFixed(1)}GB, sufficient=${sufficient}`);
    return result;
  }

  /**
   * Register this training session with the GPU memory system.
   * Makes it visible in eviction registry and pressure calculations.
   */
  async register(): Promise<void> {
    try {
      const rustClient = RustCoreIPCClient.getInstance();
      const result = await rustClient.gpuRegisterConsumer(
        this._consumerId,
        `Training: ${this.personaName} / ${this.traitType} (${this.modelName})`,
        this._estimatedBytes,
        'batch', // Lowest priority — yields first under pressure
      );
      this._registered = true;
      log.info(`Registered: id=${this._consumerId}, estimated=${(this._estimatedBytes / BYTES_PER_GB).toFixed(1)}GB, pressure=${(result.pressure * 100).toFixed(0)}%`);
    } catch (e) {
      log.warn(`Failed to register training consumer (non-blocking): ${e}`);
    }
  }

  /**
   * Unregister this training session. Called on completion or failure.
   */
  async unregister(): Promise<void> {
    if (!this._registered) return;
    try {
      const rustClient = RustCoreIPCClient.getInstance();
      await rustClient.gpuUnregisterConsumer(this._consumerId, this._estimatedBytes);
      this._registered = false;
      log.info(`Unregistered: id=${this._consumerId}`);
    } catch (e) {
      log.warn(`Failed to unregister training consumer (non-blocking): ${e}`);
    }
  }

  /**
   * Subscribe to real memory reports from the training process.
   * The Python script emits MEMORY_REPORT: lines on stdout, which the Rust
   * sentinel parses and publishes as sentinel:{handle}:memory-report events.
   * When received, we update the GPU registry with actual (not estimated) bytes.
   */
  watchMemoryReports(sentinelHandle: string): void {
    this._sentinelHandle = sentinelHandle;
    const eventName = `sentinel:${sentinelHandle}:memory-report`;

    this._memoryReportUnsub = Events.subscribe(eventName, async (report: {
      phase: string;
      allocatedBytes: number;
      peakBytes: number;
      processRssBytes: number;
    }) => {
      // Use peak or RSS — whichever is larger — as the real footprint.
      // Peak captures GPU/unified memory; RSS captures full process footprint.
      const realBytes = Math.max(report.peakBytes, report.processRssBytes, report.allocatedBytes);
      if (realBytes <= 0) return;

      const realGb = realBytes / BYTES_PER_GB;
      const estimatedGb = this._estimatedBytes / BYTES_PER_GB;
      this._actualBytes = realBytes;

      log.info(`Memory report [${report.phase}]: actual=${realGb.toFixed(1)}GB (estimated=${estimatedGb.toFixed(1)}GB, peak=${(report.peakBytes / BYTES_PER_GB).toFixed(1)}GB, rss=${(report.processRssBytes / BYTES_PER_GB).toFixed(1)}GB)`);

      // If actual usage differs significantly from estimate, update the GPU registry
      if (this._registered && Math.abs(realBytes - this._estimatedBytes) > 512 * 1024 * 1024) {
        try {
          const rustClient = RustCoreIPCClient.getInstance();
          // Unregister old estimate, re-register with real data
          await rustClient.gpuUnregisterConsumer(this._consumerId, this._estimatedBytes);
          await rustClient.gpuRegisterConsumer(
            this._consumerId,
            `Training: ${this.personaName} / ${this.traitType} (${this.modelName}) [measured]`,
            realBytes,
            'batch',
          );
          log.info(`Updated GPU registry: ${estimatedGb.toFixed(1)}GB (est) -> ${realGb.toFixed(1)}GB (actual)`);
          this._estimatedBytes = realBytes; // Update so cleanup releases the right amount
        } catch (e) {
          log.warn(`Failed to update GPU registry with real memory data: ${e}`);
        }
      }
    });

    log.info(`Watching memory reports: event=${eventName}`);
  }

  /**
   * Start monitoring memory during training.
   * Kills the training process if available memory drops below critical threshold.
   * @param pid Optional PID of the training process to kill if memory is critical
   */
  startMonitoring(pid?: number): void {
    this._trainingPid = pid ?? null;

    this._monitorTimer = setInterval(async () => {
      try {
        const rustClient = RustCoreIPCClient.getInstance();
        const memStats = await rustClient.systemMemory();
        const availableGb = memStats.availableBytes / BYTES_PER_GB;

        if (availableGb < CRITICAL_AVAILABLE_GB) {
          log.error(`CRITICAL: Only ${availableGb.toFixed(1)}GB available during training. KILLING training process.`);
          Events.emit('training:memory:critical', {
            consumerId: this._consumerId,
            availableGb,
            action: 'killed',
          }).catch(() => {});
          this.killTrainingProcess();
          this.stopMonitoring();
        } else if (availableGb < WARNING_AVAILABLE_GB) {
          log.warn(`WARNING: Only ${availableGb.toFixed(1)}GB available during training (${this._consumerId})`);
          Events.emit('training:memory:warning', {
            consumerId: this._consumerId,
            availableGb,
          }).catch(() => {});
        }
      } catch {
        // IPC failure during monitoring — don't crash, just skip this check
      }
    }, MONITOR_INTERVAL_MS);

    log.info(`Monitoring started: interval=${MONITOR_INTERVAL_MS}ms, critical<${CRITICAL_AVAILABLE_GB}GB, pid=${pid ?? 'unknown'}`);
  }

  /** Stop monitoring and clean up event subscriptions. */
  stopMonitoring(): void {
    if (this._monitorTimer) {
      clearInterval(this._monitorTimer);
      this._monitorTimer = null;
    }
    if (this._memoryReportUnsub) {
      this._memoryReportUnsub();
      this._memoryReportUnsub = null;
    }
  }

  /**
   * Execute a training function with full memory lifecycle management.
   * Handles: preflight → register → monitor → execute → unregister (guaranteed cleanup).
   */
  async guard<T>(trainingFn: () => Promise<T>): Promise<T> {
    // 1. Preflight check
    const estimate = await this.preflight();
    if (!estimate.sufficient) {
      throw new Error(estimate.reason!);
    }

    // 2. Register with GPU system
    await this.register();

    try {
      // 3. Start monitoring (PID not known yet — training subprocess will set it)
      this.startMonitoring();

      // 4. Execute training
      return await trainingFn();
    } finally {
      // 5. Guaranteed cleanup
      this.stopMonitoring();
      await this.unregister();
    }
  }

  // ── Private ──

  private estimateTrainingMemory(): number {
    // Try to match model name against known estimates
    for (const [key, gb] of Object.entries(TRAINING_MEMORY_MULTIPLIERS)) {
      if (this.modelName.toLowerCase().includes(key.toLowerCase())) {
        return gb * BYTES_PER_GB;
      }
    }
    // Default: assume 6GB for unknown models
    return DEFAULT_TRAINING_GB * BYTES_PER_GB;
  }

  private killTrainingProcess(): void {
    if (this._trainingPid) {
      try {
        process.kill(this._trainingPid, 'SIGTERM');
        log.info(`Sent SIGTERM to training PID ${this._trainingPid}`);
        // Give it 5s to clean up, then SIGKILL
        setTimeout(() => {
          try {
            process.kill(this._trainingPid!, 'SIGKILL');
            log.info(`Sent SIGKILL to training PID ${this._trainingPid}`);
          } catch {
            // Process already exited
          }
        }, 5000);
      } catch {
        // Process already exited
      }
    } else {
      log.warn('Cannot kill training process: PID unknown');
    }
  }
}
