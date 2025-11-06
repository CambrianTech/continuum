/**
 * Persona Configuration - Single Source of Truth
 *
 * All persona definitions in one place for easy maintenance.
 * Used by seed-continuum.ts to create persona users.
 *
 * uniqueId format: @username (short, simple, no spaces)
 * Examples: @claude, @helper, @sentinel
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
 * uniqueId is auto-generated from displayName using generateUniqueId()
 */
export const PERSONA_CONFIGS: PersonaConfig[] = [
  // Core agents
  { uniqueId: generateUniqueId('Claude'), displayName: 'Claude Code', provider: 'anthropic', type: 'agent' },
  { uniqueId: generateUniqueId('General'), displayName: 'General AI', provider: 'anthropic', type: 'agent' },

  // Local personas (Ollama-based)
  { uniqueId: generateUniqueId('Helper'), displayName: 'Helper AI', type: 'persona' },
  { uniqueId: generateUniqueId('Teacher'), displayName: 'Teacher AI', type: 'persona' },
  { uniqueId: generateUniqueId('CodeReview'), displayName: 'CodeReview AI', type: 'persona' },

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
 * Export the uniqueId directly so callers can use constants, not magic strings
 * Using type as stable lookup property (more robust than array indices)
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
