/**
 * Persona Learning Pattern Query Command - Server Implementation
 *
 * Query the collective pattern knowledge base. Search for patterns that might help solve the current problem.
 */

import { CommandBase, type ICommandDaemon } from '@daemons/command-daemon/shared/CommandBase';
import type { JTAGContext } from '@system/core/types/JTAGTypes';
import type { PersonaLearningPatternQueryParams, PersonaLearningPatternQueryResult, PatternSummary } from '../shared/PersonaLearningPatternQueryTypes';
import { createPersonaLearningPatternQueryResultFromParams } from '../shared/PersonaLearningPatternQueryTypes';
import { FeedbackEntity, FeedbackStatus } from '@system/data/entities/FeedbackEntity';
import { Commands } from '@system/core/shared/Commands';
import type { DataListParams, DataListResult } from '@commands/data/list/shared/DataListTypes';

import { DataList } from '../../../../../data/list/shared/DataListTypes';
export class PersonaLearningPatternQueryServerCommand extends CommandBase<PersonaLearningPatternQueryParams, PersonaLearningPatternQueryResult> {

  constructor(context: JTAGContext, subpath: string, commander: ICommandDaemon) {
    super('persona/learning/pattern/query', context, subpath, commander);
  }

  async execute(params: PersonaLearningPatternQueryParams): Promise<PersonaLearningPatternQueryResult> {
    console.log('🔍 PATTERN QUERY: Searching collective knowledge base');

    // Build filter for database query
    // Note: Using simple equality filters compatible with SQLite
    const filter: Record<string, unknown> = {};

    // Filter by status if specified, otherwise show public patterns
    if (params.status) {
      filter['status'] = params.status;
    } else {
      // Default: show public patterns (simpler than $or which SQLite doesn't support)
      filter['isPublic'] = true;
    }

    // Filter by domain
    if (params.domain) {
      filter['domain'] = params.domain;
    }

    // Filter by type
    if (params.type) {
      filter['type'] = params.type;
    }

    // Note: minConfidence filtering done in post-processing since SQLite
    // doesn't support $gte operator in our data layer

    // Determine sort order
    const orderByField = params.orderBy || 'confidence';
    const orderBy = [{ field: orderByField, direction: 'desc' as const }];

    // Execute database query
    const listResult = await DataList.execute<FeedbackEntity>({
      collection: FeedbackEntity.collection,
      filter,
      orderBy,
      limit: params.limit || 10,
      context: params.context,
      sessionId: params.sessionId
    });

    if (!listResult.success) {
      throw new Error(`Failed to query patterns: ${listResult.error || 'Unknown error'}`);
    }

    let patterns: FeedbackEntity[] = [...listResult.items];

    // Post-filter: text search across multiple fields
    if (params.search) {
      const searchLower = params.search.toLowerCase();
      patterns = patterns.filter(p =>
        p.name?.toLowerCase().includes(searchLower) ||
        p.problem?.toLowerCase().includes(searchLower) ||
        p.solution?.toLowerCase().includes(searchLower) ||
        p.description?.toLowerCase().includes(searchLower) ||
        (p.tags || []).some(t => t.toLowerCase().includes(searchLower))
      );
    }

    // Post-filter: keywords
    if (params.keywords && params.keywords.length > 0) {
      const keywordsLower = params.keywords.map(k => k.toLowerCase());
      patterns = patterns.filter(p => {
        const patternText = `${p.name} ${p.problem} ${p.solution} ${(p.tags || []).join(' ')}`.toLowerCase();
        return keywordsLower.some(k => patternText.includes(k));
      });
    }

    // Post-filter: minimum confidence
    if (params.minConfidence !== undefined && params.minConfidence > 0) {
      patterns = patterns.filter(p => p.confidence >= params.minConfidence!);
    }

    // Transform to PatternSummary format
    const patternSummaries: PatternSummary[] = patterns.map(p => ({
      id: p.id,
      name: p.name,
      type: p.type,
      domain: p.domain,
      problem: p.problem,
      solution: p.solution,
      confidence: p.confidence,
      status: p.status,
      successCount: p.successCount,
      failureCount: p.failureCount,
      discoveredAt: p.discoveredAt?.toISOString() || new Date().toISOString(),
      sourcePersonaId: p.sourcePersonaId,
      tags: p.tags || []
    }));

    const totalMatches = patternSummaries.length;

    console.log(`✅ Found ${totalMatches} patterns matching query`);

    // Generate helpful message
    let message: string;
    if (totalMatches === 0) {
      message = 'No patterns found. Try broadening your search or capturing new patterns when you discover solutions.';
    } else if (totalMatches === 1) {
      message = `Found 1 pattern. Use pattern/endorse to report if it helped.`;
    } else {
      message = `Found ${totalMatches} patterns. Use pattern/endorse to report outcomes and help improve confidence scores.`;
    }

    return createPersonaLearningPatternQueryResultFromParams(params, {
      success: true,
      patterns: patternSummaries,
      totalMatches,
      message
    });
  }
}
