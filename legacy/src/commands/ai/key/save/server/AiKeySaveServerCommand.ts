/**
 * Ai Key Save Command - Server Implementation
 *
 * Saves API key to ~/.continuum/config.env via SecretManager,
 * sets process.env for immediate use, and emits system:config:key-added
 * event to trigger runtime persona creation.
 */

import { CommandBase, type ICommandDaemon } from '@daemons/command-daemon/shared/CommandBase';
import type { JTAGContext } from '@system/core/types/JTAGTypes';
import { ValidationError } from '@system/core/types/ErrorTypes';
import { Events } from '@system/core/shared/Events';
import type { AiKeySaveParams, AiKeySaveResult } from '../shared/AiKeySaveTypes';
import { createAiKeySaveResultFromParams } from '../shared/AiKeySaveTypes';
import { SecretManager } from '@system/secrets/SecretManager';

export class AiKeySaveServerCommand extends CommandBase<AiKeySaveParams, AiKeySaveResult> {

  constructor(context: JTAGContext, subpath: string, commander: ICommandDaemon) {
    super('ai/key/save', context, subpath, commander);
  }

  async execute(params: AiKeySaveParams): Promise<AiKeySaveResult> {
    if (!params.provider || params.provider.trim() === '') {
      throw new ValidationError(
        'provider',
        `Missing required parameter 'provider'. Expected config key name like 'ANTHROPIC_API_KEY'.`
      );
    }

    if (!params.value || params.value.trim() === '') {
      throw new ValidationError(
        'value',
        `Missing required parameter 'value'. Provide the API key value to save.`
      );
    }

    const secrets = SecretManager.getInstance();

    // Persist to ~/.continuum/config.env
    await secrets.set(params.provider, params.value);

    // Also set in process.env for immediate runtime use
    process.env[params.provider] = params.value;

    // Emit event for PersonaLifecycleManager to create new personas
    await Events.emit('system:config:key-added', {
      provider: params.provider,
      timestamp: Date.now(),
    });

    console.log(`🔑 API key saved: ${params.provider}`);

    return createAiKeySaveResultFromParams(params, {
      success: true,
      saved: true,
      provider: params.provider,
    });
  }
}
