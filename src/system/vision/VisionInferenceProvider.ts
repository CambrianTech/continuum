/**
 * VisionInferenceProvider — thin shim.
 *
 * Pre-#1276 this file was 176 LOC owning vision-model selection,
 * prompt construction, multimodal `AIProviderDaemon.generateText`
 * dispatch, and response parsing. Per Joel 2026-05-15 ("if not UI/UX
 * it is rust") and the #1248 oxidizer umbrella, all four steps moved
 * to Rust at `../core/continuum-core/src/cognition/vision_describe.rs`
 * and are exposed via the `cognition/vision-describe` IPC.
 *
 * This file now exists ONLY as a thin TS-side shape preserver so
 * `VisionDescriptionService` can keep its constructor / cache /
 * dedup contract unchanged. Every method is a single
 * `Commands.execute('cognition/vision-describe', ...)` call.
 *
 * Outlier-validation pair with codex's #1284 (AIDecisionService
 * structured-decision shape).
 */

import { CognitionVisionDescribe } from '@commands/cognition/vision-describe/shared/CognitionVisionDescribeTypes';
import type { VisionDescription, DescribeOptions } from './VisionDescriptionService';

export class VisionInferenceProvider {
  /**
   * Best-effort "vision available?" — kept for VisionDescriptionService's
   * synchronous fast-fail call sites. Post-#1276 the real signal is
   * `describe()` returning null. See VisionDescriptionService.isAvailable()
   * docstring for the migration plan.
   */
  isAvailable(): boolean {
    return true;
  }

  /**
   * Describe an image via multimodal inference.
   *
   * Thin pass-through to `cognition/vision-describe`. The Rust side
   * owns model selection, prompt construction, the `ai/generate`
   * dispatch, and response parsing.
   */
  async describe(
    base64Data: string,
    mimeType: string,
    options: DescribeOptions = {},
  ): Promise<VisionDescription | null> {
    const result = await CognitionVisionDescribe.execute({
      base64Data,
      mimeType,
      options: {
        preferredModel: options.preferredModel,
        preferredProvider: options.preferredProvider,
        maxLength: options.maxLength,
        prompt: options.prompt,
        detectObjects: options.detectObjects ?? false,
        detectColors: options.detectColors ?? false,
        detectText: options.detectText ?? false,
      },
    });

    if (!result.success || result.result === null) return null;

    // Rust returns the same `VisionDescription` shape that this file
    // historically constructed (description / modelId / provider /
    // timestamp / objects / colors / text / responseTimeMs).
    return result.result as VisionDescription;
  }
}
