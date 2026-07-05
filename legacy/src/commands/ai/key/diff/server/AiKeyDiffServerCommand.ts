/**
 * Ai Key Diff Command - Server Implementation
 *
 * Compare redacted AI key status entries and produce a value-free merge plan for trusted grid reconciliation.
 */

import { CommandBase, type ICommandDaemon } from '@daemons/command-daemon/shared/CommandBase';
import type { JTAGContext } from '@system/core/types/JTAGTypes';
import { ValidationError } from '@system/core/types/ErrorTypes';
import type { AiKeyDiffParams, AiKeyDiffResult } from '../shared/AiKeyDiffTypes';
import { createAiKeyDiffResultFromParams } from '../shared/AiKeyDiffTypes';
import { buildAiKeyDiffActions, createAiKeyMergePlanId } from '../shared/AiKeyDiffPlanner';

export class AiKeyDiffServerCommand extends CommandBase<AiKeyDiffParams, AiKeyDiffResult> {

  constructor(context: JTAGContext, subpath: string, commander: ICommandDaemon) {
    super('ai/key/diff', context, subpath, commander);
  }

  async execute(params: AiKeyDiffParams): Promise<AiKeyDiffResult> {
    await Promise.resolve();

    if (!Array.isArray(params.localEntries)) {
      throw new ValidationError(
        'localEntries',
        `Missing required array parameter 'localEntries'. Use ai/key/status output for the local node.`
      );
    }

    if (!Array.isArray(params.remoteEntries)) {
      throw new ValidationError(
        'remoteEntries',
        `Missing required array parameter 'remoteEntries'. Use ai/key/status output from a trusted remote node.`
      );
    }

    const actions = buildAiKeyDiffActions(params.localEntries, params.remoteEntries, params.targetNode);

    return createAiKeyDiffResultFromParams(params, {
      success: true,
      mergePlanId: createAiKeyMergePlanId(actions, params.targetNode),
      actions,
      conflictCount: actions.filter(action => action.action === 'conflict').length,
      actionCount: actions.length,
    });
  }
}
