/**
 * Ai Key Status Command - Server Implementation
 *
 * Report redacted API-key availability and fingerprints without exposing raw or masked secret values.
 */

import { CommandBase, type ICommandDaemon } from '@daemons/command-daemon/shared/CommandBase';
import type { JTAGContext } from '@system/core/types/JTAGTypes';
import { ValidationError } from '@system/core/types/ErrorTypes';
import { SecretManager } from '@system/secrets/SecretManager';
import type { AiKeyStatusParams, AiKeyStatusResult } from '../shared/AiKeyStatusTypes';
import { createAiKeyStatusResultFromParams } from '../shared/AiKeyStatusTypes';
import { createAiKeyStatusEntry } from '../shared/AiKeyStatusRedaction';
import { AI_KEY_PROVIDERS, findAiKeyProvider, type AiKeyProviderMetadata } from '../../common/AiKeyProviders';

export class AiKeyStatusServerCommand extends CommandBase<AiKeyStatusParams, AiKeyStatusResult> {

  constructor(context: JTAGContext, subpath: string, commander: ICommandDaemon) {
    super('ai/key/status', context, subpath, commander);
  }

  async execute(params: AiKeyStatusParams): Promise<AiKeyStatusResult> {
    const secrets = SecretManager.getInstance();
    const requestedProvider = params.provider?.trim();

    const providers: AiKeyProviderMetadata[] = requestedProvider
      ? [findAiKeyProvider(requestedProvider)].filter((provider): provider is AiKeyProviderMetadata => provider !== undefined)
      : [...AI_KEY_PROVIDERS];

    if (requestedProvider && providers.length === 0) {
      throw new ValidationError(
        'provider',
        `Unknown API key provider '${requestedProvider}'. Use a provider name or config key like OPENAI_API_KEY.`
      );
    }

    const entries = providers.map(provider => {
      const value = provider.category === 'local'
        ? process.env[provider.key]
        : secrets.get(provider.key, 'AiKeyStatusServerCommand');

      return createAiKeyStatusEntry({
        provider: provider.provider,
        key: provider.key,
        category: provider.category,
        description: provider.description,
        value,
        processValue: process.env[provider.key]
      });
    });

    return createAiKeyStatusResultFromParams(params, {
      success: true,
      provider: requestedProvider,
      entries,
      configuredCount: entries.filter(entry => entry.configured).length,
      totalCount: entries.length,
    });
  }
}
