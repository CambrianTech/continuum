#!/usr/bin/env tsx
/**
 * Ensure config.env exists with proper defaults
 *
 * - Creates ~/.continuum/config.env if it doesn't exist
 * - Never overwrites existing config
 * - Provides helpful defaults and documentation
 */

import * as fs from 'fs';
import * as path from 'path';
import * as os from 'os';

const CONFIG_DIR = path.join(os.homedir(), '.continuum');
const CONFIG_PATH = path.join(CONFIG_DIR, 'config.env');

const DEFAULT_CONFIG = `# ============================================
# CONTINUUM CONFIGURATION
# ============================================
# This file is auto-generated on first run and will NOT be overwritten.
# Edit values as needed for your environment.

# ============================================
# NETWORKING
# ============================================
# Controls network ports for HTTP and WebSocket servers

# HTTP Port - Port for HTTP server
HTTP_PORT=9000

# WebSocket Port - Port for WebSocket server
WS_PORT=9001

# ============================================
# LOGGER CONFIGURATION
# ============================================
# Controls system-wide logging behavior via Logger.ts

# Log Level - Controls verbosity of logs
# Values: debug, info, warn, error, silent
# - debug: Everything (verbose, for debugging only)
# - info: Info, warnings, errors
# - warn: Warnings and errors (default — keeps system quiet)
# - error: Only errors
# - silent: No logs
# Per-component overrides via LogLevelRegistry (runtime mutable)
LOG_LEVEL=warn

# Timestamps - Add timestamps to log entries
# Values: 0 (disabled), 1 (enabled)
# Default: 1 (enabled)
LOG_TIMESTAMPS=1

# Console Logging - Output logs to stdout/stderr
# Values: 0 (disabled), 1 (enabled)
# Default: 1 (enabled for visibility)
LOG_TO_CONSOLE=1

# File Logging - Write logs to .continuum/*/logs/
# Values: 0 (disabled), 1 (enabled)
# Default: 1 (enabled)
LOG_TO_FILES=1

# File Mode - How to handle existing log files on restart
# Values: clean, append, archive
# - clean: Start fresh each session (truncate existing logs)
# - append: Keep existing logs and add to them
# - archive: Rotate logs (not implemented yet, falls back to append)
# Default: clean
LOG_FILE_MODE=clean

# Performance Timing - Record operation timing to /tmp/jtag-timing.jsonl
# Values: true (enabled), false (disabled)
# Default: false (enable when analyzing performance)
JTAG_TIMING_ENABLED=false

# ============================================
# API KEYS
# ============================================
# Add your API keys below. Leave blank if not using a provider.
# These are required for AI features to work with external providers.

# Anthropic (Claude models)
ANTHROPIC_API_KEY=

# OpenAI (GPT models)
OPENAI_API_KEY=

# X.AI (Grok models)
XAI_API_KEY=

# DeepSeek
DEEPSEEK_API_KEY=

# Together AI
TOGETHER_API_KEY=

# Fireworks AI
FIREWORKS_API_KEY=
FIREWORKS_ACCOUNT_ID=

# Groq (fast inference)
GROQ_API_KEY=

# Mistral AI
MISTRAL_API_KEY=

# HuggingFace - https://huggingface.co/settings/tokens
HF_TOKEN=

# ============================================
# DATABASE PATHS
# ============================================
# Override default database locations if needed.
# By default, databases are stored in ~/.continuum/data
# Uncomment and modify these paths to use custom locations.

# Main database directory (SQLite files)
#DATABASE_DIR=~/.continuum/data

# Database backups directory
#DATABASE_BACKUP_DIR=~/.continuum/data/backups

# Database archive directory (for archived data)
#DATABASE_ARCHIVE_DIR=~/.continuum/data/archive

# ============================================
# DATASETS AND EXTENSIONS
# ============================================

# Datasets directory - Location for training datasets
# Uncomment and set if you have a custom datasets location
#DATASETS_DIR=/path/to/datasets

# Sentinel-AI Path - Path to Sentinel AI system (if installed)
# Uncomment to enable Sentinel AI integration
#SENTINEL_PATH=/path/to/sentinel-ai

# ============================================
# LOCAL INFERENCE
# ============================================

# Inference Mode - Controls local model loading strategy
# Values: auto, quantized, bf16
# - auto: BF16 (full LoRA support), fallback to quantized if needed
# - quantized: Force Q4_K_M quantized (~2GB, 2s load, NO LoRA support)
# - bf16: Force full-precision BF16 (~6GB, 14s load, full LoRA)
# Default: auto (BF16 for LoRA support)
INFERENCE_MODE=auto

# ============================================
# VOICE MODELS
# ============================================

# Whisper STT Model - Speech-to-text model selection
# Values: base, small, medium, large-v3, large-v3-turbo
# - base: ~74MB, fastest, ~60-70% accuracy (not recommended)
# - small: ~244MB, fast, ~75-80% accuracy
# - medium: ~1.5GB, balanced, ~75-85% accuracy
# - large-v3: ~3GB, best accuracy ~90-95%, slower
# - large-v3-turbo: ~1.5GB, best balance ~90-95% accuracy, 6x faster than large-v3
# Default: large-v3-turbo (best balance for real-time use)
WHISPER_MODEL=large-v3-turbo

# ============================================
# FEATURE FLAGS
# ============================================

# Use Rust AI Provider - Enable Rust-based AI provider for better performance
# Values: 0 (disabled), 1 (enabled)
# Default: 0 (disabled)
USE_RUST_AI_PROVIDER=0

# Enable Archive Daemon - Enable automatic data archiving
# Values: 0 (disabled), 1 (enabled)
# Default: 0 (disabled)
ENABLE_ARCHIVE_DAEMON=0
`;

/**
 * Extract configuration keys from a config file content
 * Returns Set of keys (ignoring comments and empty lines)
 */
function extractConfigKeys(content: string): Set<string> {
  const keys = new Set<string>();
  const lines = content.split('\n');

  for (const line of lines) {
    const trimmed = line.trim();
    // Skip comments and empty lines
    if (!trimmed || trimmed.startsWith('#')) continue;

    // Extract key from KEY=value or #KEY=value
    const match = trimmed.match(/^#?\s*([A-Z_][A-Z0-9_]*)=/);
    if (match) {
      keys.add(match[1]);
    }
  }

  return keys;
}

/**
 * Extract config sections with their keys for smart merging
 */
function extractConfigSections(content: string): Map<string, string[]> {
  const sections = new Map<string, string[]>();
  const lines = content.split('\n');
  let currentSection = 'header';
  let currentLines: string[] = [];

  for (const line of lines) {
    // Detect section headers
    if (line.trim().startsWith('# ===')) {
      // Save previous section
      if (currentLines.length > 0) {
        sections.set(currentSection, currentLines);
      }
      // Start new section
      currentLines = [line];
      const sectionMatch = lines[lines.indexOf(line) + 1]?.match(/# (.+)/);
      if (sectionMatch) {
        currentSection = sectionMatch[1].toLowerCase().replace(/\s+/g, '_');
      }
    } else {
      currentLines.push(line);
    }
  }

  // Save last section
  if (currentLines.length > 0) {
    sections.set(currentSection, currentLines);
  }

  return sections;
}

async function ensureConfig(): Promise<void> {
  try {
    // Ensure config directory exists
    if (!fs.existsSync(CONFIG_DIR)) {
      fs.mkdirSync(CONFIG_DIR, { recursive: true });
      console.log(`✅ Created config directory: ${CONFIG_DIR}`);
    }

    // Check if config already exists
    if (fs.existsSync(CONFIG_PATH)) {
      // Config exists - check for new keys
      const existingContent = fs.readFileSync(CONFIG_PATH, 'utf8');
      const existingKeys = extractConfigKeys(existingContent);
      const defaultKeys = extractConfigKeys(DEFAULT_CONFIG);

      // Find new keys in default config that don't exist in user's config
      const newKeys = new Set<string>();
      for (const key of defaultKeys) {
        if (!existingKeys.has(key)) {
          newKeys.add(key);
        }
      }

      if (newKeys.size > 0) {
        console.log(`📝 Found ${newKeys.size} new configuration key(s): ${Array.from(newKeys).join(', ')}`);
        console.log(`   Add them manually from the template or regenerate config`);
        console.log(`   Template: src/scripts/ensure-config.ts`);
      } else {
        console.log(`✅ Config up to date: ${CONFIG_PATH}`);
      }
      return;
    }

    // Create new config with defaults
    fs.writeFileSync(CONFIG_PATH, DEFAULT_CONFIG, 'utf8');
    console.log(`✅ Created default config: ${CONFIG_PATH}`);
    console.log('');
    console.log('📝 Next steps:');
    console.log('   1. Edit the config file to add your API keys');
    console.log('   2. Customize paths and feature flags as needed');
    console.log(`   3. Run: open ${CONFIG_PATH}`);

  } catch (error) {
    const errorMsg = error instanceof Error ? error.message : String(error);
    console.error(`❌ Failed to create config: ${errorMsg}`);
    process.exit(1);
  }
}

// Run if called directly
if (import.meta.url === `file://${process.argv[1]}`) {
  ensureConfig();
}

export { ensureConfig, CONFIG_PATH, CONFIG_DIR };
