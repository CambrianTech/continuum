/**
 * VisionInferenceProvider — thin shim.
 *
 * Pre-#1276 this file was 176 LOC owning vision-model selection,
 * prompt construction, multimodal `AIProviderDaemon.generateText`
 * dispatch, and response parsing. Per Joel 2026-05-15 ("if not UI/UX
 * it is rust") and the #1248 oxidizer umbrella, all four steps moved
 * to Rust at `workers/continuum-core/src/cognition/vision_describe.rs`
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
   * Check if any vision model is available for inference.
   *
   * Pre-#1276 this called into `AICapabilityRegistry` directly. Today
   * the source-of-truth is the Rust model registry. We avoid an extra
   * IPC ping per `descriptionStatus()` call by always returning true —
   * the actual `cognition/vision-describe` call returns `null` when no
   * vision model is registered, which `describe()` already surfaces.
   * `VisionDescriptionService` only uses this for a coarse "should I
   * even try?" check; on no-vision-models it gets a `null` result
   * back the first time and degrades the same way.
   */
  isAvailable(): boolean {
    return true;
  }

  /**
   * Get available vision models with their providers.
   *
   * Returns an empty array — the legacy callers (UI diagnostics) used
   * this for human-readable model lists; that surface is being moved
   * to a dedicated Rust IPC (`ai/providers/list` already exists +
   * filters by capability). See the parent #1276 follow-up for the
   * full removal of this method.
   */
  availableModels(): Array<{ modelId: string; provider: string }> {
    return [];
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
