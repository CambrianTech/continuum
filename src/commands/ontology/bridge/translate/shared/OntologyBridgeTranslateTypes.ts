/**
 * ontology/bridge/translate — High-level semantic translation command
 *
 * The primary entry point for programmatic semantic translation.
 * Wraps SemanticBridgeService.translate() as a first-class command so it's
 * accessible from the pipeline system, ./jtag CLI, and Commands.execute().
 *
 * Usage:
 *   const result = await OntologyBridgeTranslate.execute({
 *     content: 'Implement low-rank adaptation using rank-r matrices',
 *     sourceProviderId: 'anthropic',
 *     sourceModelId: 'claude-sonnet-4-6',
 *     targetProviderId: 'candle',
 *     targetModelId: 'qwen2.5-14b',
 *   });
 *   console.log(result.translatedContent);
 */

import type { CommandParams, CommandResult } from '../../../../../system/core/types/JTAGTypes';
import type { ConceptAnchor } from '../../../../../system/ontology/shared/OntologyTypes';
import { Commands } from '@system/core/shared/Commands';
import type { CommandInput } from '@system/core/types/JTAGTypes';

export interface OntologyBridgeTranslateParams extends CommandParams {
  /** The content to translate */
  content: string;

  /** Provider ID for the source model (e.g. "anthropic") */
  sourceProviderId: string;

  /** Model ID for the source (e.g. "claude-sonnet-4-6") */
  sourceModelId: string;

  /** Provider ID for the target model (e.g. "candle") */
  targetProviderId: string;

  /** Model ID for the target (e.g. "qwen2.5-14b") */
  targetModelId: string;

  /** Correlation ID — included in the ontology:translate:complete event */
  requestId?: string;

  /**
   * Maximum concept anchors to inject.
   * More anchors = richer translation but slower.
   * Default: 8
   */
  maxAnchors?: number;

  /**
   * Minimum cosine similarity for an anchor to be used.
   * Below this, the anchor is skipped and flagged for re-alignment.
   * Default: 0.65
   */
  minSimilarity?: number;

  /**
   * Restrict concept search to specific domains.
   * If omitted, all domains are searched.
   */
  domains?: string[];
}

export interface OntologyBridgeTranslateResult extends CommandResult {
  /** The translated content */
  translatedContent: string;

  /** Original content, unmodified */
  originalContent: string;

  /** Concept anchors used in this translation */
  anchorsUsed: ConceptAnchor[];

  /** Number of concepts that required re-alignment before translation */
  realignmentsNeeded: number;

  /** Mean confidence across used anchors (0–1) */
  translationConfidence: number;

  /** True if any anchors were skipped due to drift */
  hasDriftWarnings: boolean;

  /** Request correlation ID */
  requestId: string;
}

export const OntologyBridgeTranslate = {
  execute(params: CommandInput<OntologyBridgeTranslateParams>): Promise<OntologyBridgeTranslateResult> {
    return Commands.execute<OntologyBridgeTranslateParams, OntologyBridgeTranslateResult>(
      'ontology/bridge/translate',
      params as Partial<OntologyBridgeTranslateParams>,
    );
  },
  commandName: 'ontology/bridge/translate' as const,
} as const;
