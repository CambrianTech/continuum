/**
 * ontology/concept/lookup-batch — Find concept anchors relevant to a piece of content
 *
 * Used by the SemanticTranslatorPipeline as Step 0: embed the source content,
 * compute cosine similarity against all stored concept embeddings, and return
 * the top-N concept anchors with their source→target expression mappings.
 *
 * This command is the "vocabulary lookup" that powers the translation glossary.
 */

import type { CommandParams, CommandResult } from '../../../../../system/core/types/JTAGTypes';
import type { ConceptAnchor } from '../../../../../system/ontology/shared/OntologyTypes';
import { Commands } from '@system/core/shared/Commands';
import type { CommandInput } from '@system/core/types/JTAGTypes';

export interface OntologyConceptLookupBatchParams extends CommandParams {
  /** Content whose relevant concepts we want to find */
  content: string;

  /** Source model key (e.g. "anthropic/claude-sonnet-4-6") */
  sourceModelKey: string;

  /** Target model key (e.g. "candle/qwen2.5-14b") */
  targetModelKey: string;

  /** Maximum number of anchors to return (default: 8) */
  maxAnchors?: number;

  /**
   * Minimum cosine similarity for a mapping to be included.
   * Mappings below this score are omitted and flagged for re-alignment.
   * Default: 0.65
   */
  minSimilarity?: number;

  /**
   * Restrict search to concepts in these domains.
   * If omitted, all domains are searched.
   */
  domains?: string[];
}

export interface OntologyConceptLookupBatchResult extends CommandResult {
  /** Resolved concept anchors with source and target expressions */
  anchors: ConceptAnchor[];

  /** Number of anchors returned */
  anchorCount: number;

  /** Mean confidence score across returned anchors */
  meanConfidence: number;

  /**
   * Formatted glossary string for injection into LLM prompts.
   * Format: one line per anchor: `"<source>" → "<target>" (<slug>)`
   */
  glossary: string;

  /**
   * True if any concepts had similarity below minSimilarity
   * and were excluded (pending re-alignment).
   */
  hasDriftWarnings: boolean;

  /** Number of concepts that were too low-similarity to use */
  driftCount: number;
}

export const OntologyConceptLookupBatch = {
  execute(params: CommandInput<OntologyConceptLookupBatchParams>): Promise<OntologyConceptLookupBatchResult> {
    return Commands.execute<OntologyConceptLookupBatchParams, OntologyConceptLookupBatchResult>(
      'ontology/concept/lookup-batch',
      params as Partial<OntologyConceptLookupBatchParams>,
    );
  },
  commandName: 'ontology/concept/lookup-batch' as const,
} as const;
