/**
 * Unit tests for PeerReviewTypes.ts
 *
 * Tests weighted scoring, aggregation logic, and model intelligence weights
 * WITHOUT requiring full system integration.
 */

import { describe, it, expect } from 'vitest';
import {
  calculateWeightedScore,
  aggregateProposalDecision,
  getModelIntelligenceWeight,
  type ResponseProposal,
  type ProposalRating,
  type PeerReviewThresholds
} from '../../system/user/server/modules/cognition/PeerReviewTypes';
import { generateUUID } from '../../system/core/types/CrossPlatformUUID';

describe('PeerReviewTypes - Model Intelligence Weights', () => {
  it('should return 1.0 for top-tier models', () => {
    expect(getModelIntelligenceWeight('anthropic', 'claude-sonnet-4-5-20250929')).toBe(1.0);
    expect(getModelIntelligenceWeight('anthropic', 'claude-3-opus-20240229')).toBe(1.0);
    expect(getModelIntelligenceWeight('openai', 'gpt-4')).toBe(1.0);
  });

  it('should return lower weights for smaller models', () => {
    expect(getModelIntelligenceWeight('groq', 'llama-3.1-8b-instant')).toBe(0.5);
    expect(getModelIntelligenceWeight('candle', 'llama3.2:3b')).toBe(0.3);
    expect(getModelIntelligenceWeight('sentinel', 'gpt2')).toBe(0.2);
  });

  it('should return default weight (0.5) for unknown models', () => {
    expect(getModelIntelligenceWeight('unknown', 'mystery-model')).toBe(0.5);
  });

  it('should match provider:model keys exactly (case-sensitive)', () => {
    // Keys are lowercase in MODEL_INTELLIGENCE_WEIGHTS
    expect(getModelIntelligenceWeight('anthropic', 'claude-sonnet-4-5-20250929')).toBe(1.0);
    expect(getModelIntelligenceWeight('openai', 'gpt-4')).toBe(1.0);

    // Case mismatch returns default
    expect(getModelIntelligenceWeight('Anthropic', 'claude-sonnet-4-5-20250929')).toBe(0.5);
    expect(getModelIntelligenceWeight('OPENAI', 'gpt-4')).toBe(0.5);
  });
});

describe('PeerReviewTypes - Weighted Score Calculation', () => {
  it('should return 0 for empty ratings', () => {
    expect(calculateWeightedScore([])).toBe(0);
  });

  it('should calculate simple average when all weights equal', () => {
    const ratings: ProposalRating[] = [
      createRating({ score: 0.8, reviewerWeight: 1.0 }),
      createRating({ score: 0.6, reviewerWeight: 1.0 }),
      createRating({ score: 1.0, reviewerWeight: 1.0 })
    ];

    const result = calculateWeightedScore(ratings);
    expect(result).toBeCloseTo((0.8 + 0.6 + 1.0) / 3, 2);
  });

  it('should give more influence to higher-weighted reviewers', () => {
    const ratings: ProposalRating[] = [
      createRating({ score: 0.9, reviewerWeight: 1.0 }), // GPT-4: high score, high weight
      createRating({ score: 0.3, reviewerWeight: 0.3 }), // Llama 3B: low score, low weight
      createRating({ score: 0.3, reviewerWeight: 0.3 })  // Another small model
    ];

    const result = calculateWeightedScore(ratings);
    // Should be closer to 0.9 than 0.3 because GPT-4 has more weight
    expect(result).toBeGreaterThan(0.6);
  });

  it('should handle all zero weights gracefully', () => {
    const ratings: ProposalRating[] = [
      createRating({ score: 0.8, reviewerWeight: 0 }),
      createRating({ score: 0.6, reviewerWeight: 0 })
    ];

    expect(calculateWeightedScore(ratings)).toBe(0);
  });

  it('should weight scores correctly with mixed capabilities', () => {
    // Scenario: 2 smart models say "good" (0.8), 3 small models say "bad" (0.2)
    const ratings: ProposalRating[] = [
      createRating({ score: 0.8, reviewerWeight: 1.0 }), // GPT-4
      createRating({ score: 0.8, reviewerWeight: 1.0 }), // Claude Opus
      createRating({ score: 0.2, reviewerWeight: 0.3 }), // Llama 3B
      createRating({ score: 0.2, reviewerWeight: 0.3 }), // Llama 3B
      createRating({ score: 0.2, reviewerWeight: 0.3 })  // Llama 3B
    ];

    const result = calculateWeightedScore(ratings);
    // Total weight = 2.0 + 0.9 = 2.9
    // Weighted sum = (0.8*1.0 + 0.8*1.0) + (0.2*0.3 + 0.2*0.3 + 0.2*0.3) = 1.6 + 0.18 = 1.78
    // Expected = 1.78 / 2.9 ≈ 0.614
    expect(result).toBeCloseTo(0.614, 2);
  });
});

describe('PeerReviewTypes - Proposal Decision Aggregation', () => {
  it('should approve proposal when both thresholds met', () => {
    const proposal = createProposal();
    const ratings: ProposalRating[] = [
      createRating({ score: 0.8, shouldPost: true, reviewerWeight: 1.0 }),
      createRating({ score: 0.7, shouldPost: true, reviewerWeight: 1.0 }),
      createRating({ score: 0.9, shouldPost: true, reviewerWeight: 1.0 })
    ];

    const decision = aggregateProposalDecision(proposal, ratings);

    expect(decision.shouldPost).toBe(true);
    expect(decision.weightedAvgScore).toBeCloseTo(0.8, 1); // Average of 0.8, 0.7, 0.9
    expect(decision.postVotePercentage).toBe(1.0); // 3/3 = 100%
    expect(decision.postVotes).toBe(3);
    expect(decision.totalReviewers).toBe(3);
  });

  it('should reject proposal when vote threshold not met', () => {
    const proposal = createProposal();
    const ratings: ProposalRating[] = [
      createRating({ score: 0.8, shouldPost: true, reviewerWeight: 1.0 }),
      createRating({ score: 0.7, shouldPost: false, reviewerWeight: 1.0 }),
      createRating({ score: 0.75, shouldPost: false, reviewerWeight: 1.0 })
    ];

    const decision = aggregateProposalDecision(proposal, ratings);

    expect(decision.shouldPost).toBe(false);
    expect(decision.postVotePercentage).toBeCloseTo(0.33, 2); // 1/3 = 33%
    expect(decision.reasoning).toContain('Failed vote threshold');
  });

  it('should reject proposal when weighted score too low', () => {
    const proposal = createProposal();
    const ratings: ProposalRating[] = [
      createRating({ score: 0.5, shouldPost: true, reviewerWeight: 1.0 }),
      createRating({ score: 0.55, shouldPost: true, reviewerWeight: 1.0 }),
      createRating({ score: 0.45, shouldPost: true, reviewerWeight: 1.0 })
    ];

    const decision = aggregateProposalDecision(proposal, ratings);

    expect(decision.shouldPost).toBe(false);
    expect(decision.weightedAvgScore).toBeLessThan(0.6); // Below threshold
    expect(decision.reasoning).toContain('Failed score threshold');
  });

  it('should require both thresholds to pass', () => {
    const proposal = createProposal();

    // Good score, bad votes
    const ratings1: ProposalRating[] = [
      createRating({ score: 0.9, shouldPost: true, reviewerWeight: 1.0 }),
      createRating({ score: 0.85, shouldPost: false, reviewerWeight: 1.0 }),
      createRating({ score: 0.8, shouldPost: false, reviewerWeight: 1.0 })
    ];

    const decision1 = aggregateProposalDecision(proposal, ratings1);
    expect(decision1.shouldPost).toBe(false); // Only 33% voted yes

    // Good votes, bad score
    const ratings2: ProposalRating[] = [
      createRating({ score: 0.5, shouldPost: true, reviewerWeight: 1.0 }),
      createRating({ score: 0.5, shouldPost: true, reviewerWeight: 1.0 }),
      createRating({ score: 0.4, shouldPost: true, reviewerWeight: 1.0 })
    ];

    const decision2 = aggregateProposalDecision(proposal, ratings2);
    expect(decision2.shouldPost).toBe(false); // Weighted score < 0.6
  });

  it('should handle custom thresholds', () => {
    const proposal = createProposal();
    const ratings: ProposalRating[] = [
      createRating({ score: 0.7, shouldPost: true, reviewerWeight: 1.0 }),
      createRating({ score: 0.65, shouldPost: false, reviewerWeight: 1.0 })
    ];

    // Lenient thresholds
    const lenientThresholds: PeerReviewThresholds = {
      minPostVotePercentage: 0.4,  // 40% vote
      minWeightedScore: 0.5,         // 50% score
      minReviewers: 2,
      reviewTimeoutMs: 2000
    };

    const decision = aggregateProposalDecision(proposal, ratings, lenientThresholds);
    expect(decision.shouldPost).toBe(true); // 50% voted yes, 0.675 score
  });

  it('should reject when not enough reviewers', () => {
    const proposal = createProposal();
    const ratings: ProposalRating[] = [
      createRating({ score: 0.9, shouldPost: true, reviewerWeight: 1.0 })
    ];

    const thresholds: PeerReviewThresholds = {
      minPostVotePercentage: 0.5,
      minWeightedScore: 0.6,
      minReviewers: 2, // Require at least 2 reviewers
      reviewTimeoutMs: 2000
    };

    const decision = aggregateProposalDecision(proposal, ratings, thresholds);
    expect(decision.shouldPost).toBe(false);
    expect(decision.reasoning).toContain('Insufficient reviewers');
  });

  it('should handle zero ratings gracefully', () => {
    const proposal = createProposal();
    const ratings: ProposalRating[] = [];

    const decision = aggregateProposalDecision(proposal, ratings);

    expect(decision.shouldPost).toBe(false);
    expect(decision.weightedAvgScore).toBe(0);
    expect(decision.postVotePercentage).toBe(0);
  });
});

describe('PeerReviewTypes - Real-World Scenarios', () => {
  it('should handle scenario from README: 3 AIs respond to quantum question', () => {
    // Helper AI (Llama 8B, weight=0.5): Proposal gets rejected
    const helperProposal = createProposal({ proposerName: 'Helper AI' });
    const helperRatings: ProposalRating[] = [
      createRating({ score: 0.7, shouldPost: true, reviewerWeight: 0.5, reviewerName: 'Helper AI' }),
      createRating({ score: 0.6, shouldPost: false, reviewerWeight: 1.0, reviewerName: 'Teacher AI' }),
      createRating({ score: 0.5, shouldPost: false, reviewerWeight: 1.0, reviewerName: 'Physicist AI' })
    ];

    const helperDecision = aggregateProposalDecision(helperProposal, helperRatings);
    // Weighted avg: (0.7*0.5 + 0.6*1.0 + 0.5*1.0) / 2.5 = 0.58
    expect(helperDecision.weightedAvgScore).toBeCloseTo(0.58, 2);
    expect(helperDecision.postVotePercentage).toBeCloseTo(0.33, 2); // 1/3
    expect(helperDecision.shouldPost).toBe(false);

    // Teacher AI (GPT-4, weight=1.0): Proposal gets approved
    const teacherProposal = createProposal({ proposerName: 'Teacher AI' });
    const teacherRatings: ProposalRating[] = [
      createRating({ score: 0.85, shouldPost: true, reviewerWeight: 0.5, reviewerName: 'Helper AI' }),
      createRating({ score: 0.8, shouldPost: true, reviewerWeight: 1.0, reviewerName: 'Teacher AI' }),
      createRating({ score: 0.7, shouldPost: true, reviewerWeight: 1.0, reviewerName: 'Physicist AI' })
    ];

    const teacherDecision = aggregateProposalDecision(teacherProposal, teacherRatings);
    // Weighted avg: (0.85*0.5 + 0.8*1.0 + 0.7*1.0) / 2.5 = 0.77
    expect(teacherDecision.weightedAvgScore).toBeCloseTo(0.77, 2);
    expect(teacherDecision.postVotePercentage).toBe(1.0); // 3/3
    expect(teacherDecision.shouldPost).toBe(true);

    // Physicist AI (Claude Sonnet, weight=1.0): Proposal gets approved
    const physicistProposal = createProposal({ proposerName: 'Physicist AI' });
    const physicistRatings: ProposalRating[] = [
      createRating({ score: 0.9, shouldPost: true, reviewerWeight: 0.5, reviewerName: 'Helper AI' }),
      createRating({ score: 0.75, shouldPost: true, reviewerWeight: 1.0, reviewerName: 'Teacher AI' }),
      createRating({ score: 0.95, shouldPost: true, reviewerWeight: 1.0, reviewerName: 'Physicist AI' })
    ];

    const physicistDecision = aggregateProposalDecision(physicistProposal, physicistRatings);
    // Weighted avg: (0.9*0.5 + 0.75*1.0 + 0.95*1.0) / 2.5 = 0.86
    expect(physicistDecision.weightedAvgScore).toBeCloseTo(0.86, 2);
    expect(physicistDecision.postVotePercentage).toBe(1.0); // 3/3
    expect(physicistDecision.shouldPost).toBe(true);
  });

  it('should handle edge case: all reviewers reject', () => {
    const proposal = createProposal();
    const ratings: ProposalRating[] = [
      createRating({ score: 0.3, shouldPost: false, reviewerWeight: 1.0 }),
      createRating({ score: 0.2, shouldPost: false, reviewerWeight: 1.0 }),
      createRating({ score: 0.4, shouldPost: false, reviewerWeight: 1.0 })
    ];

    const decision = aggregateProposalDecision(proposal, ratings);
    expect(decision.shouldPost).toBe(false);
    expect(decision.postVotePercentage).toBe(0);
  });

  it('should handle edge case: all reviewers approve unanimously', () => {
    const proposal = createProposal();
    const ratings: ProposalRating[] = [
      createRating({ score: 0.95, shouldPost: true, reviewerWeight: 1.0 }),
      createRating({ score: 0.9, shouldPost: true, reviewerWeight: 1.0 }),
      createRating({ score: 0.85, shouldPost: true, reviewerWeight: 1.0 })
    ];

    const decision = aggregateProposalDecision(proposal, ratings);
    expect(decision.shouldPost).toBe(true);
    expect(decision.postVotePercentage).toBe(1.0);
    expect(decision.weightedAvgScore).toBeGreaterThan(0.85);
  });
});

// Helper functions for creating test data

function createRating(overrides: Partial<ProposalRating> = {}): ProposalRating {
  return {
    ratingId: generateUUID(),
    proposalId: generateUUID(),
    reviewerId: generateUUID(),
    reviewerName: overrides.reviewerName || 'Test Reviewer',
    reviewerWeight: overrides.reviewerWeight ?? 1.0,
    score: overrides.score ?? 0.5,
    shouldPost: overrides.shouldPost ?? false,
    ratedAt: Date.now(),
    reasoning: overrides.reasoning || 'Test reasoning',
    ...overrides
  };
}

function createProposal(overrides: Partial<ResponseProposal> = {}): ResponseProposal {
  return {
    proposalId: generateUUID(),
    roomId: generateUUID(),
    respondingToId: generateUUID(),
    proposerId: generateUUID(),
    proposerName: overrides.proposerName || 'Test AI',
    proposerModelProvider: 'openai',
    proposerModelId: 'gpt-4',
    responseText: 'This is a test response',
    confidence: 0.8,
    inferenceDuration: 3000,
    declaredAt: Date.now(),
    currentContext: {
      newMessagesSinceInference: 0,
      otherActiveProposals: 0
    },
    ...overrides
  };
}
