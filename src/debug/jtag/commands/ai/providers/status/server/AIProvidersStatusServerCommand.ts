/**
 * AI Providers Status Server Command
 *
 * Checks which API keys are configured in ~/.continuum/config.env
 * Returns status only - NEVER exposes actual key values to browser.
 */

import type { ICommandDaemon } from '@daemons/command-daemon/shared/CommandBase';
import type { JTAGContext, JTAGPayload } from '@system/core/types/JTAGTypes';
import { transformPayload } from '@system/core/types/JTAGTypes';
import type { AIProvidersStatusParams, AIProvidersStatusResult, ProviderStatus } from '../shared/AIProvidersStatusTypes';
import { AIProvidersStatusCommand } from '../shared/AIProvidersStatusCommand';
import { SecretManager } from '@system/secrets/SecretManager';

// Provider configuration with helpful links
const PROVIDER_CONFIG: Array<{
  provider: string;
  key: string;
  category: 'local' | 'cloud';
  description: string;
  getKeyUrl?: string;
  billingUrl?: string;
}> = [
  {
    provider: 'Ollama',
    key: 'OLLAMA_HOST',
    category: 'local',
    description: 'Local AI server - free, private, no API key needed',
    getKeyUrl: 'https://ollama.ai/download'
  },
  {
    provider: 'Anthropic',
    key: 'ANTHROPIC_API_KEY',
    category: 'cloud',
    description: 'Claude models - best for complex reasoning',
    getKeyUrl: 'https://console.anthropic.com/settings/keys',
    billingUrl: 'https://console.anthropic.com/settings/billing'
  },
  {
    provider: 'OpenAI',
    key: 'OPENAI_API_KEY',
    category: 'cloud',
    description: 'GPT models - widely compatible',
    getKeyUrl: 'https://platform.openai.com/api-keys',
    billingUrl: 'https://platform.openai.com/account/billing'
  },
  {
    provider: 'Groq',
    key: 'GROQ_API_KEY',
    category: 'cloud',
    description: 'Ultra-fast inference',
    getKeyUrl: 'https://console.groq.com/keys',
    billingUrl: 'https://console.groq.com/settings/billing'
  },
  {
    provider: 'DeepSeek',
    key: 'DEEPSEEK_API_KEY',
    category: 'cloud',
    description: 'Cost-effective reasoning',
    getKeyUrl: 'https://platform.deepseek.com/api_keys',
    billingUrl: 'https://platform.deepseek.com/usage'
  },
  {
    provider: 'xAI',
    key: 'XAI_API_KEY',
    category: 'cloud',
    description: 'Grok models',
    getKeyUrl: 'https://console.x.ai/',
    billingUrl: 'https://console.x.ai/'
  },
  {
    provider: 'Together',
    key: 'TOGETHER_API_KEY',
    category: 'cloud',
    description: 'Open-source model hosting',
    getKeyUrl: 'https://api.together.xyz/settings/api-keys',
    billingUrl: 'https://api.together.xyz/settings/billing'
  },
  {
    provider: 'Fireworks',
    key: 'FIREWORKS_API_KEY',
    category: 'cloud',
    description: 'Fast open-source models',
    getKeyUrl: 'https://fireworks.ai/account/api-keys',
    billingUrl: 'https://fireworks.ai/account/billing'
  },
  {
    provider: 'Alibaba',
    key: 'DASHSCOPE_API_KEY',
    category: 'cloud',
    description: 'Qwen3-Omni - audio-native, open-source',
    getKeyUrl: 'https://dashscope.console.aliyun.com/apiKey',
    billingUrl: 'https://usercenter2.aliyun.com/finance/fund-management/overview'
  },
  {
    provider: 'Google',
    key: 'GOOGLE_API_KEY',
    category: 'cloud',
    description: 'Gemini Live - audio-native, free tier available',
    getKeyUrl: 'https://aistudio.google.com/app/apikey',
    billingUrl: 'https://console.cloud.google.com/billing'
  }
];

export class AIProvidersStatusServerCommand extends AIProvidersStatusCommand {
  constructor(context: JTAGContext, subpath: string, commander: ICommandDaemon) {
    super('ai/providers/status', context, subpath, commander);
  }

  /**
   * Mask a key to show prefix and suffix only, e.g. "sk-...QfQA"
   */
  private maskKey(key: string): string {
    if (!key || key.length < 8) return '***';

    // Find prefix (up to first dash or 4 chars)
    const dashIndex = key.indexOf('-');
    const prefixEnd = dashIndex > 0 && dashIndex < 6 ? dashIndex + 1 : 4;
    const prefix = key.slice(0, prefixEnd);

    // Show last 4 chars
    const suffix = key.slice(-4);

    return `${prefix}...${suffix}`;
  }

  async execute(params: JTAGPayload): Promise<AIProvidersStatusResult> {
    const secrets = SecretManager.getInstance();

    const providers: ProviderStatus[] = PROVIDER_CONFIG.map(config => {
      const isConfigured = secrets.has(config.key);
      const rawKey = isConfigured ? secrets.get(config.key) : undefined;

      return {
        provider: config.provider,
        key: config.key,
        category: config.category,
        description: config.description,
        isConfigured,
        getKeyUrl: config.getKeyUrl,
        billingUrl: config.billingUrl,
        maskedKey: rawKey ? this.maskKey(rawKey) : undefined
      };
    });

    const configuredCount = providers.filter(p => p.isConfigured).length;

    return transformPayload(params as AIProvidersStatusParams, {
      providers,
      configuredCount,
      totalCount: providers.length
    });
  }
}
