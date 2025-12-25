/**
 * AI Providers Status Server Command
 *
 * Checks which API keys are configured in ~/.continuum/config.env
 * Returns status only - NEVER exposes actual key values to browser.
 */

import { CommandBase, type ICommandDaemon } from '../../../../../daemons/command-daemon/shared/CommandBase';
import type { JTAGContext, JTAGPayload } from '../../../../../system/core/types/JTAGTypes';
import { transformPayload } from '../../../../../system/core/types/JTAGTypes';
import type { AIProvidersStatusParams, AIProvidersStatusResult, ProviderStatus } from '../shared/AIProvidersStatusTypes';
import { SecretManager } from '../../../../../system/secrets/SecretManager';

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
  }
];

export class AIProvidersStatusServerCommand extends CommandBase<
  AIProvidersStatusParams,
  AIProvidersStatusResult
> {
  constructor(context: JTAGContext, subpath: string, commander: ICommandDaemon) {
    super('ai/providers/status', context, subpath, commander);
  }

  async execute(params: JTAGPayload): Promise<AIProvidersStatusResult> {
    const secrets = SecretManager.getInstance();

    const providers: ProviderStatus[] = PROVIDER_CONFIG.map(config => ({
      provider: config.provider,
      key: config.key,
      category: config.category,
      description: config.description,
      isConfigured: secrets.has(config.key),
      getKeyUrl: config.getKeyUrl,
      billingUrl: config.billingUrl
    }));

    const configuredCount = providers.filter(p => p.isConfigured).length;

    return transformPayload(params as AIProvidersStatusParams, {
      providers,
      configuredCount,
      totalCount: providers.length
    });
  }
}
