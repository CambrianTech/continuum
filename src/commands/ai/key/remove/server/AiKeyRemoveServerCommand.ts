/**
 * Ai Key Remove Command - Server Implementation
 *
 * Removes API key from ~/.continuum/config.env via SecretManager,
 * clears process.env, and emits system:config:key-removed event
 * to deactivate that provider's personas.
 */

import { CommandBase, type ICommandDaemon } from '@daemons/command-daemon/shared/CommandBase';
import type { JTAGContext } from '@system/core/types/JTAGTypes';
import { ValidationError } from '@system/core/types/ErrorTypes';
import { Events } from '@system/core/shared/Events';
import type { AiKeyRemoveParams, AiKeyRemoveResult } from '../shared/AiKeyRemoveTypes';
import { createAiKeyRemoveResultFromParams } from '../shared/AiKeyRemoveTypes';
import { SecretManager } from '@system/secrets/SecretManager';

export class AiKeyRemoveServerCommand extends CommandBase<AiKeyRemoveParams, AiKeyRemoveResult> {

  constructor(context: JTAGContext, subpath: string, commander: ICommandDaemon) {
    super('ai/key/remove', context, subpath, commander);
  }

  async execute(params: AiKeyRemoveParams): Promise<AiKeyRemoveResult> {
    if (!params.provider || params.provider.trim() === '') {
      throw new ValidationError(
        'provider',
        `Missing required parameter 'provider'. Expected config key name like 'ANTHROPIC_API_KEY'.`
      );
    }

    const secrets = SecretManager.getInstance();

    // Remove from ~/.continuum/config.env
    await secrets.remove(params.provider);

    // Clear from process.env
    delete process.env[params.provider];

    // Emit event for PersonaLifecycleManager to deactivate personas
    await Events.emit('system:config:key-removed', {
      provider: params.provider,
      timestamp: Date.now(),
    });

    console.log(`🔑 API key removed: ${params.provider}`);

    return createAiKeyRemoveResultFromParams(params, {
      success: true,
      removed: true,
      provider: params.provider,
    });
  }
}
