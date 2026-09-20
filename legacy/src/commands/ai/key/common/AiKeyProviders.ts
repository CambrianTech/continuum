/**
 * Known AI provider key metadata shared by ai/key/* commands.
 *
 * Keep this list about secret/config keys only. Transport routing and grid
 * synchronization stay command execution context, not provider taxonomy.
 */

export type AiKeyCategory = 'local' | 'cloud';

export interface AiKeyProviderMetadata {
  provider: string;
  key: string;
  category: AiKeyCategory;
  description: string;
}

export const AI_KEY_PROVIDERS: readonly AiKeyProviderMetadata[] = [
  {
    provider: 'Docker Model Runner',
    key: 'DMR_ENABLED',
    category: 'local',
    description: 'Local LLM inference via Docker Desktop Model Runner'
  },
  {
    provider: 'Anthropic',
    key: 'ANTHROPIC_API_KEY',
    category: 'cloud',
    description: 'Claude models'
  },
  {
    provider: 'OpenAI',
    key: 'OPENAI_API_KEY',
    category: 'cloud',
    description: 'GPT models'
  },
  {
    provider: 'Groq',
    key: 'GROQ_API_KEY',
    category: 'cloud',
    description: 'Fast inference'
  },
  {
    provider: 'DeepSeek',
    key: 'DEEPSEEK_API_KEY',
    category: 'cloud',
    description: 'Reasoning models'
  },
  {
    provider: 'xAI',
    key: 'XAI_API_KEY',
    category: 'cloud',
    description: 'Grok models'
  },
  {
    provider: 'Together',
    key: 'TOGETHER_API_KEY',
    category: 'cloud',
    description: 'Open model hosting'
  },
  {
    provider: 'Fireworks',
    key: 'FIREWORKS_API_KEY',
    category: 'cloud',
    description: 'Open model hosting'
  },
  {
    provider: 'Alibaba',
    key: 'DASHSCOPE_API_KEY',
    category: 'cloud',
    description: 'Qwen/DashScope models'
  },
  {
    provider: 'Google',
    key: 'GOOGLE_API_KEY',
    category: 'cloud',
    description: 'Gemini models'
  },
  {
    provider: 'Hugging Face',
    key: 'HF_TOKEN',
    category: 'cloud',
    description: 'Model upload/factory access. Public downloads must not require this.'
  }
] as const;

export function normalizeAiKeyProvider(input: string): string {
  return input.trim().toLowerCase().replace(/[\s_-]+/g, '');
}

export function findAiKeyProvider(input: string): AiKeyProviderMetadata | undefined {
  const normalized = normalizeAiKeyProvider(input);
  return AI_KEY_PROVIDERS.find(provider =>
    normalizeAiKeyProvider(provider.provider) === normalized ||
    normalizeAiKeyProvider(provider.key) === normalized
  );
}
