/**
 * Ai Key Test Command - Server Implementation
 *
 * Test an API key before saving it. Makes a minimal API call to verify the key is valid.
 * The key is used for the test only and is NOT stored.
 */

import { CommandBase, type ICommandDaemon } from '@daemons/command-daemon/shared/CommandBase';
import type { JTAGContext } from '@system/core/types/JTAGTypes';
import { ValidationError } from '@system/core/types/ErrorTypes';
import type { AiKeyTestParams, AiKeyTestResult } from '../shared/AiKeyTestTypes';
import { createAiKeyTestResultFromParams } from '../shared/AiKeyTestTypes';
import { SecretManager } from '@system/secrets/SecretManager';

// Supported providers and their API endpoints
const PROVIDER_ENDPOINTS: Record<string, {
  testEndpoint: string;
  headerName: string;
  headerPrefix: string;
}> = {
  anthropic: {
    testEndpoint: 'https://api.anthropic.com/v1/messages',
    headerName: 'x-api-key',
    headerPrefix: ''
  },
  openai: {
    testEndpoint: 'https://api.openai.com/v1/models',
    headerName: 'Authorization',
    headerPrefix: 'Bearer '
  },
  groq: {
    testEndpoint: 'https://api.groq.com/openai/v1/models',
    headerName: 'Authorization',
    headerPrefix: 'Bearer '
  },
  deepseek: {
    testEndpoint: 'https://api.deepseek.com/v1/models',
    headerName: 'Authorization',
    headerPrefix: 'Bearer '
  },
  xai: {
    testEndpoint: 'https://api.x.ai/v1/models',
    headerName: 'Authorization',
    headerPrefix: 'Bearer '
  },
  together: {
    testEndpoint: 'https://api.together.xyz/v1/models',
    headerName: 'Authorization',
    headerPrefix: 'Bearer '
  },
  fireworks: {
    testEndpoint: 'https://api.fireworks.ai/inference/v1/models',
    headerName: 'Authorization',
    headerPrefix: 'Bearer '
  }
};

export class AiKeyTestServerCommand extends CommandBase<AiKeyTestParams, AiKeyTestResult> {

  constructor(context: JTAGContext, subpath: string, commander: ICommandDaemon) {
    super('ai/key/test', context, subpath, commander);
  }

  // Map provider names to their environment variable keys
  private static readonly PROVIDER_ENV_KEYS: Record<string, string> = {
    anthropic: 'ANTHROPIC_API_KEY',
    openai: 'OPENAI_API_KEY',
    groq: 'GROQ_API_KEY',
    deepseek: 'DEEPSEEK_API_KEY',
    xai: 'XAI_API_KEY',
    together: 'TOGETHER_API_KEY',
    fireworks: 'FIREWORKS_API_KEY'
  };

  async execute(params: AiKeyTestParams): Promise<AiKeyTestResult> {
    const startTime = Date.now();
    const provider = params.provider?.toLowerCase();

    // Validate provider
    if (!provider || !PROVIDER_ENDPOINTS[provider]) {
      throw new ValidationError(
        'provider',
        `Invalid provider '${params.provider}'. Supported: ${Object.keys(PROVIDER_ENDPOINTS).join(', ')}`
      );
    }

    // Get the key - either from params or from stored secrets
    let key = params.key;
    if (params.useStored) {
      const envKey = AiKeyTestServerCommand.PROVIDER_ENV_KEYS[provider];
      if (envKey) {
        const secrets = SecretManager.getInstance();
        key = secrets.get(envKey) || '';
      }
    }

    // Validate key
    if (!key || key.trim() === '') {
      throw new ValidationError('key', 'API key is required');
    }

    const config = PROVIDER_ENDPOINTS[provider];

    // Make test API call - let errors propagate naturally
    const headers: Record<string, string> = {
      [config.headerName]: config.headerPrefix + key,
      'Content-Type': 'application/json'
    };

    // Anthropic requires additional headers
    if (provider === 'anthropic') {
      headers['anthropic-version'] = '2023-06-01';
    }

    let response: Response;
    let models: string[] | undefined;

    if (provider === 'anthropic') {
      // Anthropic doesn't have a /models endpoint - use a minimal message request
      response = await fetch(config.testEndpoint, {
        method: 'POST',
        headers,
        body: JSON.stringify({
          model: 'claude-3-haiku-20240307',
          max_tokens: 1,
          messages: [{ role: 'user', content: 'test' }]
        })
      });
    } else {
      // Other providers have /models endpoint
      response = await fetch(config.testEndpoint, {
        method: 'GET',
        headers
      });

      // Extract models list if available
      if (response.ok) {
        const data = await response.json();
        if (data.data && Array.isArray(data.data)) {
          models = data.data.slice(0, 10).map((m: { id: string }) => m.id);
        }
      }
    }

    const responseTime = Date.now() - startTime;

    if (response.ok) {
      return createAiKeyTestResultFromParams(params, {
        success: true,
        valid: true,
        provider: params.provider,
        responseTime,
        models
      });
    }

    // API returned error - key is invalid but command succeeded
    return createAiKeyTestResultFromParams(params, {
      success: true,
      valid: false,
      provider: params.provider,
      responseTime,
      errorMessage: `${response.status} ${response.statusText}`
    });
  }
}
