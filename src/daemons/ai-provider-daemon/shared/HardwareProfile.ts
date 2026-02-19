/**
 * HardwareProfile - Self-classifying hardware capability system
 *
 * Automatically detects and benchmarks hardware to determine:
 * - Inference speed (tokens/sec)
 * - Appropriate timeouts
 * - Context window limits based on latency targets
 * - Recommended model quantization
 *
 * Used by CandleAdapter and AIProviderDaemon to make intelligent
 * decisions about timeouts, budgets, and model selection.
 */

import * as fs from 'fs';
import * as path from 'path';
import { execSync } from 'child_process';

export interface HardwareCapabilities {
  // Hardware detection
  accelerator: 'cuda' | 'metal' | 'cpu';
  device: string;  // e.g., "RTX 5090", "M3 Max", "Intel i9"
  memoryGB: number;

  // Benchmarked capabilities
  tokensPerSecond: number;
  promptProcessingSpeed: number;  // tokens/sec for input processing
  benchmarkModel: string;
  benchmarkedAt: string;

  // Derived limits
  recommendedMaxTokens: number;
  recommendedTimeout: number;  // ms
  recommendedContextWindow: number;

  // User overrides (optional)
  userOverrides?: {
    maxTokens?: number;
    timeout?: number;
    contextWindow?: number;
    targetLatency?: number;  // seconds
  };
}

// Default profiles for different hardware classes
const HARDWARE_PROFILES: Record<string, Partial<HardwareCapabilities>> = {
  // NVIDIA GPUs
  'rtx-5090': { tokensPerSecond: 250, promptProcessingSpeed: 2000, memoryGB: 32 },
  'rtx-4090': { tokensPerSecond: 180, promptProcessingSpeed: 1500, memoryGB: 24 },
  'rtx-3090': { tokensPerSecond: 100, promptProcessingSpeed: 800, memoryGB: 24 },
  'rtx-4080': { tokensPerSecond: 140, promptProcessingSpeed: 1200, memoryGB: 16 },
  'rtx-3080': { tokensPerSecond: 80, promptProcessingSpeed: 600, memoryGB: 10 },

  // Apple Silicon
  'm4-max': { tokensPerSecond: 70, promptProcessingSpeed: 500, memoryGB: 128 },
  'm3-max': { tokensPerSecond: 45, promptProcessingSpeed: 350, memoryGB: 96 },
  'm3-pro': { tokensPerSecond: 30, promptProcessingSpeed: 250, memoryGB: 36 },
  'm2-max': { tokensPerSecond: 35, promptProcessingSpeed: 280, memoryGB: 96 },
  'm2-pro': { tokensPerSecond: 25, promptProcessingSpeed: 200, memoryGB: 32 },
  'm1-max': { tokensPerSecond: 30, promptProcessingSpeed: 250, memoryGB: 64 },
  'm1-pro': { tokensPerSecond: 18, promptProcessingSpeed: 150, memoryGB: 32 },
  'm1': { tokensPerSecond: 12, promptProcessingSpeed: 100, memoryGB: 16 },

  // CPU fallback
  'cpu-fast': { tokensPerSecond: 5, promptProcessingSpeed: 20, memoryGB: 32 },
  'cpu-slow': { tokensPerSecond: 2, promptProcessingSpeed: 8, memoryGB: 16 },
};

export class HardwareProfile {
  private static instance: HardwareProfile | null = null;
  private capabilities: HardwareCapabilities | null = null;
  private profilePath: string;

  private constructor() {
    this.profilePath = path.join(process.cwd(), '.continuum', 'hardware-profile.json');
  }

  static getInstance(): HardwareProfile {
    if (!HardwareProfile.instance) {
      HardwareProfile.instance = new HardwareProfile();
    }
    return HardwareProfile.instance;
  }

  /**
   * Get hardware capabilities, loading from cache or detecting fresh
   */
  async getCapabilities(forceRefresh = false): Promise<HardwareCapabilities> {
    // Return cached capabilities if available
    if (this.capabilities && !forceRefresh) {
      return this.capabilities;
    }

    // Try loading from disk cache
    if (!forceRefresh && fs.existsSync(this.profilePath)) {
      try {
        const cached = JSON.parse(fs.readFileSync(this.profilePath, 'utf-8'));
        // Check if cache is less than 24 hours old
        const cacheAge = Date.now() - new Date(cached.benchmarkedAt).getTime();
        if (cacheAge < 24 * 60 * 60 * 1000) {
          this.capabilities = cached;
          console.log(`📊 HardwareProfile: Loaded cached profile (${cached.device})`);
          return cached;
        }
      } catch (e) {
        // Ignore cache errors, will re-detect
      }
    }

    // Detect fresh
    this.capabilities = await this.detectAndBenchmark();
    return this.capabilities;
  }

  /**
   * Detect hardware and optionally run quick benchmark
   */
  private async detectAndBenchmark(): Promise<HardwareCapabilities> {
    console.log('📊 HardwareProfile: Detecting hardware capabilities...');

    // Detect accelerator type
    const { accelerator, device, memoryGB } = this.detectHardware();

    // Get base profile from known hardware
    const profileKey = this.getProfileKey(device);
    const baseProfile = HARDWARE_PROFILES[profileKey] || HARDWARE_PROFILES['cpu-slow'];

    // Build capabilities
    const capabilities: HardwareCapabilities = {
      accelerator,
      device,
      memoryGB,
      tokensPerSecond: baseProfile.tokensPerSecond || 10,
      promptProcessingSpeed: baseProfile.promptProcessingSpeed || 50,
      benchmarkModel: 'estimated',
      benchmarkedAt: new Date().toISOString(),
      recommendedMaxTokens: this.calculateMaxTokens(baseProfile.tokensPerSecond || 10),
      recommendedTimeout: this.calculateTimeout(baseProfile.tokensPerSecond || 10),
      recommendedContextWindow: this.calculateContextWindow(baseProfile.promptProcessingSpeed || 50),
    };

    console.log(`📊 HardwareProfile: ${device} (${accelerator}) - ${capabilities.tokensPerSecond} tok/s`);

    // Save to cache
    this.saveProfile(capabilities);

    return capabilities;
  }

  /**
   * Detect what hardware is available
   */
  private detectHardware(): { accelerator: 'cuda' | 'metal' | 'cpu'; device: string; memoryGB: number } {
    // Try CUDA first (NVIDIA)
    try {
      const nvidiaSmi = execSync('nvidia-smi --query-gpu=name,memory.total --format=csv,noheader', { encoding: 'utf-8' });
      const [name, memStr] = nvidiaSmi.trim().split(',').map(s => s.trim());
      const memoryGB = parseInt(memStr) / 1024;  // Convert MiB to GB
      return { accelerator: 'cuda', device: name, memoryGB };
    } catch (e) {
      // No NVIDIA GPU
    }

    // Try Metal (Apple Silicon)
    try {
      const chip = execSync('sysctl -n machdep.cpu.brand_string', { encoding: 'utf-8' }).trim();
      const memBytes = parseInt(execSync('sysctl -n hw.memsize', { encoding: 'utf-8' }).trim());
      const memoryGB = memBytes / (1024 * 1024 * 1024);

      if (chip.includes('Apple')) {
        return { accelerator: 'metal', device: chip, memoryGB };
      }
    } catch (e) {
      // Not macOS or error
    }

    // Fallback to CPU
    let cpuInfo = 'Unknown CPU';
    try {
      cpuInfo = execSync('sysctl -n machdep.cpu.brand_string', { encoding: 'utf-8' }).trim();
    } catch (e) {
      try {
        cpuInfo = execSync('cat /proc/cpuinfo | grep "model name" | head -1 | cut -d: -f2', { encoding: 'utf-8' }).trim();
      } catch (e2) {
        // Give up
      }
    }

    return { accelerator: 'cpu', device: cpuInfo, memoryGB: 16 };
  }

  /**
   * Map detected device to profile key
   */
  private getProfileKey(device: string): string {
    const lower = device.toLowerCase();

    // NVIDIA
    if (lower.includes('5090')) return 'rtx-5090';
    if (lower.includes('4090')) return 'rtx-4090';
    if (lower.includes('3090')) return 'rtx-3090';
    if (lower.includes('4080')) return 'rtx-4080';
    if (lower.includes('3080')) return 'rtx-3080';

    // Apple Silicon
    if (lower.includes('m4') && lower.includes('max')) return 'm4-max';
    if (lower.includes('m3') && lower.includes('max')) return 'm3-max';
    if (lower.includes('m3') && lower.includes('pro')) return 'm3-pro';
    if (lower.includes('m2') && lower.includes('max')) return 'm2-max';
    if (lower.includes('m2') && lower.includes('pro')) return 'm2-pro';
    if (lower.includes('m1') && lower.includes('max')) return 'm1-max';
    if (lower.includes('m1') && lower.includes('pro')) return 'm1-pro';
    if (lower.includes('m1')) return 'm1';
    if (lower.includes('m2')) return 'm2-pro';
    if (lower.includes('m3')) return 'm3-pro';
    if (lower.includes('m4')) return 'm4-max';

    // CPU fallback
    if (lower.includes('i9') || lower.includes('i7') || lower.includes('ryzen 9')) {
      return 'cpu-fast';
    }

    return 'cpu-slow';
  }

  /**
   * Calculate recommended max tokens based on generation speed
   * Target: 30 second response time
   */
  private calculateMaxTokens(tokensPerSecond: number, targetSeconds = 30): number {
    const raw = tokensPerSecond * targetSeconds;
    // Clamp between 50 and 2000
    return Math.max(50, Math.min(2000, Math.round(raw)));
  }

  /**
   * Calculate recommended timeout based on generation speed
   * Allow 2x the expected time as safety margin
   */
  private calculateTimeout(tokensPerSecond: number, targetTokens = 500): number {
    const expectedMs = (targetTokens / tokensPerSecond) * 1000;
    // Minimum 30s, maximum 5 minutes
    return Math.max(30000, Math.min(300000, Math.round(expectedMs * 2)));
  }

  /**
   * Calculate recommended context window based on prompt processing speed
   * Target: 10 second prompt processing time
   */
  private calculateContextWindow(promptSpeed: number, targetSeconds = 10): number {
    const raw = promptSpeed * targetSeconds;
    // Clamp between 500 and 32000
    return Math.max(500, Math.min(32000, Math.round(raw)));
  }

  /**
   * Get tokens per second (for use by other systems)
   */
  getTokensPerSecond(): number {
    return this.capabilities?.tokensPerSecond || 10;
  }

  /**
   * Calculate how many input tokens we can afford given a latency target
   */
  getLatencyAwareInputLimit(targetLatencySeconds = 30, reserveOutputTokens = 200): number {
    if (!this.capabilities) {
      return 2000;  // Safe default
    }

    const outputTime = reserveOutputTokens / this.capabilities.tokensPerSecond;
    const inputTime = targetLatencySeconds - outputTime;
    const inputTokens = inputTime * this.capabilities.promptProcessingSpeed;

    return Math.max(500, Math.round(inputTokens));
  }

  /**
   * Check if we should use quantized models
   */
  shouldUseQuantizedModels(): boolean {
    if (!this.capabilities) return true;

    // Use quantized if we're slow or have limited memory
    return this.capabilities.tokensPerSecond < 30 ||
           this.capabilities.memoryGB < 16;
  }

  /**
   * Save profile to disk
   */
  private saveProfile(capabilities: HardwareCapabilities): void {
    try {
      const dir = path.dirname(this.profilePath);
      if (!fs.existsSync(dir)) {
        fs.mkdirSync(dir, { recursive: true });
      }
      fs.writeFileSync(this.profilePath, JSON.stringify(capabilities, null, 2));
    } catch (e) {
      console.warn('📊 HardwareProfile: Failed to save profile:', e);
    }
  }

  /**
   * Update with user overrides
   */
  setUserOverrides(overrides: HardwareCapabilities['userOverrides']): void {
    if (this.capabilities) {
      this.capabilities.userOverrides = overrides;
      this.saveProfile(this.capabilities);
    }
  }

  /**
   * Run actual benchmark (optional - for more accurate numbers)
   * Returns updated capabilities
   */
  async runBenchmark(socketPath: string): Promise<HardwareCapabilities | null> {
    if (!this.capabilities) {
      await this.getCapabilities();
    }

    try {
      console.log('📊 HardwareProfile: Running inference benchmark...');

      // This would run actual inference through the socket
      // For now, just return estimated capabilities
      console.log('📊 HardwareProfile: Using estimated values (benchmark not implemented)');

      return this.capabilities;
    } catch (e) {
      console.error('📊 HardwareProfile: Benchmark failed:', e);
      return null;
    }
  }
}

// Singleton export
export const hardwareProfile = HardwareProfile.getInstance();
