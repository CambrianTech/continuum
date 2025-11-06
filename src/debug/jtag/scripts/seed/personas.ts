/**
 * Persona Configuration - Single Source of Truth
 *
 * All persona definitions in one place for easy maintenance.
 * Used by seed-continuum.ts to create persona users.
 */

export interface PersonaConfig {
  uniqueId: string;
  displayName: string;
  provider?: string;
  type: 'agent' | 'persona';
}

/**
 * Complete list of all personas in the system
 */
export const PERSONA_CONFIGS: PersonaConfig[] = [
  // Core agents (use DEFAULT_USER_UNIQUE_IDS)
  { uniqueId: 'claude-code', displayName: 'Claude Code', provider: 'anthropic', type: 'agent' },
  { uniqueId: 'general-ai', displayName: 'General AI', provider: 'anthropic', type: 'agent' },

  // Local personas (Ollama-based)
  { uniqueId: 'persona-helper-001', displayName: 'Helper AI', type: 'persona' },
  { uniqueId: 'persona-teacher-001', displayName: 'Teacher AI', type: 'persona' },
  { uniqueId: 'persona-codereview-001', displayName: 'CodeReview AI', type: 'persona' },

  // Cloud provider personas
  { uniqueId: 'persona-deepseek', displayName: 'DeepSeek Assistant', provider: 'deepseek', type: 'persona' },
  { uniqueId: 'persona-groq', displayName: 'Groq Lightning', provider: 'groq', type: 'persona' },
  { uniqueId: 'persona-anthropic', displayName: 'Claude Assistant', provider: 'anthropic', type: 'persona' },
  { uniqueId: 'persona-openai', displayName: 'GPT Assistant', provider: 'openai', type: 'persona' },
  { uniqueId: 'persona-xai', displayName: 'Grok', provider: 'xai', type: 'persona' },
  { uniqueId: 'persona-together', displayName: 'Together Assistant', provider: 'together', type: 'persona' },
  { uniqueId: 'persona-fireworks', displayName: 'Fireworks AI', provider: 'fireworks', type: 'persona' },
  { uniqueId: 'persona-ollama', displayName: 'Local Assistant', provider: 'ollama', type: 'persona' },
  { uniqueId: 'persona-sentinel', displayName: 'Sentinel', provider: 'sentinel', type: 'persona' },
];
