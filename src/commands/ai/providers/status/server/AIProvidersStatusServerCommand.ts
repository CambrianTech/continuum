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
    // Local inference goes through Docker Model Runner via Rust IPC
    // (AIProviderDaemon.generateText → ai/generate). The previous entry
    // was "Candle" with a similar description, but Candle is a training
    // framework (LoRA, autodiff, fine-tuning), NOT inference — Joel's
    // correction in #980 Bug 6. Training callers access Candle through
    // the training/plasticity module directly; it doesn't belong in the
    // user-facing inference-providers list. AIProviderDaemonServer.ts
    // line 146-150 confirms: Candle is NOT registered in the inference
    // adapter registry.
    provider: 'Docker Model Runner',
    key: 'DMR_ENABLED',
    category: 'local',
    description: 'Local LLM inference via Docker Desktop Model Runner (Metal on Apple Silicon, CUDA on Nvidia, Vulkan on AMD/Intel)',
    getKeyUrl: 'https://docs.docker.com/desktop/features/model-runner/'
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
      // Candle is always available — it's local inference, no API key needed
      //
      // For non-local providers: SecretManager.has(key) returns true when the
      // key NAME is present in config.env even if its VALUE is empty (the
      // shipped fresh config has ANTHROPIC_API_KEY=, OPENAI_API_KEY=,
      // DEEPSEEK_API_KEY= as empty placeholders). So has(key) gave false-
      // positive isConfigured=true for every fresh install, leading users to
      // attempt chat and hit an opaque 401. Check the actual value length
      // instead. (#980 Bug 5.)
      const rawKey = config.category === 'local' ? undefined : secrets.get(config.key);
      const isConfigured = config.category === 'local' ? true : (rawKey?.length ?? 0) > 0;

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
