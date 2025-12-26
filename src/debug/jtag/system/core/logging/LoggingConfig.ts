/**
 * LoggingConfig - Centralized logging configuration
 *
 * Controls which logs are enabled per persona and per category.
 * Allows focusing on specific areas of concern without drowning in noise.
 *
 * Config file: .continuum/logging.json
 *
 * Usage:
 *   LoggingConfig.isEnabled('helper', 'cognition') // Check if enabled
 *   LoggingConfig.setEnabled('helper', 'cognition', true) // Enable
 *   LoggingConfig.setPersonaEnabled('helper', false) // Disable all for persona
 *
 * Future: ./jtag logging/enable --persona=helper --category=cognition
 */

import * as fs from 'fs';
import * as path from 'path';

/**
 * Per-persona logging configuration
 */
export interface PersonaLoggingConfig {
  /** Master switch for this persona */
  enabled: boolean;
  /** Enabled categories (log file types). Empty = all enabled when persona enabled */
  categories?: string[];
}

/**
 * Full logging configuration structure
 */
export interface LoggingConfigData {
  /** Schema version for future migrations */
  version: 1;

  /** Default settings for personas not explicitly configured */
  defaults: {
    /** Default enabled state for new personas */
    enabled: boolean;
    /** Default categories (empty = all) */
    categories: string[];
  };

  /** Per-persona overrides */
  personas: Record<string, PersonaLoggingConfig>;

  /** System-level logging (non-persona) */
  system?: {
    enabled: boolean;
    categories?: string[];
  };
}

/**
 * Default configuration - all logging OFF by default
 * Use wildcards to enable selectively:
 *   "*": { enabled: true }  - Enable all personas
 *   "helper": { enabled: true, categories: ["*"] }  - Enable all categories for helper
 *   "helper": { enabled: true, categories: ["cognition", "hippocampus"] }  - Specific categories
 */
const DEFAULT_CONFIG: LoggingConfigData = {
  version: 1,
  defaults: {
    enabled: false,  // OFF by default - opt-in logging
    categories: []
  },
  personas: {
    // Wildcard: "*" matches all personas not explicitly configured
    // Example: "*": { enabled: false }  - All off except explicit overrides
  },
  system: {
    enabled: true,  // System logs stay on
    categories: []
  }
};

/**
 * Known logging categories for reference
 */
export const LOGGING_CATEGORIES = {
  // Persona categories
  COGNITION: 'cognition',      // Task processing, decisions, adapter chain
  HIPPOCAMPUS: 'hippocampus',  // Memory consolidation
  TRAINING: 'training',        // Training data accumulation, fine-tuning
  GENOME: 'genome',            // LoRA adapter loading/unloading
  USER: 'user',                // General persona lifecycle
  ADAPTERS: 'adapters',        // AI provider adapter activity

  // System categories (future)
  SERVER: 'server',
  BROWSER: 'browser',
  COMMANDS: 'commands',
  EVENTS: 'events'
} as const;

/**
 * LoggingConfig - Singleton for managing logging configuration
 */
export class LoggingConfig {
  private static instance: LoggingConfig | null = null;
  private config: LoggingConfigData;
  private configPath: string;
  private lastModified: number = 0;

  private constructor(baseDir: string) {
    this.configPath = path.join(baseDir, '.continuum', 'logging.json');
    this.config = this.load();
  }

  /**
   * Initialize with base directory (call once at startup)
   */
  static initialize(baseDir: string): void {
    if (!LoggingConfig.instance) {
      LoggingConfig.instance = new LoggingConfig(baseDir);
    }
  }

  /**
   * Get singleton instance
   */
  static getInstance(): LoggingConfig {
    if (!LoggingConfig.instance) {
      // Fallback: use process.cwd() if not initialized
      LoggingConfig.instance = new LoggingConfig(process.cwd());
    }
    return LoggingConfig.instance;
  }

  /**
   * Check if logging is enabled for a persona + category combination
   */
  static isEnabled(personaId: string, category: string): boolean {
    return LoggingConfig.getInstance().checkEnabled(personaId, category);
  }

  /**
   * Check if system logging is enabled for a category
   */
  static isSystemEnabled(category: string): boolean {
    return LoggingConfig.getInstance().checkSystemEnabled(category);
  }

  /**
   * Enable/disable a specific persona + category
   */
  static setEnabled(personaId: string, category: string, enabled: boolean): void {
    LoggingConfig.getInstance().updateEnabled(personaId, category, enabled);
  }

  /**
   * Enable/disable all logging for a persona
   */
  static setPersonaEnabled(personaId: string, enabled: boolean): void {
    LoggingConfig.getInstance().updatePersonaEnabled(personaId, enabled);
  }

  /**
   * Get current config (for display/debugging)
   */
  static getConfig(): LoggingConfigData {
    return LoggingConfig.getInstance().config;
  }

  /**
   * Reload config from disk (useful if edited externally)
   */
  static reload(): void {
    LoggingConfig.getInstance().config = LoggingConfig.getInstance().load();
  }

  // ===== Instance Methods =====

  private checkEnabled(personaId: string, category: string): boolean {
    // Hot reload: check if file changed
    this.maybeReload();

    // Normalize personaId (handle both uniqueId and displayName)
    const normalizedId = this.normalizePersonaId(personaId);

    // Check persona-specific config first
    let personaConfig = this.config.personas[normalizedId];

    // If no specific config, check wildcard "*"
    if (!personaConfig) {
      personaConfig = this.config.personas['*'];
    }

    if (personaConfig) {
      if (!personaConfig.enabled) return false;

      // Check categories
      if (personaConfig.categories && personaConfig.categories.length > 0) {
        // "*" in categories means all categories enabled
        if (personaConfig.categories.includes('*')) return true;
        return personaConfig.categories.includes(category);
      }
      // No categories specified = all enabled when persona is enabled
      return true;
    }

    // Fall back to defaults
    if (!this.config.defaults.enabled) return false;

    if (this.config.defaults.categories && this.config.defaults.categories.length > 0) {
      if (this.config.defaults.categories.includes('*')) return true;
      return this.config.defaults.categories.includes(category);
    }

    return true;
  }

  private checkSystemEnabled(category: string): boolean {
    this.maybeReload();

    const systemConfig = this.config.system;
    if (!systemConfig || !systemConfig.enabled) return false;

    if (systemConfig.categories && systemConfig.categories.length > 0) {
      return systemConfig.categories.includes(category);
    }

    return true;
  }

  private updateEnabled(personaId: string, category: string, enabled: boolean): void {
    const normalizedId = this.normalizePersonaId(personaId);

    // Ensure persona config exists (don't change enabled state)
    if (!this.config.personas[normalizedId]) {
      this.config.personas[normalizedId] = {
        enabled: false,
        categories: []
      };
    }

    const personaConfig = this.config.personas[normalizedId];

    if (!personaConfig.categories) {
      personaConfig.categories = [];
    }

    // All known categories for converting empty->explicit list
    const allCategories = Object.values(LOGGING_CATEGORIES);

    if (enabled) {
      // ENABLING a category
      if (personaConfig.categories.length === 0 || personaConfig.categories.includes('*')) {
        // Already all enabled - nothing to do
        return;
      }
      // Add category if not present
      if (!personaConfig.categories.includes(category)) {
        personaConfig.categories.push(category);
      }
      // If all categories are now enabled, simplify to empty array (meaning "all")
      if (allCategories.every(c => personaConfig.categories!.includes(c))) {
        personaConfig.categories = [];
      }
    } else {
      // DISABLING a category
      if (personaConfig.categories.length === 0 || personaConfig.categories.includes('*')) {
        // Currently "all enabled" - convert to explicit list of all EXCEPT the disabled one
        personaConfig.categories = allCategories.filter(c => c !== category);
      } else {
        // Remove category from explicit list
        personaConfig.categories = personaConfig.categories.filter(c => c !== category);
      }
    }

    // Individual toggles don't change persona.enabled - that's controlled by global toggle only

    this.save();
  }

  private updatePersonaEnabled(personaId: string, enabled: boolean): void {
    const normalizedId = this.normalizePersonaId(personaId);

    if (!this.config.personas[normalizedId]) {
      this.config.personas[normalizedId] = { enabled, categories: [] };
    } else {
      this.config.personas[normalizedId].enabled = enabled;
    }

    this.save();
  }

  private normalizePersonaId(personaId: string): string {
    // Convert display names to lowercase slug format
    // "Helper AI" -> "helper", "CodeReview AI" -> "codereview"
    return personaId
      .toLowerCase()
      .replace(/\s+ai$/i, '')  // Remove " AI" suffix
      .replace(/\s+/g, '-')    // Spaces to dashes
      .replace(/[^a-z0-9-]/g, ''); // Remove special chars
  }

  private load(): LoggingConfigData {
    try {
      if (fs.existsSync(this.configPath)) {
        const content = fs.readFileSync(this.configPath, 'utf-8');
        const stats = fs.statSync(this.configPath);
        this.lastModified = stats.mtimeMs;
        return { ...DEFAULT_CONFIG, ...JSON.parse(content) };
      }
    } catch (error) {
      console.warn(`LoggingConfig: Failed to load ${this.configPath}, using defaults`);
    }

    // Create default config file
    this.config = DEFAULT_CONFIG;
    this.save();
    return DEFAULT_CONFIG;
  }

  private save(): void {
    try {
      const dir = path.dirname(this.configPath);
      if (!fs.existsSync(dir)) {
        fs.mkdirSync(dir, { recursive: true });
      }
      fs.writeFileSync(this.configPath, JSON.stringify(this.config, null, 2));
      const stats = fs.statSync(this.configPath);
      this.lastModified = stats.mtimeMs;
    } catch (error) {
      console.error(`LoggingConfig: Failed to save ${this.configPath}:`, error);
    }
  }

  private maybeReload(): void {
    // Hot reload if file was modified externally
    try {
      if (fs.existsSync(this.configPath)) {
        const stats = fs.statSync(this.configPath);
        if (stats.mtimeMs > this.lastModified) {
          this.config = this.load();
        }
      }
    } catch {
      // Ignore errors during hot reload check
    }
  }
}
