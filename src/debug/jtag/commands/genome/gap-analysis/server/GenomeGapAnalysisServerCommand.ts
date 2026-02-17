/**
 * Genome Gap Analysis Command — Server Implementation
 *
 * Reads competition state and exam results from the database,
 * computes per-persona performance gaps relative to the field,
 * and returns prioritized remediation recommendations.
 *
 * This is a read-only analytics command — it does not modify data.
 */

import { CommandBase, type ICommandDaemon } from '@daemons/command-daemon/shared/CommandBase';
import type { JTAGContext } from '@system/core/types/JTAGTypes';
import { ValidationError } from '@system/core/types/ErrorTypes';
import type {
  GenomeGapAnalysisParams,
  GenomeGapAnalysisResult,
} from '../shared/GenomeGapAnalysisTypes';
import { createGenomeGapAnalysisResultFromParams } from '../shared/GenomeGapAnalysisTypes';
import { Commands } from '@system/core/shared/Commands';
import { CompetitionEntity } from '@system/genome/entities/CompetitionEntity';
import type { CompetitorEntry, GapAnalysis, TopicGap } from '@system/genome/shared/CompetitionTypes';
import type { UUID } from '@system/core/types/CrossPlatformUUID';

export class GenomeGapAnalysisServerCommand extends CommandBase<GenomeGapAnalysisParams, GenomeGapAnalysisResult> {

  constructor(context: JTAGContext, subpath: string, commander: ICommandDaemon) {
    super('genome/gap-analysis', context, subpath, commander);
  }

  async execute(params: GenomeGapAnalysisParams): Promise<GenomeGapAnalysisResult> {
    const { competitionId, personaId } = params;

    if (!competitionId?.trim()) {
      throw new ValidationError('competitionId', 'Missing required parameter.');
    }

    // 1. Load competition entity
    const readResult = await Commands.execute('data/read', {
      collection: CompetitionEntity.collection,
      id: competitionId,
    } as any) as any;

    if (!readResult.success || !readResult.data) {
      return createGenomeGapAnalysisResultFromParams(params, {
        success: false,
        error: `Competition not found: ${competitionId}`,
        analyses: [],
        skill: '',
        totalTopics: 0,
      });
    }

    const competition = readResult.data as CompetitionEntity;
    const { competitors, skill } = competition;

    if (!competitors || competitors.length === 0) {
      return createGenomeGapAnalysisResultFromParams(params, {
        success: false,
        error: 'Competition has no competitors',
        analyses: [],
        skill,
        totalTopics: 0,
      });
    }

    // 2. Load exam results for all competitors
    const examResults = await this.loadExamResults(competitionId);

    // 3. Determine total topics from the max topic index seen
    const totalTopics = this.computeTotalTopics(competitors);

    // 4. Compute per-topic field statistics
    const fieldStats = this.computeFieldStats(competitors, totalTopics);

    // 5. Build gap analysis per competitor
    const targetCompetitors = personaId
      ? competitors.filter(c => c.personaId === personaId)
      : competitors;

    const analyses: GapAnalysis[] = targetCompetitors.map(competitor =>
      this.analyzeCompetitor(competitor, competitors, fieldStats, competitionId as UUID, totalTopics)
    );

    console.log(`\u{1F4CA} GAP ANALYSIS: ${analyses.length} personas analyzed for "${skill}" (${totalTopics} topics)`);

    return createGenomeGapAnalysisResultFromParams(params, {
      success: true,
      analyses,
      skill,
      totalTopics,
    });
  }

  /**
   * Load exam results for all sessions in this competition.
   */
  private async loadExamResults(competitionId: string): Promise<any[]> {
    const listResult = await Commands.execute('data/list', {
      collection: 'academy_examinations',
      filter: { sessionId: competitionId },
      orderBy: [{ field: 'createdAt', direction: 'asc' }],
    } as any) as any;

    return listResult.success ? (listResult.data ?? []) : [];
  }

  /**
   * Determine total topics from competitor score arrays.
   */
  private computeTotalTopics(competitors: CompetitorEntry[]): number {
    let max = 0;
    for (const c of competitors) {
      if (c.topicScores.length > max) {
        max = c.topicScores.length;
      }
    }
    return max;
  }

  /**
   * Compute per-topic field best and average scores.
   */
  private computeFieldStats(competitors: CompetitorEntry[], totalTopics: number): Array<{ best: number; average: number }> {
    const stats: Array<{ best: number; average: number }> = [];

    for (let t = 0; t < totalTopics; t++) {
      const scores = competitors
        .map(c => c.topicScores[t] ?? 0)
        .filter(s => s > 0);

      if (scores.length === 0) {
        stats.push({ best: 0, average: 0 });
      } else {
        const best = Math.max(...scores);
        const average = scores.reduce((a, b) => a + b, 0) / scores.length;
        stats.push({ best, average: Math.round(average * 10) / 10 });
      }
    }

    return stats;
  }

  /**
   * Build a full gap analysis for one competitor.
   */
  private analyzeCompetitor(
    competitor: CompetitorEntry,
    allCompetitors: CompetitorEntry[],
    fieldStats: Array<{ best: number; average: number }>,
    competitionId: UUID,
    totalTopics: number,
  ): GapAnalysis {
    const topicGaps: TopicGap[] = [];

    for (let t = 0; t < totalTopics; t++) {
      const personaScore = competitor.topicScores[t] ?? 0;
      const { best: fieldBest, average: fieldAverage } = fieldStats[t];

      topicGaps.push({
        topicIndex: t,
        topicName: `Topic ${t + 1}`, // Enriched from curriculum if available
        personaScore,
        fieldBest,
        fieldAverage,
        gapFromBest: personaScore - fieldBest,
        gapFromAverage: Math.round((personaScore - fieldAverage) * 10) / 10,
        weakAreas: [], // Populated from exam feedback
      });
    }

    // Sort by gap (worst first) for weakness identification
    const sortedByGap = [...topicGaps].sort((a, b) => a.gapFromBest - b.gapFromBest);
    const weakestTopics = sortedByGap
      .filter(g => g.gapFromBest < 0)
      .slice(0, 3)
      .map(g => g.topicName);

    // Strongest = best gap from average (sorted descending)
    const sortedByStrength = [...topicGaps].sort((a, b) => b.gapFromAverage - a.gapFromAverage);
    const strongestTopics = sortedByStrength
      .filter(g => g.gapFromAverage > 0)
      .slice(0, 3)
      .map(g => g.topicName);

    // Remediation priorities = weakest topics
    const remediationPriorities = weakestTopics.length > 0
      ? weakestTopics
      : sortedByGap.slice(0, 2).map(g => g.topicName);

    return {
      personaId: competitor.personaId,
      personaName: competitor.personaName,
      competitionId,
      topicGaps,
      overallRank: competitor.rank,
      overallAverage: competitor.averageScore,
      weakestTopics,
      strongestTopics,
      remediationPriorities,
    };
  }
}
