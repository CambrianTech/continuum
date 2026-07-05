/**
 * DecisionReportFormatter - Convert decision data to human-readable markdown
 */

import type { DecisionForReport } from '../shared/DecisionReportTypes';

export class DecisionReportFormatter {
  /**
   * Format decisions as markdown report
   */
  static formatReport(
    decisions: DecisionForReport[],
    options: {
      verbose?: boolean;
      groupByActor?: boolean;
      startDate?: string;
      endDate?: string;
    }
  ): string {
    const lines: string[] = [];

    // Header
    lines.push('# AI Decision Intelligence Report');
    lines.push('');
    lines.push(`Generated: ${new Date().toISOString()}`);
    lines.push('');

    // Date range
    if (options.startDate || options.endDate) {
      lines.push('## Date Range');
      lines.push('');
      if (options.startDate) lines.push(`- **Start**: ${options.startDate}`);
      if (options.endDate) lines.push(`- **End**: ${options.endDate}`);
      lines.push('');
    }

    // Summary statistics
    lines.push('## Summary Statistics');
    lines.push('');
    const stats = this.calculateStats(decisions);
    lines.push(`- **Total Decisions**: ${stats.total}`);
    lines.push(`- **Posted**: ${stats.posted} (${stats.postedPct}%)`);
    lines.push(`- **Silent**: ${stats.silent} (${stats.silentPct}%)`);
    lines.push(`- **Errors**: ${stats.errors}`);
    lines.push(`- **Average Confidence**: ${stats.avgConfidence.toFixed(2)}`);
    lines.push(`- **Unique Actors**: ${stats.uniqueActors}`);
    lines.push('');

    // Actor breakdown
    lines.push('## Actor Breakdown');
    lines.push('');
    const actorStats = this.calculateActorStats(decisions);
    lines.push('| Actor | Total | Posted | Silent | Avg Confidence |');
    lines.push('|-------|-------|--------|--------|----------------|');
    for (const [actor, data] of Object.entries(actorStats)) {
      lines.push(`| ${actor} | ${data.total} | ${data.posted} | ${data.silent} | ${data.avgConfidence.toFixed(2)} |`);
    }
    lines.push('');

    // Decisions
    if (options.groupByActor) {
      lines.push(...this.formatByActor(decisions, options.verbose));
    } else {
      lines.push(...this.formatChronological(decisions, options.verbose));
    }

    return lines.join('\n');
  }

  /**
   * Calculate summary statistics
   */
  private static calculateStats(decisions: DecisionForReport[]): {
    total: number;
    posted: number;
    silent: number;
    errors: number;
    postedPct: number;
    silentPct: number;
    avgConfidence: number;
    uniqueActors: number;
  } {
    const total = decisions.length;
    const posted = decisions.filter(d => d.action === 'POSTED').length;
    const silent = decisions.filter(d => d.action === 'SILENT').length;
    const errors = decisions.filter(d => d.action === 'ERROR' || d.action === 'TIMEOUT').length;
    const avgConfidence = total > 0
      ? decisions.reduce((sum, d) => sum + d.confidence, 0) / total
      : 0;
    const uniqueActors = new Set(decisions.map(d => d.actorName)).size;

    return {
      total,
      posted,
      silent,
      errors,
      postedPct: total > 0 ? Math.round((posted / total) * 100) : 0,
      silentPct: total > 0 ? Math.round((silent / total) * 100) : 0,
      avgConfidence,
      uniqueActors
    };
  }

  /**
   * Calculate per-actor statistics
   */
  private static calculateActorStats(decisions: DecisionForReport[]): Record<string, {
    total: number;
    posted: number;
    silent: number;
    avgConfidence: number;
  }> {
    const stats: Record<string, { total: number; posted: number; silent: number; confidenceSum: number }> = {};

    for (const decision of decisions) {
      if (!stats[decision.actorName]) {
        stats[decision.actorName] = { total: 0, posted: 0, silent: 0, confidenceSum: 0 };
      }

      stats[decision.actorName].total++;
      stats[decision.actorName].confidenceSum += decision.confidence;

      if (decision.action === 'POSTED') {
        stats[decision.actorName].posted++;
      } else if (decision.action === 'SILENT') {
        stats[decision.actorName].silent++;
      }
    }

    // Convert to final format
    const result: Record<string, { total: number; posted: number; silent: number; avgConfidence: number }> = {};
    for (const [actor, data] of Object.entries(stats)) {
      result[actor] = {
        total: data.total,
        posted: data.posted,
        silent: data.silent,
        avgConfidence: data.total > 0 ? data.confidenceSum / data.total : 0
      };
    }

    return result;
  }

  /**
   * Format decisions chronologically
   */
  private static formatChronological(decisions: DecisionForReport[], verbose?: boolean): string[] {
    const lines: string[] = [];

    lines.push('## Decision Timeline');
    lines.push('');
    lines.push('Decisions in chronological order:');
    lines.push('');

    for (const decision of decisions) {
      lines.push(...this.formatDecision(decision, verbose));
      lines.push('');
      lines.push('---');
      lines.push('');
    }

    return lines;
  }

  /**
   * Format decisions grouped by actor
   */
  private static formatByActor(decisions: DecisionForReport[], verbose?: boolean): string[] {
    const lines: string[] = [];

    // Group by actor
    const byActor = new Map<string, DecisionForReport[]>();
    for (const decision of decisions) {
      if (!byActor.has(decision.actorName)) {
        byActor.set(decision.actorName, []);
      }
      byActor.get(decision.actorName)!.push(decision);
    }

    lines.push('## Decisions by Actor');
    lines.push('');

    for (const [actor, actorDecisions] of byActor.entries()) {
      lines.push(`### ${actor}`);
      lines.push('');
      lines.push(`Total decisions: ${actorDecisions.length}`);
      lines.push('');

      for (const decision of actorDecisions) {
        lines.push(...this.formatDecision(decision, verbose));
        lines.push('');
      }

      lines.push('---');
      lines.push('');
    }

    return lines;
  }

  /**
   * Format a single decision
   */
  private static formatDecision(decision: DecisionForReport, verbose?: boolean): string[] {
    const lines: string[] = [];

    // Header
    lines.push(`### Decision: ${decision.action} by ${decision.actorName}`);
    lines.push('');

    // Metadata
    lines.push(`**Time**: ${new Date(decision.timestamp).toISOString()}`);
    lines.push(`**Actor**: ${decision.actorName} (${decision.actorType})`);
    lines.push(`**Action**: ${decision.action}`);
    lines.push(`**Confidence**: ${decision.confidence.toFixed(2)}`);
    if (decision.modelUsed) {
      lines.push(`**Model**: ${decision.modelUsed}`);
    }
    lines.push('');

    // Reasoning
    if (decision.reasoning) {
      lines.push('**Reasoning**:');
      lines.push('');
      lines.push(`> ${decision.reasoning}`);
      lines.push('');
    }

    // Response content (if posted)
    if (decision.action === 'POSTED' && decision.responseContent) {
      lines.push('**Response**:');
      lines.push('');
      lines.push('```');
      lines.push(decision.responseContent);
      lines.push('```');
      lines.push('');
    }

    // Ambient state
    lines.push('**Ambient State**:');
    lines.push('');
    lines.push(`- Temperature: ${decision.ambientState.temperature.toFixed(2)}`);
    lines.push(`- User Present: ${decision.ambientState.userPresent ? 'Yes' : 'No'}`);
    lines.push(`- Time Since Last Response: ${Math.round(decision.ambientState.timeSinceLastResponse / 1000)}s`);
    lines.push(`- Mentioned by Name: ${decision.ambientState.mentionedByName ? 'Yes' : 'No'}`);
    lines.push('');

    // Coordination snapshot
    lines.push('**Coordination State**:');
    lines.push('');
    if (decision.coordinationSnapshot.phase) {
      lines.push(`- Phase: ${decision.coordinationSnapshot.phase}`);
    }
    if (decision.coordinationSnapshot.availableSlots !== undefined) {
      lines.push(`- Available Slots: ${decision.coordinationSnapshot.availableSlots}`);
    }
    lines.push(`- Others Considering: ${decision.coordinationSnapshot.othersConsideringCount}`);
    if (decision.coordinationSnapshot.othersConsideringNames.length > 0) {
      lines.push(`- Considering Names: ${decision.coordinationSnapshot.othersConsideringNames.join(', ')}`);
    }
    lines.push('');

    // RAG context - may be inline or in blob storage
    if (!decision.ragContext && decision.ragContextRef) {
      // RAG context is in blob storage (large context)
      lines.push('**RAG Context**: Stored in blob storage');
      lines.push(`- Reference: \`${decision.ragContextRef}\``);
      lines.push('- (Use CoordinationDecisionLogger.retrieveRAGContext() to fetch)');
      lines.push('');
    } else if (decision.ragContext) {
      if (verbose) {
        lines.push('**RAG Context (COMPLETE - EXACTLY what AI saw)**:');
        lines.push('');

        // FULL system prompt - this is critical
        lines.push('**System Prompt** (complete):');
        lines.push('```');
        lines.push(decision.ragContext.identity.systemPrompt);
        lines.push('```');
        lines.push('');

        lines.push('**Identity**:');
        lines.push(`- Role: ${decision.ragContext.identity.role}`);
        lines.push(`- Bio: ${decision.ragContext.identity.bio || ''}`);
        lines.push('');

        // FULL conversation history - every single message
        lines.push(`**Conversation History** (ALL ${decision.ragContext.conversationHistory.length} messages):`);
        lines.push('');
        for (const msg of decision.ragContext.conversationHistory) {
          const time = new Date(msg.timestamp).toISOString();
          lines.push(`---`);
          lines.push(`**[${time}] ${msg.role.toUpperCase()}**:`);
          lines.push('');
          lines.push(msg.content);  // FULL content, no truncation
          lines.push('');
        }

        lines.push(`**Context Metadata**:`);
        lines.push(`- Token Count: ${decision.ragContext.metadata.tokenCount}`);
        lines.push(`- Context Window: ${decision.ragContext.metadata.contextWindow}`);
        lines.push(`- Total Messages: ${decision.ragContext.conversationHistory.length}`);
        lines.push('');
      } else {
        lines.push('**RAG Context Summary**:');
        lines.push('');
        lines.push(`- Role: ${decision.ragContext.identity.role}`);
        lines.push(`- Messages in Context: ${decision.ragContext.conversationHistory.length}`);
        lines.push(`- Token Count: ~${decision.ragContext.metadata.tokenCount}`);
        lines.push('');
      }
    } else {
      lines.push('**RAG Context**: Not available');
      lines.push('');
    }

    // Tags
    if (decision.metadata.tags && decision.metadata.tags.length > 0) {
      lines.push('**Tags**: ' + decision.metadata.tags.map(t => `\`${t}\``).join(', '));
      lines.push('');
    }

    return lines;
  }
}
