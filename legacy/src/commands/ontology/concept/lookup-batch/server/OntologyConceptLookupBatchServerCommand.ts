/**
 * ontology/concept/lookup-batch — Server implementation
 *
 * Delegates to OntologyRegistry (L1 cache) to:
 *  1. Embed the content using the source model
 *  2. Rank stored concepts by cosine similarity
 *  3. Resolve source→target mappings for top-N concepts
 *  4. Return anchors + glossary string for LLM injection
 */

import { CommandBase, type ICommandDaemon } from '../../../../../daemons/command-daemon/shared/CommandBase';
import type { JTAGContext, JTAGPayload } from '../../../../../system/core/types/JTAGTypes';
import { transformPayload } from '../../../../../system/core/types/JTAGTypes';
import type {
  OntologyConceptLookupBatchParams,
  OntologyConceptLookupBatchResult,
} from '../shared/OntologyConceptLookupBatchTypes';
import { OntologyRegistry } from '../../../../../system/ontology/server/OntologyRegistry';
import { SemanticBridgeService } from '../../../../../system/ontology/shared/SemanticBridgeService';
import type { ModelKey, ConceptAnchor } from '../../../../../system/ontology/shared/OntologyTypes';
import { ONTOLOGY_CONSTANTS } from '../../../../../system/ontology/shared/OntologyTypes';
import { EmbeddingGenerate } from '../../../../ai/embedding/generate/shared/EmbeddingGenerateTypes';

export class OntologyConceptLookupBatchServerCommand extends CommandBase<
  OntologyConceptLookupBatchParams,
  OntologyConceptLookupBatchResult
> {
  constructor(context: JTAGContext, subpath: string, commander: ICommandDaemon) {
    super('ontology/concept/lookup-batch', context, subpath, commander);
  }

  async execute(params: JTAGPayload): Promise<OntologyConceptLookupBatchResult> {
    const {
      content,
      sourceModelKey,
      targetModelKey,
      maxAnchors = ONTOLOGY_CONSTANTS.DEFAULT_MAX_ANCHORS,
      minSimilarity = ONTOLOGY_CONSTANTS.MIN_USABLE_SIMILARITY,
      domains,
    } = params as JTAGPayload & OntologyConceptLookupBatchParams;

    if (!content) {
      return transformPayload(params, {
        success: false,
        error: 'content is required',
        anchors: [],
        anchorCount: 0,
        meanConfidence: 0,
        glossary: '',
        hasDriftWarnings: false,
        driftCount: 0,
      });
    }

    if (!sourceModelKey || !targetModelKey) {
      return transformPayload(params, {
        success: false,
        error: 'sourceModelKey and targetModelKey are required',
        anchors: [],
        anchorCount: 0,
        meanConfidence: 0,
        glossary: '',
        hasDriftWarnings: false,
        driftCount: 0,
      });
    }

    try {
      const registry = OntologyRegistry.sharedInstance();

      // Embed the content using the source model
      const sourceKey = sourceModelKey as ModelKey;
      const targetKey = targetModelKey as ModelKey;

      const contentEmbedding = await this.embedContent(content, sourceKey);

      // Get all concepts, filtered by domain if specified
      const allConcepts = await registry.getAllConcepts();
      const filtered = domains
        ? allConcepts.filter(c => domains.includes(c.domain))
        : allConcepts;

      // Score by cosine similarity against content embedding
      const scored: Array<{ slug: string; score: number }> = [];
      for (const concept of filtered) {
        const conceptVec = concept.embeddings?.[sourceKey];
        if (!conceptVec || conceptVec.length === 0) continue;
        const score = SemanticBridgeService.cosineSimilarity(contentEmbedding, conceptVec);
        scored.push({ slug: concept.slug, score });
      }
      scored.sort((a, b) => b.score - a.score);

      // Resolve mappings for top candidates
      const anchors: ConceptAnchor[] = [];
      let driftCount = 0;

      for (const { slug, score } of scored) {
        if (score < 0.3) break; // not relevant
        if (anchors.length >= maxAnchors) break;

        const mappings = await registry.getMappingsBetween(sourceKey, targetKey);
        const mapping = mappings.find(m => m.conceptSlug === slug);
        if (!mapping) continue;

        if (mapping.similarity < minSimilarity) {
          driftCount++;
          continue; // needs re-alignment
        }

        anchors.push({
          conceptSlug: slug,
          sourceExpression: mapping.sourceExpression,
          targetExpression: mapping.targetExpression,
          similarity: mapping.similarity,
          confidence: mapping.confidence,
        });
      }

      const meanConfidence = anchors.length > 0
        ? anchors.reduce((sum, a) => sum + a.confidence, 0) / anchors.length
        : 0;

      const glossary = anchors
        .map(a => `"${a.sourceExpression}" → "${a.targetExpression}" (${a.conceptSlug})`)
        .join('\n');

      return transformPayload(params, {
        success: true,
        anchors,
        anchorCount: anchors.length,
        meanConfidence: Math.round(meanConfidence * 100) / 100,
        glossary,
        hasDriftWarnings: driftCount > 0,
        driftCount,
      });
    } catch (err) {
      return transformPayload(params, {
        success: false,
        error: err instanceof Error ? err.message : String(err),
        anchors: [],
        anchorCount: 0,
        meanConfidence: 0,
        glossary: '',
        hasDriftWarnings: false,
        driftCount: 0,
      });
    }
  }

  /** Embed text via AIProviderDaemon using the specified model */
  private async embedContent(text: string, modelKey: ModelKey): Promise<number[]> {
    const [providerId, ...modelParts] = modelKey.split('/');
    const result = await EmbeddingGenerate.execute({
      input: text,
      provider: providerId,
      model: modelParts.join('/'),
    });
    return result?.embeddings?.[0] ?? [];
  }
}
