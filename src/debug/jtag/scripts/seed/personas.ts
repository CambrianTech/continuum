/**
 * Persona Configuration - Single Source of Truth
 *
 * All persona definitions in one place for easy maintenance.
 * Used by seed-continuum.ts to create persona users.
 *
 * uniqueId format: Simple slug WITHOUT @ prefix
 * Examples: claude, helper, grok, sentinel
 *
 * The @ symbol is ONLY for UI mentions, NOT part of uniqueId
 */

import { generateUniqueId } from '../../system/data/utils/UniqueIdUtils';

export interface PersonaConfig {
  uniqueId: string;
  displayName: string;
  provider?: string;
  type: 'agent' | 'persona';
}

/**
 * Complete list of all personas in the system
 * uniqueId is clean slug (no @ prefix, no UUID suffix)
 *
 * generateUniqueId() now returns clean slugs without @ prefix
 */
export const PERSONA_CONFIGS: PersonaConfig[] = [
  // Core agents
  { uniqueId: generateUniqueId('Claude'), displayName: 'Claude Code', provider: 'anthropic', type: 'agent' },
  { uniqueId: generateUniqueId('General'), displayName: 'General AI', provider: 'anthropic', type: 'agent' },

  // Local personas (Ollama-based - Candle has mutex blocking issue)
  { uniqueId: generateUniqueId('Helper'), displayName: 'Helper AI', provider: 'ollama', type: 'persona' },
  { uniqueId: generateUniqueId('Teacher'), displayName: 'Teacher AI', provider: 'ollama', type: 'persona' },
  { uniqueId: generateUniqueId('CodeReview'), displayName: 'CodeReview AI', provider: 'ollama', type: 'persona' },

  // Cloud provider personas
  { uniqueId: generateUniqueId('DeepSeek'), displayName: 'DeepSeek Assistant', provider: 'deepseek', type: 'persona' },
  { uniqueId: generateUniqueId('Groq'), displayName: 'Groq Lightning', provider: 'groq', type: 'persona' },
  { uniqueId: generateUniqueId('Claude Assistant'), displayName: 'Claude Assistant', provider: 'anthropic', type: 'persona' },
  { uniqueId: generateUniqueId('GPT'), displayName: 'GPT Assistant', provider: 'openai', type: 'persona' },
  { uniqueId: generateUniqueId('Grok'), displayName: 'Grok', provider: 'xai', type: 'persona' },
  { uniqueId: generateUniqueId('Together'), displayName: 'Together Assistant', provider: 'together', type: 'persona' },
  { uniqueId: generateUniqueId('Fireworks'), displayName: 'Fireworks AI', provider: 'fireworks', type: 'persona' },
  { uniqueId: generateUniqueId('Local'), displayName: 'Local Assistant', provider: 'ollama', type: 'persona' },
  { uniqueId: generateUniqueId('Sentinel'), displayName: 'Sentinel', provider: 'sentinel', type: 'persona' },
];

/**
 * Helper constants for commonly referenced personas
 */
export const PERSONA_UNIQUE_IDS = {
  CLAUDE: generateUniqueId('Claude'),
  GENERAL: generateUniqueId('General'),
  HELPER: generateUniqueId('Helper'),
  TEACHER: generateUniqueId('Teacher'),
  CODE_REVIEW: generateUniqueId('CodeReview'),
  DEEPSEEK: generateUniqueId('DeepSeek'),
  GROQ: generateUniqueId('Groq'),
  CLAUDE_ASSISTANT: generateUniqueId('Claude Assistant'),
  GPT: generateUniqueId('GPT'),
  GROK: generateUniqueId('Grok'),
  TOGETHER: generateUniqueId('Together'),
  FIREWORKS: generateUniqueId('Fireworks'),
  LOCAL: generateUniqueId('Local'),
  SENTINEL: generateUniqueId('Sentinel'),
} as const;
