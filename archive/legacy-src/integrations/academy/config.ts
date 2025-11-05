/**
 * Academy Integration - Dynamic Configuration System
 * 
 * Replaces hardcoded values with environment-driven configuration
 * Supports runtime reconfiguration and dynamic discovery
 */

import * as path from 'path';
import * as fs from 'fs';

/**
 * Dynamic path configuration that adapts to environment
 */
export interface DynamicPathConfig {
  base_directory: string;
  training_data: string;
  model_cache: string;
  session_data: string;
  logs: string;
}

/**
 * Dynamic model configuration for runtime adaptation
 */
export interface DynamicModelConfig {
  default_provider: string;
  fallback_providers: string[];
  model_discovery_enabled: boolean;
  auto_scaling_enabled: boolean;
}

/**
 * Dynamic performance configuration
 */
export interface DynamicPerformanceConfig {
  max_concurrent_sessions: number;
  evaluation_interval_ms: number;
  health_check_interval_ms: number;
  auto_cleanup_enabled: boolean;
  resource_monitoring_enabled: boolean;
}

/**
 * Comprehensive dynamic configuration
 */
export interface DynamicAcademyConfig {
  paths: DynamicPathConfig;
  models: DynamicModelConfig;
  performance: DynamicPerformanceConfig;
  features: {
    local_mode: boolean;
    p2p_enabled: boolean;
    distributed_training: boolean;
    real_time_metrics: boolean;
  };
  persona: {
    auto_discovery: boolean;
    capability_inference: boolean;
    dynamic_specialization: boolean;
  };
}

/**
 * Environment-based configuration loader
 * Supports hot-reloading and runtime reconfiguration
 */
export class DynamicConfigurationManager {
  private static instance: DynamicConfigurationManager;
  private config: DynamicAcademyConfig;
  private watchers: Map<string, fs.FSWatcher> = new Map();
  private changeListeners: ((config: DynamicAcademyConfig) => void)[] = [];

  private constructor() {
    this.config = this.loadConfiguration();
    this.setupFileWatching();
  }

  static getInstance(): DynamicConfigurationManager {
    if (!DynamicConfigurationManager.instance) {
      DynamicConfigurationManager.instance = new DynamicConfigurationManager();
    }
    return DynamicConfigurationManager.instance;
  }

  /**
   * Get current configuration (dynamic, can change at runtime)
   */
  getConfig(): DynamicAcademyConfig {
    return { ...this.config }; // Return copy to prevent mutations
  }

  /**
   * Register for configuration change notifications
   */
  onConfigChange(listener: (config: DynamicAcademyConfig) => void): () => void {
    this.changeListeners.push(listener);
    return () => {
      const index = this.changeListeners.indexOf(listener);
      if (index !== -1) {
        this.changeListeners.splice(index, 1);
      }
    };
  }

  /**
   * Update configuration at runtime
   */
  updateConfig(updates: Partial<DynamicAcademyConfig>): void {
    this.config = { ...this.config, ...updates };
    this.notifyListeners();
  }

  /**
   * Load configuration from multiple sources
   * Priority: Environment Variables > Config Files > Defaults
   */
  private loadConfiguration(): DynamicAcademyConfig {
    const baseDir = this.resolveBasePath();
    
    // Start with secure defaults
    const defaultConfig: DynamicAcademyConfig = {
      paths: {
        base_directory: baseDir,
        training_data: path.join(baseDir, 'academy', 'training'),
        model_cache: path.join(baseDir, 'academy', 'models'),
        session_data: path.join(baseDir, 'academy', 'sessions'),
        logs: path.join(baseDir, 'academy', 'logs')
      },
      models: {
        default_provider: process.env.ACADEMY_DEFAULT_PROVIDER || 'local',
        fallback_providers: this.parseProviderList(process.env.ACADEMY_FALLBACK_PROVIDERS) || ['local'],
        model_discovery_enabled: process.env.ACADEMY_MODEL_DISCOVERY === 'true',
        auto_scaling_enabled: process.env.ACADEMY_AUTO_SCALING === 'true'
      },
      performance: {
        max_concurrent_sessions: parseInt(process.env.ACADEMY_MAX_SESSIONS || '3'),
        evaluation_interval_ms: parseInt(process.env.ACADEMY_EVAL_INTERVAL || '30000'),
        health_check_interval_ms: parseInt(process.env.ACADEMY_HEALTH_INTERVAL || '10000'),
        auto_cleanup_enabled: process.env.ACADEMY_AUTO_CLEANUP !== 'false',
        resource_monitoring_enabled: process.env.ACADEMY_MONITORING !== 'false'
      },
      features: {
        local_mode: process.env.ACADEMY_LOCAL_MODE !== 'false',
        p2p_enabled: process.env.ACADEMY_P2P_ENABLED === 'true',
        distributed_training: process.env.ACADEMY_DISTRIBUTED === 'true',
        real_time_metrics: process.env.ACADEMY_REAL_TIME_METRICS !== 'false'
      },
      persona: {
        auto_discovery: process.env.ACADEMY_PERSONA_AUTO_DISCOVERY !== 'false',
        capability_inference: process.env.ACADEMY_CAPABILITY_INFERENCE === 'true',
        dynamic_specialization: process.env.ACADEMY_DYNAMIC_SPECIALIZATION === 'true'
      }
    };

    // Override with file-based configuration if it exists
    const fileConfig = this.loadFileConfiguration();
    if (fileConfig) {
      return this.mergeConfigurations(defaultConfig, fileConfig);
    }

    return defaultConfig;
  }

  /**
   * Resolve base path dynamically based on environment
   */
  private resolveBasePath(): string {
    // Priority: Explicit env var > Project discovery > Default
    if (process.env.CONTINUUM_BASE_PATH) {
      return process.env.CONTINUUM_BASE_PATH;
    }

    // Dynamic project root discovery
    let currentDir = process.cwd();
    while (currentDir !== path.dirname(currentDir)) {
      if (fs.existsSync(path.join(currentDir, '.continuum'))) {
        return path.join(currentDir, '.continuum');
      }
      if (fs.existsSync(path.join(currentDir, 'package.json'))) {
        const packageJson = JSON.parse(
          fs.readFileSync(path.join(currentDir, 'package.json'), 'utf8')
        );
        if (packageJson.name === 'continuum' || packageJson.continuum) {
          return path.join(currentDir, '.continuum');
        }
      }
      currentDir = path.dirname(currentDir);
    }

    // Fallback to current working directory
    return path.join(process.cwd(), '.continuum');
  }

  /**
   * Load configuration from file if it exists
   */
  private loadFileConfiguration(): Partial<DynamicAcademyConfig> | null {
    const configPaths = [
      path.join(this.resolveBasePath(), 'academy-config.json'),
      path.join(process.cwd(), 'academy-config.json'),
      path.join(process.cwd(), 'config', 'academy.json')
    ];

    for (const configPath of configPaths) {
      try {
        if (fs.existsSync(configPath)) {
          const content = fs.readFileSync(configPath, 'utf8');
          return JSON.parse(content);
        }
      } catch (error) {
        console.warn(`Failed to load config from ${configPath}:`, error);
      }
    }

    return null;
  }

  /**
   * Setup file watching for hot-reloading
   */
  private setupFileWatching(): void {
    const configPaths = [
      path.join(this.resolveBasePath(), 'academy-config.json'),
      path.join(process.cwd(), 'academy-config.json')
    ];

    for (const configPath of configPaths) {
      if (fs.existsSync(configPath)) {
        const watcher = fs.watch(configPath, () => {
          this.reloadConfiguration();
        });
        this.watchers.set(configPath, watcher);
      }
    }
  }

  /**
   * Reload configuration from all sources
   */
  private reloadConfiguration(): void {
    try {
      this.config = this.loadConfiguration();
      this.notifyListeners();
      console.log('🔄 Academy configuration reloaded dynamically');
    } catch (error) {
      console.error('❌ Failed to reload configuration:', error);
    }
  }

  /**
   * Notify all change listeners
   */
  private notifyListeners(): void {
    for (const listener of this.changeListeners) {
      try {
        listener(this.config);
      } catch (error) {
        console.error('Configuration change listener error:', error);
      }
    }
  }

  /**
   * Parse comma-separated provider list from environment
   */
  private parseProviderList(envValue?: string): string[] | undefined {
    if (!envValue) return undefined;
    return envValue.split(',').map(p => p.trim()).filter(p => p.length > 0);
  }

  /**
   * Merge configurations with deep merge for nested objects
   */
  private mergeConfigurations(
    base: DynamicAcademyConfig, 
    override: Partial<DynamicAcademyConfig>
  ): DynamicAcademyConfig {
    return {
      paths: { ...base.paths, ...override.paths },
      models: { ...base.models, ...override.models },
      performance: { ...base.performance, ...override.performance },
      features: { ...base.features, ...override.features },
      persona: { ...base.persona, ...override.persona }
    };
  }

  /**
   * Cleanup watchers on shutdown
   */
  destroy(): void {
    for (const watcher of this.watchers.values()) {
      watcher.close();
    }
    this.watchers.clear();
    this.changeListeners.length = 0;
  }
}

/**
 * Legacy configuration adapter for backward compatibility
 * TODO: ROADMAP_ITEM - Remove this adapter once all consumers use dynamic config
 */
export function createLegacyConfig(dynamicConfig: DynamicAcademyConfig) {
  return {
    local_mode: dynamicConfig.features.local_mode,
    p2p_enabled: dynamicConfig.features.p2p_enabled,
    max_concurrent_sessions: dynamicConfig.performance.max_concurrent_sessions,
    training_data_path: dynamicConfig.paths.training_data,
    model_cache_path: dynamicConfig.paths.model_cache,
    evaluation_interval_ms: dynamicConfig.performance.evaluation_interval_ms
  };
}

/**
 * Environment variable documentation for dynamic configuration
 * TODO: ROADMAP_ITEM - Generate this documentation automatically
 */
export const ENVIRONMENT_VARIABLES = {
  // Base Configuration
  'CONTINUUM_BASE_PATH': 'Override base directory path for all Academy data',
  
  // Model Configuration  
  'ACADEMY_DEFAULT_PROVIDER': 'Default model provider (local, openai, anthropic, etc.)',
  'ACADEMY_FALLBACK_PROVIDERS': 'Comma-separated list of fallback providers',
  'ACADEMY_MODEL_DISCOVERY': 'Enable automatic model discovery (true/false)',
  'ACADEMY_AUTO_SCALING': 'Enable automatic model scaling (true/false)',
  
  // Performance Configuration
  'ACADEMY_MAX_SESSIONS': 'Maximum concurrent training sessions (number)',
  'ACADEMY_EVAL_INTERVAL': 'Evaluation interval in milliseconds (number)',
  'ACADEMY_HEALTH_INTERVAL': 'Health check interval in milliseconds (number)',
  'ACADEMY_AUTO_CLEANUP': 'Enable automatic resource cleanup (true/false)',
  'ACADEMY_MONITORING': 'Enable resource monitoring (true/false)',
  
  // Feature Flags
  'ACADEMY_LOCAL_MODE': 'Enable local-only mode (true/false)',
  'ACADEMY_P2P_ENABLED': 'Enable peer-to-peer networking (true/false)',
  'ACADEMY_DISTRIBUTED': 'Enable distributed training (true/false)',
  'ACADEMY_REAL_TIME_METRICS': 'Enable real-time metrics (true/false)',
  
  // Persona Configuration
  'ACADEMY_PERSONA_AUTO_DISCOVERY': 'Enable automatic persona discovery (true/false)',
  'ACADEMY_CAPABILITY_INFERENCE': 'Enable capability inference (true/false)',
  'ACADEMY_DYNAMIC_SPECIALIZATION': 'Enable dynamic specialization (true/false)'
} as const;

// Singleton instance for global access
export const dynamicConfig = DynamicConfigurationManager.getInstance();