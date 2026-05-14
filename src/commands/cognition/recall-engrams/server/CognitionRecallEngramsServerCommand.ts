/**
 * cognition/recall-engrams — Server Implementation
 *
 * Pure pass-through to the Rust `cognition/recall-engrams` IPC handler
 * shipped in #1121 PR-5. Wire format: { personaId, kind?, limit?,
 * id?, keyword?, origin? } → { engrams, count }. All recall logic
 * (recent / by_id / by_keyword / by_origin enumeration) lives in
 * Rust (`workers/continuum-core/src/modules/cognition.rs`).
 *
 * Per CLAUDE.md "Rust-Backed Commands (IPC Mixin Pattern)" + Joel's
 * "if not UI/UX it is rust" rule: this TS file exists ONLY so the
 * recipe pipeline + ./jtag CLI can route through `Commands.execute`.
 * It is a thin bridge. No business logic. No reimplementation.
 */

import { CommandBase, type ICommandDaemon } from '@daemons/command-daemon/shared/CommandBase';
import type { JTAGContext } from '@system/core/types/JTAGTypes';
import { ValidationError } from '@system/core/types/ErrorTypes';
import type {
  CognitionRecallEngramsParams,
  CognitionRecallEngramsResult,
} from '../shared/CognitionRecallEngramsTypes';
import { createCognitionRecallEngramsResultFromParams } from '../shared/CognitionRecallEngramsTypes';
import { RustCoreIPCClient } from '../../../../workers/continuum-core/bindings/RustCoreIPC';

export class CognitionRecallEngramsServerCommand extends CommandBase<
  CognitionRecallEngramsParams,
  CognitionRecallEngramsResult
> {
  constructor(context: JTAGContext, subpath: string, commander: ICommandDaemon) {
    super('cognition/recall-engrams', context, subpath, commander);
  }

  /**
   * Per-kind required-companion-field check. Returns the field name +
   * message if a required companion is missing, else null.
   */
  private validateKindCompanion(
    params: CognitionRecallEngramsParams,
  ): { field: string; message: string } | null {
    const kind = params.kind ?? 'recent';
    if (kind === 'by_id' && (params.id === undefined || params.id.trim() === '')) {
      return { field: 'id', message: `kind='by_id' requires an 'id' parameter (the engram UUID to look up).` };
    }
    if (kind === 'by_keyword' && (params.keyword === undefined || params.keyword.trim() === '')) {
      return { field: 'keyword', message: `kind='by_keyword' requires a 'keyword' parameter (substring to match).` };
    }
    if (kind === 'by_origin' && params.origin === undefined) {
      return { field: 'origin', message: `kind='by_origin' requires an 'origin' parameter (chat | airc | tool | self_reflection).` };
    }
    return null;
  }

  async execute(
    params: CognitionRecallEngramsParams,
  ): Promise<CognitionRecallEngramsResult> {
    if (params.personaId === undefined || params.personaId.trim() === '') {
      throw new ValidationError(
        'personaId',
        `Missing required parameter 'personaId'. Provide the UUID of the persona whose engram store to query. See the cognition/recall-engrams README for usage.`,
      );
    }

    const companionMiss = this.validateKindCompanion(params);
    if (companionMiss !== null) {
      throw new ValidationError(companionMiss.field, companionMiss.message);
    }

    const client = await RustCoreIPCClient.getInstanceAsync();
    const { engrams, count } = await client.cognitionRecallEngrams({
      personaId: params.personaId,
      kind: params.kind ?? 'recent',
      limit: params.limit,
      id: params.id,
      keyword: params.keyword,
      origin: params.origin,
    });

    return createCognitionRecallEngramsResultFromParams(params, {
      success: true,
      engrams: engrams as unknown as Array<Record<string, unknown>>,
      count,
    });
  }
}
