/**
 * cognition/vision-describe — Server Implementation
 *
 * Pure pass-through to the Rust `cognition/vision-describe` IPC handler
 * shipped in #1276. Wire format: { base64Data, mimeType, options? } →
 * { result: VisionDescription | null }. All vision-model selection,
 * prompt construction, multimodal `ai/generate` dispatch, and response
 * parsing live in Rust (`workers/continuum-core/src/cognition/vision_describe.rs`).
 *
 * Per CLAUDE.md "Rust-Backed Commands (IPC Mixin Pattern)" + Joel's
 * "if not UI/UX it is rust" rule: this TS file exists ONLY so the
 * recipe pipeline + ./jtag CLI can route through `Commands.execute`.
 * It is a thin bridge. No business logic. No reimplementation.
 *
 * Pre-#1276 the equivalent logic lived in
 * `system/vision/VisionInferenceProvider.ts` (176 LOC). Outlier-validation
 * pair with codex's #1284 (AIDecisionService.evaluateGating →
 * cognition/should-respond, structured-decision shape); this card is
 * the freeform-shape outlier.
 */

import type { ICommandDaemon } from '@daemons/command-daemon/shared/CommandBase';
import { RustBackedCommand } from '@daemons/command-daemon/shared/RustBackedCommand';
import type { JTAGContext } from '@system/core/types/JTAGTypes';
import type { VisionDescription } from '@shared/generated/cognition';
import type {
  CognitionVisionDescribeParams,
  CognitionVisionDescribeResult,
} from '../shared/CognitionVisionDescribeTypes';
import { createCognitionVisionDescribeResultFromParams } from '../shared/CognitionVisionDescribeTypes';
import type { RustCoreIPCClient } from '../../../../workers/continuum-core/bindings/RustCoreIPC';

/** Snake-case shape returned by the Rust mixin — matches the IPC payload. */
type VisionDescribeRustResponse = VisionDescription | null;

export class CognitionVisionDescribeServerCommand extends RustBackedCommand<
  CognitionVisionDescribeParams,
  CognitionVisionDescribeResult,
  VisionDescribeRustResponse
> {
  protected override readonly requiredParams = ['base64Data', 'mimeType'] as const;

  constructor(context: JTAGContext, subpath: string, commander: ICommandDaemon) {
    super('cognition/vision-describe', context, subpath, commander);
  }

  protected override async callRust(
    params: CognitionVisionDescribeParams,
    client: RustCoreIPCClient,
  ): Promise<VisionDescribeRustResponse> {
    return client.cognitionVisionDescribe({
      base64Data: params.base64Data,
      mimeType: params.mimeType,
      options: params.options ?? {
        detectObjects: false,
        detectColors: false,
        detectText: false,
      },
    });
  }

  protected override toResult(
    raw: VisionDescribeRustResponse,
    params: CognitionVisionDescribeParams,
  ): CognitionVisionDescribeResult {
    return createCognitionVisionDescribeResultFromParams(params, {
      success: raw !== null,
      result: raw,
    });
  }
}
