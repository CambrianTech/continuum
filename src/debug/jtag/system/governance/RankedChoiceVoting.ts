/**
 * Ranked-Choice Voting Algorithm
 *
 * Implements instant-runoff voting (IRV) to determine consensus winners
 * Prevents vote splitting and ensures majority support
 *
 * Algorithm:
 * 1. Count first-choice votes for each option
 * 2. If any option has >50% of votes, it wins
 * 3. Otherwise, eliminate option with fewest votes
 * 4. Redistribute eliminated option's votes to next-ranked choices
 * 5. Repeat until a winner emerges
 */

import type { VoteRecord, DecisionOption, VoteResults, RoundResult } from '../data/entities/DecisionEntity';

export class RankedChoiceVoting {
  /**
   * Calculate winner using instant-runoff voting algorithm
   *
   * @param options - Available options in the decision
   * @param votes - All cast votes with ranked choices
   * @param totalEligibleVoters - Total number of eligible voters (for participation metrics)
   * @returns Complete vote results with winner, rounds, and participation metrics
   */
  static calculateWinner(
    options: readonly DecisionOption[],
    votes: VoteRecord[],
    totalEligibleVoters: number
  ): VoteResults {
    // Validation
    if (options.length === 0) {
      throw new Error('Cannot calculate winner: no options provided');
    }
    if (votes.length === 0) {
      throw new Error('Cannot calculate winner: no votes cast');
    }
    if (totalEligibleVoters < 1) {
      throw new Error('Cannot calculate winner: totalEligibleVoters must be at least 1');
    }

    // Initialize active options (options not yet eliminated)
    const activeOptions = new Set(options.map(opt => opt.id));
    const rounds: RoundResult[] = [];
    let roundNumber = 1;

    // Track which votes are still active (not yet assigned to eliminated option)
    const activeVotes = votes.map(vote => ({
      ...vote,
      currentChoiceIndex: 0  // Start with first choice
    }));

    while (activeOptions.size > 1) {
      // Count votes for each active option based on current choice index
      const tallies: Record<string, number> = {};
      for (const optionId of activeOptions) {
        tallies[optionId] = 0;
      }

      // Tally votes
      for (const vote of activeVotes) {
        // Find the highest-ranked choice that's still active
        let voteCounted = false;
        for (let i = vote.currentChoiceIndex; i < vote.rankedChoices.length; i++) {
          const choiceId = vote.rankedChoices[i];
          if (activeOptions.has(choiceId)) {
            tallies[choiceId]++;
            vote.currentChoiceIndex = i;  // Update to current active choice
            voteCounted = true;
            break;
          }
        }

        // If no active choices remain in this vote, it's exhausted (doesn't count)
        if (!voteCounted) {
          vote.currentChoiceIndex = vote.rankedChoices.length;  // Mark as exhausted
        }
      }

      // Check for majority winner (>50% of active votes)
      const totalActiveVotes = activeVotes.filter(v => v.currentChoiceIndex < v.rankedChoices.length).length;
      const majorityThreshold = totalActiveVotes / 2;

      let winner: string | null = null;
      for (const [optionId, voteCount] of Object.entries(tallies)) {
        if (voteCount > majorityThreshold) {
          winner = optionId;
          break;
        }
      }

      // Record round results
      const round: RoundResult = {
        round: roundNumber,
        tallies: { ...tallies }
      };

      if (winner) {
        // Winner found!
        rounds.push(round);
        return this.buildResults(winner, votes, totalEligibleVoters, rounds, tallies);
      }

      // No winner yet - eliminate option with fewest votes
      let minVotes = Infinity;
      let eliminatedOption: string | null = null;

      for (const [optionId, voteCount] of Object.entries(tallies)) {
        if (voteCount < minVotes) {
          minVotes = voteCount;
          eliminatedOption = optionId;
        }
      }

      if (!eliminatedOption) {
        throw new Error('Algorithm error: no option to eliminate');
      }

      // Record elimination in round results
      round.eliminated = eliminatedOption;
      rounds.push(round);

      // Remove eliminated option from active set
      activeOptions.delete(eliminatedOption);
      roundNumber++;
    }

    // Only one option remains - it wins by default
    const winner = Array.from(activeOptions)[0];
    const finalTallies: Record<string, number> = {};
    for (const optionId of options.map(opt => opt.id)) {
      finalTallies[optionId] = activeOptions.has(optionId) ? activeVotes.length : 0;
    }

    return this.buildResults(winner, votes, totalEligibleVoters, rounds, finalTallies);
  }

  /**
   * Build final VoteResults object
   */
  private static buildResults(
    winner: string,
    votes: VoteRecord[],
    totalEligibleVoters: number,
    rounds: RoundResult[],
    finalTallies: Record<string, number>
  ): VoteResults {
    const totalVoted = votes.length;
    const participationPercentage = (totalVoted / totalEligibleVoters) * 100;

    return {
      winner,
      method: 'ranked-choice',
      rounds,
      participation: {
        totalEligible: totalEligibleVoters,
        totalVoted,
        percentage: Math.round(participationPercentage * 100) / 100  // Round to 2 decimals
      },
      finalTallies
    };
  }

  /**
   * Validate ranked choices against available options
   * Throws error if validation fails
   */
  static validateRankedChoices(
    rankedChoices: string[],
    options: readonly DecisionOption[]
  ): void {
    const validOptionIds = options.map(opt => opt.id);

    // Check each choice is a valid option ID
    for (const choiceId of rankedChoices) {
      if (!validOptionIds.includes(choiceId)) {
        throw new Error(`Invalid option ID in ranked choices: ${choiceId}`);
      }
    }

    // Check for duplicates
    const uniqueChoices = new Set(rankedChoices);
    if (uniqueChoices.size !== rankedChoices.length) {
      throw new Error('Ranked choices cannot contain duplicates');
    }

    // Must rank at least one option
    if (rankedChoices.length === 0) {
      throw new Error('Must rank at least one option');
    }
  }

  /**
   * Check if quorum is met
   *
   * @param votesCount - Number of votes cast
   * @param requiredQuorum - Required minimum number of votes
   * @returns true if quorum met or no quorum required
   */
  static isQuorumMet(votesCount: number, requiredQuorum?: number): boolean {
    if (requiredQuorum === undefined || requiredQuorum === null) {
      return true;  // No quorum requirement
    }
    return votesCount >= requiredQuorum;
  }

  /**
   * Get human-readable summary of voting rounds
   * Useful for displaying results to users
   */
  static formatRoundsSummary(rounds: RoundResult[]): string {
    if (rounds.length === 0) {
      return 'No rounds processed';
    }

    if (rounds.length === 1 && !rounds[0].eliminated) {
      return 'Winner determined in first round (majority achieved)';
    }

    const lines: string[] = [];
    for (const round of rounds) {
      const tallyStr = Object.entries(round.tallies)
        .map(([id, count]) => `${id}: ${count}`)
        .join(', ');

      if (round.eliminated) {
        lines.push(`Round ${round.round}: ${tallyStr} → Eliminated: ${round.eliminated}`);
      } else {
        lines.push(`Round ${round.round}: ${tallyStr} → Winner found`);
      }
    }

    return lines.join('\n');
  }
}
