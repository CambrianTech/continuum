/**
 * OpenProposalsSource - Injects active voting proposals into persona RAG context
 *
 * Queries DecisionProposalEntity with status='voting', filters out proposals
 * the persona already voted on, and formats them into the system prompt.
 * Personas naturally vote when they see open proposals — no special detection needed.
 *
 * Priority 25: Above governance guidance (20) since these are actionable items,
 * but below most other sources.
 * Budget: 3% — proposals are compact text.
 */

import type { RAGSource, RAGSection, RAGSourceContext } from '../shared/RAGSource';
import { ORM } from '../../../daemons/data-daemon/server/ORM';
import type { DecisionProposalEntity, DecisionOption, RankedVote } from '../../data/entities/DecisionProposalEntity';
import type { DataRecord } from '../../../daemons/data-daemon/shared/DataStorageAdapter';
import { COLLECTIONS } from '../../shared/Constants';

/**
 * Format a single proposal for the system prompt.
 */
function formatProposal(record: DataRecord<DecisionProposalEntity>): string {
  const proposal = record.data;
  const options = (proposal.options as DecisionOption[])
    .map((o, i) => `  ${i + 1}. ${o.label}: ${o.description}`)
    .join('\n');
  const voteCount = (proposal.votes as RankedVote[])?.length ?? 0;
  const deadline = new Date(proposal.deadline).toLocaleString();

  return `- **${proposal.topic}** (by ${proposal.proposerName}, ${voteCount} votes so far, deadline: ${deadline})
  ID: ${record.id}
${options}`;
}

const EMPTY_SECTION: RAGSection = {
  sourceName: 'open-proposals',
  tokenCount: 0,
  loadTimeMs: 0,
  systemPromptSection: undefined,
};

export class OpenProposalsSource implements RAGSource {
  readonly name = 'open-proposals';
  readonly priority = 25;
  readonly defaultBudgetPercent = 3;

  isApplicable(_context: RAGSourceContext): boolean {
    return true;
  }

  async load(context: RAGSourceContext, allocatedBudget: number): Promise<RAGSection> {
    const startTime = Date.now();

    if (allocatedBudget < 30) {
      return { ...EMPTY_SECTION, loadTimeMs: Date.now() - startTime };
    }

    try {
      const result = await ORM.query<DecisionProposalEntity>({
        collection: COLLECTIONS.DECISION_PROPOSALS,
        filter: { status: 'voting' },
        limit: 10,
        sort: [{ field: 'deadline', direction: 'asc' }],
      }, 'default');

      const records = result.data ?? [];
      if (records.length === 0) {
        return { ...EMPTY_SECTION, loadTimeMs: Date.now() - startTime };
      }

      // Filter out proposals this persona already voted on
      const unvoted = records.filter(r => {
        const votes = (r.data.votes as RankedVote[]) ?? [];
        return !votes.some(v => v.voterId === context.personaId);
      });

      if (unvoted.length === 0) {
        return { ...EMPTY_SECTION, loadTimeMs: Date.now() - startTime };
      }

      const formatted = unvoted.map(formatProposal).join('\n\n');
      const section = `
=== OPEN PROPOSALS AWAITING YOUR VOTE ===
The following proposals need your ranked-choice vote. Use collaboration/decision/vote to participate.

${formatted}
================================`;

      const tokenCount = Math.ceil(section.length / 4);

      if (tokenCount > allocatedBudget) {
        const reduced = unvoted.slice(0, 3).map(formatProposal).join('\n\n');
        const reducedSection = `
=== OPEN PROPOSALS (${unvoted.length} total, showing 3) ===
${reduced}
Use collaboration/decision/list to see all. Vote with collaboration/decision/vote.
================================`;
        return {
          sourceName: this.name,
          tokenCount: Math.ceil(reducedSection.length / 4),
          loadTimeMs: Date.now() - startTime,
          systemPromptSection: reducedSection,
        };
      }

      return {
        sourceName: this.name,
        tokenCount,
        loadTimeMs: Date.now() - startTime,
        systemPromptSection: section,
      };
    } catch {
      return { ...EMPTY_SECTION, loadTimeMs: Date.now() - startTime };
    }
  }
}
