/**
 * CondorcetUtils - Shared voting utilities for ranked-choice decision making
 */

export interface RankedVote {
  rankings: string[]; // Ordered list of option IDs from most to least preferred
}

export interface CondorcetWinner {
  optionId: string;
  label: string;
  wins: number;
}

/**
 * Calculate Condorcet winner using pairwise comparisons
 * Returns the option that beats all others in head-to-head matchups
 * If no Condorcet winner exists (cycle), returns option with most pairwise wins
 *
 * @param votes Array of ranked votes from participants
 * @param options Available options with IDs and labels
 * @returns Winner information or null if no votes/options
 */
export function calculateCondorcetWinner(
  votes: RankedVote[],
  options: Array<{ id: string; label: string }>
): CondorcetWinner | null {
  if (votes.length === 0 || options.length === 0) {
    return null;
  }

  // Create pairwise comparison matrix
  const matrix = new Map<string, Map<string, number>>();

  // Initialize matrix
  for (const opt of options) {
    const innerMap = new Map<string, number>();
    for (const other of options) {
      if (opt.id !== other.id) {
        innerMap.set(other.id, 0);
      }
    }
    matrix.set(opt.id, innerMap);
  }

  // Count pairwise preferences
  for (const vote of votes) {
    const rankedChoices = vote.rankings;

    // Compare each pair of options in this voter's ranking
    for (let i = 0; i < rankedChoices.length; i++) {
      for (let j = i + 1; j < rankedChoices.length; j++) {
        const preferred = rankedChoices[i];
        const lessPreferred = rankedChoices[j];

        // Increment: preferred beats lessPreferred
        const row = matrix.get(preferred);
        if (row) {
          row.set(lessPreferred, (row.get(lessPreferred) ?? 0) + 1);
        }
      }
    }
  }

  // Find Condorcet winner (option that beats all others head-to-head)
  const wins = new Map<string, number>();

  for (const opt of options) {
    let winsCount = 0;
    const row = matrix.get(opt.id);

    if (row) {
      for (const other of options) {
        if (opt.id === other.id) continue;

        const votesFor = row.get(other.id) ?? 0;
        const otherRow = matrix.get(other.id);
        const votesAgainst = otherRow?.get(opt.id) ?? 0;

        // This option beats 'other' if it gets more votes in head-to-head
        if (votesFor > votesAgainst) {
          winsCount++;
        }
      }
    }

    wins.set(opt.id, winsCount);
  }

  // Find option with most pairwise wins
  let winner: CondorcetWinner | null = null;
  let maxWins = -1;

  for (const [optionId, winsCount] of wins.entries()) {
    if (winsCount > maxWins) {
      maxWins = winsCount;
      const option = options.find(o => o.id === optionId);
      if (option) {
        winner = { optionId, label: option.label, wins: winsCount };
      }
    }
  }

  return winner;
}
