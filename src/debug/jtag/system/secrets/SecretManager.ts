/**
 * SecretManager - Secure API Key and Secrets Management
 * ======================================================
 *
 * SECURITY PRINCIPLES:
 * 1. **Server-Side Only** - Secrets NEVER sent to browser
 * 2. **Environment Isolation** - Loaded once on server startup
 * 3. **Automatic Redaction** - Keys filtered from logs/screenshots/errors
 * 4. **Audit Trail** - Track access for security review
 * 5. **Graceful Degradation** - Missing keys don't crash system
 *
 * Usage:
 * ```typescript
 * // Server-side only!
 * const secrets = SecretManager.getInstance();
 * const openaiKey = secrets.get('OPENAI_API_KEY');  // Returns key or undefined
 * const required = secrets.require('ANTHROPIC_API_KEY');  // Throws if missing
 * ```
 *
 * Loading priority:
 * 1. ~/.continuum/config.env (user's home directory)
 * 2. process.env (system environment variables)
 * 3. .env file (project-local, NOT committed to git)
 */

import * as fs from 'fs';
import * as path from 'path';
import * as os from 'os';

export interface SecretAccessLog {
  key: string;
  accessedAt: number;
  requestedBy: string;
  environment: 'server' | 'browser';
}

export class SecretManager {
  private static instance: SecretManager | null = null;
  private secrets: Map<string, string> = new Map();
  private accessLog: SecretAccessLog[] = [];
  private isInitialized = false;

  private constructor() {
    // Private constructor for singleton
  }

  /**
   * Get singleton instance
   */
  static getInstance(): SecretManager {
    if (!SecretManager.instance) {
      SecretManager.instance = new SecretManager();
    }
    return SecretManager.instance;
  }

  /**
   * Check if verbose logging is enabled (static helper for common pattern)
   * Works in both browser and server environments
   *
   * Server: Checks process.env.JTAG_VERBOSE for truthy values (1, true, yes, on)
   * Browser: Checks window.JTAG_VERBOSE for boolean true
   */
  static isVerbose(): boolean {
    if (typeof window !== 'undefined') {
      return (window as any).JTAG_VERBOSE === true;
    }
    if (typeof process !== 'undefined' && process.env?.JTAG_VERBOSE) {
      const value = process.env.JTAG_VERBOSE.toLowerCase().trim();
      return ['1', 'true', 'yes', 'on'].includes(value);
    }
    return false;
  }

  /**
   * Initialize secrets from config.env
   * Called once during server startup
   */
  async initialize(): Promise<void> {
    if (this.isInitialized) {
      if (process.env.JTAG_VERBOSE === '1') {
        console.log('🔐 SecretManager: Already initialized');
      }
      return;
    }

    if (process.env.JTAG_VERBOSE === '1') {
      console.log('🔐 SecretManager: Initializing secrets...');
    }

    // Load from ~/.continuum/config.env (primary source)
    await this.loadFromHomeConfig();

    // Load from process.env (fallback/override)
    this.loadFromProcessEnv();

    // Load from .env file (local development)
    await this.loadFromProjectEnv();

    this.isInitialized = true;
    if (process.env.JTAG_VERBOSE === '1') {
      console.log(`✅ SecretManager: Loaded ${this.secrets.size} secrets`);
    }
  }

  /**
   * Initialize secrets synchronously (for CLI and scripts)
   * Use this when top-level await is not available
   */
  initializeSync(): void {
    if (this.isInitialized) {
      if (process.env.JTAG_VERBOSE === '1') {
        console.log('🔐 SecretManager: Already initialized');
      }
      return;
    }

    if (process.env.JTAG_VERBOSE === '1') {
      console.log('🔐 SecretManager: Initializing secrets (sync)...');
    }

    // Load from ~/.continuum/config.env (primary source) - synchronous
    this.loadFromHomeConfigSync();

    // Load from process.env (fallback/override)
    this.loadFromProcessEnv();

    // Load from .env file (local development) - synchronous
    this.loadFromProjectEnvSync();

    this.isInitialized = true;
    if (process.env.JTAG_VERBOSE === '1') {
      console.log(`✅ SecretManager: Loaded ${this.secrets.size} secrets`);
    }
  }

  /**
   * Get secret value (returns undefined if not found)
   * @param key - Secret key (e.g., 'OPENAI_API_KEY')
   * @param requestedBy - Who is requesting (for audit trail)
   */
  get(key: string, requestedBy = 'unknown'): string | undefined {
    this.logAccess(key, requestedBy);

    return this.secrets.get(key);
  }

  /**
   * Get secret value (throws if not found)
   * @param key - Secret key (e.g., 'ANTHROPIC_API_KEY')
   * @param requestedBy - Who is requesting (for audit trail)
   */
  require(key: string, requestedBy = 'unknown'): string {
    const value = this.get(key, requestedBy);

    if (!value) {
      throw new Error(
        `Missing required secret: ${key}\n` +
        `Please add it to ~/.continuum/config.env:\n` +
        `${key}=your-key-here`
      );
    }

    return value;
  }

  /**
   * Check if secret exists
   */
  has(key: string): boolean {
    return this.secrets.has(key);
  }

  /**
   * Get config value as boolean
   * Accepts: 1, true, yes, on (case-insensitive) → true
   * Accepts: 0, false, no, off, '' (case-insensitive) → false
   * Returns defaultValue if key not found
   */
  getBoolean(key: string, defaultValue = false): boolean {
    const value = this.secrets.get(key);
    if (value === undefined) {
      return defaultValue;
    }
    const normalized = value.toLowerCase().trim();
    return ['1', 'true', 'yes', 'on'].includes(normalized);
  }

  /**
   * Get config value as number
   * Returns defaultValue if key not found or not a valid number
   */
  getNumber(key: string, defaultValue = 0): number {
    const value = this.secrets.get(key);
    if (value === undefined) {
      return defaultValue;
    }
    const parsed = parseFloat(value);
    return isNaN(parsed) ? defaultValue : parsed;
  }

  /**
   * Get list of available secret keys (NOT values!)
   * Safe to expose to browser for UI rendering
   */
  getAvailableKeys(): string[] {
    return Array.from(this.secrets.keys());
  }

  /**
   * Set a secret (for widget-based user input)
   * IMPORTANT: Only call this from secure server-side code!
   */
  async set(key: string, value: string): Promise<void> {
    this.secrets.set(key, value);

    // Persist to ~/.continuum/config.env
    await this.persistToHomeConfig(key, value);

    console.log(`🔐 SecretManager: Set ${key} (redacted)`);
  }

  /**
   * Remove a secret
   */
  async remove(key: string): Promise<void> {
    this.secrets.delete(key);

    // Remove from ~/.continuum/config.env
    await this.removeFromHomeConfig(key);

    console.log(`🔐 SecretManager: Removed ${key}`);
  }

  /**
   * Redact secret values from text (for logs/screenshots)
   * Replaces actual keys with [REDACTED-xxx]
   */
  redact(text: string): string {
    let redacted = text;

    for (const [key, value] of this.secrets) {
      if (value && value.length > 0) {
        // Replace the actual key value with a redacted version
        const redactionPattern = new RegExp(this.escapeRegex(value), 'g');
        redacted = redacted.replace(redactionPattern, `[REDACTED-${key}]`);
      }
    }

    return redacted;
  }

  /**
   * Get access audit trail (for security review)
   */
  getAuditLog(): SecretAccessLog[] {
    return [...this.accessLog];
  }

  // ========================
  // Private Methods
  // ========================

  /**
   * Load from ~/.continuum/config.env
   */
  private async loadFromHomeConfig(): Promise<void> {
    const homeDir = os.homedir();
    const configPath = path.join(homeDir, '.continuum', 'config.env');

    try {
      if (!fs.existsSync(configPath)) {
        console.log(`ℹ️  SecretManager: No config.env found at ${configPath}`);
        return;
      }

      const content = fs.readFileSync(configPath, 'utf-8');
      this.parseEnvFile(content, 'home-config');

      console.log(`✅ SecretManager: Loaded secrets from ${configPath}`);
    } catch (error) {
      console.warn(`⚠️  SecretManager: Failed to load home config:`, error);
    }
  }

  /**
   * Load from ~/.continuum/config.env (synchronous version)
   */
  private loadFromHomeConfigSync(): void {
    const homeDir = os.homedir();
    const configPath = path.join(homeDir, '.continuum', 'config.env');

    try {
      if (!fs.existsSync(configPath)) {
        if (process.env.JTAG_VERBOSE === '1') {
          console.log(`ℹ️  SecretManager: No config.env found at ${configPath}`);
        }
        return;
      }

      const content = fs.readFileSync(configPath, 'utf-8');
      this.parseEnvFile(content, 'home-config');

      if (process.env.JTAG_VERBOSE === '1') {
        console.log(`✅ SecretManager: Loaded secrets from ${configPath}`);
      }
    } catch (error) {
      console.warn(`⚠️  SecretManager: Failed to load home config:`, error);
    }
  }

  /**
   * Load from process.env
   */
  private loadFromProcessEnv(): void {
    // Only load API key environment variables (security best practice)
    const apiKeyPattern = /^[A-Z_]+_(API_)?KEY$/;

    for (const [key, value] of Object.entries(process.env)) {
      if (apiKeyPattern.test(key) && value) {
        this.secrets.set(key, value);
      }
    }
  }

  /**
   * Load from .env file (project-local)
   */
  private async loadFromProjectEnv(): Promise<void> {
    const projectEnvPath = path.resolve(process.cwd(), '.env');

    try {
      if (!fs.existsSync(projectEnvPath)) {
        return;
      }

      const content = fs.readFileSync(projectEnvPath, 'utf-8');
      this.parseEnvFile(content, 'project-env');

      console.log(`✅ SecretManager: Loaded secrets from .env`);
    } catch (error) {
      console.warn(`⚠️  SecretManager: Failed to load project .env:`, error);
    }
  }

  /**
   * Load from .env file (project-local) - synchronous version
   */
  private loadFromProjectEnvSync(): void {
    const projectEnvPath = path.resolve(process.cwd(), '.env');

    try {
      if (!fs.existsSync(projectEnvPath)) {
        return;
      }

      const content = fs.readFileSync(projectEnvPath, 'utf-8');
      this.parseEnvFile(content, 'project-env');

      if (process.env.JTAG_VERBOSE === '1') {
        console.log(`✅ SecretManager: Loaded secrets from .env`);
      }
    } catch (error) {
      console.warn(`⚠️  SecretManager: Failed to load project .env:`, error);
    }
  }

  /**
   * Parse .env file format
   */
  private parseEnvFile(content: string, source: string): void {
    const lines = content.split('\n');

    for (const line of lines) {
      const trimmed = line.trim();

      // Skip comments and empty lines
      if (!trimmed || trimmed.startsWith('#')) {
        continue;
      }

      // Parse KEY=value (handles spaces around =)
      const match = trimmed.match(/^([A-Z_0-9]+)\s*=\s*(.*)$/);
      if (match) {
        const [, key, rawValue] = match;

        // Expand tilde (~) to home directory
        let value = rawValue.trim();
        if (value.startsWith('~/')) {
          value = path.join(os.homedir(), value.slice(2));
        }

        // Store in secrets Map
        this.secrets.set(key, value);

        // Also set config vars in process.env for ServerConfig to use
        if (key.startsWith('DATABASE_') || key.startsWith('DATASETS_') ||
            key === 'HTTP_PORT' || key === 'WS_PORT' ||
            key === 'SENTINEL_PATH' || key === 'REPO_PATH') {
          process.env[key] = value;
        }
      }
    }
  }

  /**
   * Persist secret to ~/.continuum/config.env
   */
  private async persistToHomeConfig(key: string, value: string): Promise<void> {
    const homeDir = os.homedir();
    const configDir = path.join(homeDir, '.continuum');
    const configPath = path.join(configDir, 'config.env');

    // Ensure directory exists
    if (!fs.existsSync(configDir)) {
      fs.mkdirSync(configDir, { recursive: true });
    }

    // Read existing config
    let existing: Map<string, string> = new Map();
    if (fs.existsSync(configPath)) {
      const content = fs.readFileSync(configPath, 'utf-8');
      const lines = content.split('\n');
      for (const line of lines) {
        const match = line.match(/^([A-Z_0-9]+)=(.*)$/);
        if (match) {
          existing.set(match[1], match[2]);
        }
      }
    }

    // Update with new value
    existing.set(key, value);

    // Write back
    const newContent = Array.from(existing.entries())
      .map(([k, v]) => `${k}=${v}`)
      .join('\n') + '\n';

    fs.writeFileSync(configPath, newContent, 'utf-8');
  }

  /**
   * Remove secret from ~/.continuum/config.env
   */
  private async removeFromHomeConfig(key: string): Promise<void> {
    const homeDir = os.homedir();
    const configPath = path.join(homeDir, '.continuum', 'config.env');

    if (!fs.existsSync(configPath)) {
      return;
    }

    // Read existing config
    const content = fs.readFileSync(configPath, 'utf-8');
    const lines = content.split('\n');

    // Filter out the key
    const newLines = lines.filter(line => {
      const match = line.match(/^([A-Z_0-9]+)=/);
      return !match || match[1] !== key;
    });

    // Write back
    fs.writeFileSync(configPath, newLines.join('\n'), 'utf-8');
  }

  /**
   * Log secret access (for audit trail)
   */
  private logAccess(key: string, requestedBy: string): void {
    this.accessLog.push({
      key,
      accessedAt: Date.now(),
      requestedBy,
      environment: 'server',  // Always server-side
    });

    // Keep last 1000 access logs
    if (this.accessLog.length > 1000) {
      this.accessLog.shift();
    }
  }

  /**
   * Escape string for regex
   */
  private escapeRegex(str: string): string {
    return str.replace(/[.*+?^${}()|[\]\\]/g, '\\$&');
  }
}

/**
 * Initialize secrets on server startup
 * Called by AIProviderDaemon or system bootstrap
 */
export async function initializeSecrets(): Promise<void> {
  const secrets = SecretManager.getInstance();
  await secrets.initialize();
}

/**
 * Get secret value (server-side only!)
 */
export function getSecret(key: string, requestedBy = 'unknown'): string | undefined {
  return SecretManager.getInstance().get(key, requestedBy);
}

/**
 * Require secret value (throws if missing)
 */
export function requireSecret(key: string, requestedBy = 'unknown'): string {
  return SecretManager.getInstance().require(key, requestedBy);
}

/**
 * Redact secrets from text (for logs/screenshots)
 */
export function redactSecrets(text: string): string {
  return SecretManager.getInstance().redact(text);
}
