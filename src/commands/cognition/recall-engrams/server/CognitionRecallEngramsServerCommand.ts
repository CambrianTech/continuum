/**
 * cognition/recall-engrams — Server Implementation
 *
 * Pure pass-through to the Rust `cognition/recall-engrams` IPC handler
 * shipped in #1121 PR-5. Wire format: { personaId, kind?, limit?,
 * id?, keyword?, origin? } → { engrams, count }. All recall logic
 * (recent / by_id / by_keyword / by_origin enumeration) lives in
 * Rust (`../core/continuum-core/src/modules/cognition.rs`).
 *
 * Per CLAUDE.md "Rust-Backed Commands (IPC Mixin Pattern)" + Joel's
 * "if not UI/UX it is rust" rule: this TS file exists ONLY so the
 * recipe pipeline + ./jtag CLI can route through `Commands.execute`.
 * It is a thin bridge. No business logic. No reimplementation.
 *
 * **Refactored to RustBackedCommand (#1198 follow-on to #1256):** the
 * standard validate + call mixin + wrap-result envelope is now in the
 * base class. Only the variable bits — required-param list, kind-
 * companion validation, mixin call, result mapping — remain here.
 */

import type { ICommandDaemon } from '@daemons/command-daemon/shared/CommandBase';
import { RustBackedCommand } from '@daemons/command-daemon/shared/RustBackedCommand';
import type { JTAGContext } from '@system/core/types/JTAGTypes';
import { ValidationError } from '@system/core/types/ErrorTypes';
import type {
  CognitionRecallEngramsParams,
  CognitionRecallEngramsResult,
} from '../shared/CognitionRecallEngramsTypes';
import { createCognitionRecallEngramsResultFromParams } from '../shared/CognitionRecallEngramsTypes';
import type { RustCoreIPCClient } from '../../../../../core/continuum-core/bindings/RustCoreIPC';

/** Snake-case shape returned by the Rust mixin — matches the IPC payload. */
type RecallEngramsRustResponse = {
  engrams: unknown;
  count: number;
};

export class CognitionRecallEngramsServerCommand extends RustBackedCommand<
  CognitionRecallEngramsParams,
  CognitionRecallEngramsResult,
  RecallEngramsRustResponse
> {
  protected override readonly requiredParams = ['personaId'] as const;

  constructor(context: JTAGContext, subpath: string, commander: ICommandDaemon) {
    super('cognition/recall-engrams', context, subpath, commander);
  }

  /**
   * Subclass override: in addition to the base required-param check
   * (personaId non-empty), the recall command's `kind` discriminator
   * has per-variant required-companion fields. by_id needs `id`,
   * by_keyword needs `keyword`, by_origin needs `origin`. Recent (the
   * default) needs nothing extra.
   */
  protected override validateParams(params: CognitionRecallEngramsParams): void {
    super.validateParams(params);
    const kind = params.kind ?? 'recent';
    if (kind === 'by_id' && (params.id === undefined || params.id.trim() === '')) {
      throw new ValidationError(
        'id',
        `kind='by_id' requires an 'id' parameter (the engram UUID to look up).`,
      );
    }
    if (kind === 'by_keyword' && (params.keyword === undefined || params.keyword.trim() === '')) {
      throw new ValidationError(
        'keyword',
        `kind='by_keyword' requires a 'keyword' parameter (substring to match).`,
      );
    }
    if (kind === 'by_origin' && params.origin === undefined) {
      throw new ValidationError(
        'origin',
        `kind='by_origin' requires an 'origin' parameter (chat | airc | tool | self_reflection).`,
      );
    }
  }

  protected override async callRust(
    params: CognitionRecallEngramsParams,
    client: RustCoreIPCClient,
  ): Promise<RecallEngramsRustResponse> {
    return client.cognitionRecallEngrams({
      personaId: params.personaId,
      kind: params.kind ?? 'recent',
      limit: params.limit,
      id: params.id,
      keyword: params.keyword,
      origin: params.origin,
    });
  }

  protected override toResult(
    raw: RecallEngramsRustResponse,
    params: CognitionRecallEngramsParams,
  ): CognitionRecallEngramsResult {
    return createCognitionRecallEngramsResultFromParams(params, {
      success: true,
      engrams: raw.engrams as Array<Record<string, unknown>>,
      count: raw.count,
    });
  }
}
