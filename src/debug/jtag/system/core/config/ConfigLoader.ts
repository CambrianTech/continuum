/**
 * ConfigLoader - Load environment variables from config.env into process.env
 * ============================================================================
 *
 * CRITICAL: This must run BEFORE Logger initialization!
 *
 * Purpose:
 * SecretManager only loads API keys (pattern: /^[A-Z_]+_(API_)?KEY$/)
 * But we need ALL environment variables from config.env (LOG_TO_CONSOLE, LOG_TO_FILES, etc.)
 *
 * Strategy:
 * 1. Load ~/.continuum/config.env FIRST (before any other initialization)
 * 2. Set ALL variables in process.env (not just API keys)
 * 3. Then SecretManager can filter for secrets
 * 4. Then Logger can read LOG_TO_CONSOLE, LOG_TO_FILES from process.env
 *
 * Usage:
 * ```typescript
 * // At system startup (BEFORE Logger)
 * import { loadConfig } from './ConfigLoader';
 * await loadConfig();
 * ```
 */

import * as fs from 'fs';
import * as path from 'path';
import * as os from 'os';

/**
 * Load environment variables from ~/.continuum/config.env into process.env
 * MUST be called before Logger initializes
 */
export async function loadConfig(): Promise<void> {
  const homeDir = os.homedir();
  const configPath = path.join(homeDir, '.continuum', 'config.env');

  try {
    if (!fs.existsSync(configPath)) {
      console.log(`ℹ️  ConfigLoader: No config.env found at ${configPath}`);
      return;
    }

    const content = fs.readFileSync(configPath, 'utf-8');
    const lines = content.split('\n');

    let loaded = 0;
    for (const line of lines) {
      const trimmed = line.trim();

      // Skip comments and empty lines
      if (!trimmed || trimmed.startsWith('#')) {
        continue;
      }

      // Parse KEY=value (support lowercase keys for non-secrets)
      const match = trimmed.match(/^([A-Z_0-9]+)=(.*)$/);
      if (match) {
        const [, key, value] = match;

        // Only set if not already in process.env (system env takes precedence)
        if (!process.env[key]) {
          process.env[key] = value;
          loaded++;
        }
      }
    }

    console.log(`✅ ConfigLoader: Loaded ${loaded} environment variables from ${configPath}`);
  } catch (error) {
    console.warn(`⚠️  ConfigLoader: Failed to load config:`, error);
  }
}
