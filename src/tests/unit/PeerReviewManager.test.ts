/**
 * Unit tests for PeerReviewManager.ts
 *
 * Tests fast-path detection, session management, proposal/rating flow, and decision aggregation.
 */

import { describe, it, expect, beforeEach } from 'vitest';
import {
  declareProposal,
  getActiveProposals,
  submitRatings,
  makeDecisions,
  shouldEnterPeerReview,
  waitRevelationWindow
} from '../../system/user/server/modules/cognition/PeerReviewManager';
import type { ResponseProposal, ProposalRating, PeerReviewThresholds } from '../../system/user/server/modules/cognition/PeerReviewTypes';
import { generateUUID } from '../../system/core/types/CrossPlatformUUID';
import type { UUID } from '../../system/core/types/CrossPlatformUUID';

describe('PeerReviewManager - Fast-Path Detection', () => {
  let roomId: UUID;
  let messageId: UUID;

  beforeEach(() => {
    roomId = generateUUID();
    messageId = generateUUID();
  });

  it('should use fast-path when no other proposals exist', () => {
    const result = shouldEnterPeerReview({
      roomId,
      messageId,
      newMessagesSinceInference: 0
    });

    expect(result).toBe(false); // Fast-path
  });

  it('should use slow-path when other proposals exist', async () => {
    // Declare a proposal first
    await declareProposal(createProposalInput(roomId, messageId));

    // Second AI checks - should see collision
    const result = shouldEnterPeerReview({
      roomId,
      messageId,
      newMessagesSinceInference: 0
    });

    expect(result).toBe(true); // Slow-path (collision detected)
  });

  it('should use slow-path when context changed during inference', () => {
    const result = shouldEnterPeerReview({
      roomId,
      messageId,
      newMessagesSinceInference: 2 // New messages appeared
    });

    expect(result).toBe(true); // Slow-path (context changed)
  });

  it('should handle multiple rooms independently', async () => {
    const room1 = generateUUID();
    const room2 = generateUUID();
    const msg1 = generateUUID();
    const msg2 = generateUUID();

    // Declare proposal in room1
    await declareProposal(createProposalInput(room1, msg1));

    // Check room2 - should be fast-path (different room)
    const result = shouldEnterPeerReview({
      roomId: room2,
      messageId: msg2,
      newMessagesSinceInference: 0
    });

    expect(result).toBe(false); // Fast-path (no collisions in room2)
  });
});

describe('PeerReviewManager - Proposal Declaration', () => {
  let roomId: UUID;
  let messageId: UUID;

  beforeEach(() => {
    roomId = generateUUID();
    messageId = generateUUID();
  });

  it('should create proposal with ID and timestamp', async () => {
    const input = createProposalInput(roomId, messageId);

    const proposal = await declareProposal(input);

    expect(proposal.proposalId).toBeDefined();
    expect(proposal.declaredAt).toBeGreaterThan(0);
    expect(proposal.proposerName).toBe(input.proposerName);
    expect(proposal.responseText).toBe(input.responseText);
  });

  it('should store proposal in session', async () => {
    const input = createProposalInput(roomId, messageId);

    await declareProposal(input);

    const proposals = getActiveProposals(roomId, messageId);
    expect(proposals).toHaveLength(1);
    expect(proposals[0].proposerName).toBe(input.proposerName);
  });

  it('should handle multiple proposals for same message', async () => {
    const input1 = createProposalInput(roomId, messageId, { proposerName: 'AI 1' });
    const input2 = createProposalInput(roomId, messageId, { proposerName: 'AI 2' });
    const input3 = createProposalInput(roomId, messageId, { proposerName: 'AI 3' });

    await declareProposal(input1);
    await declareProposal(input2);
    await declareProposal(input3);

    const proposals = getActiveProposals(roomId, messageId);
    expect(proposals).toHaveLength(3);
    expect(proposals.map(p => p.proposerName)).toEqual(['AI 1', 'AI 2', 'AI 3']);
  });

  it('should isolate proposals by message ID', async () => {
    const message1 = generateUUID();
    const message2 = generateUUID();

    await declareProposal(createProposalInput(roomId, message1, { proposerName: 'AI 1' }));
    await declareProposal(createProposalInput(roomId, message2, { proposerName: 'AI 2' }));

    const proposals1 = getActiveProposals(roomId, message1);
    const proposals2 = getActiveProposals(roomId, message2);

    expect(proposals1).toHaveLength(1);
    expect(proposals2).toHaveLength(1);
    expect(proposals1[0].proposerName).toBe('AI 1');
    expect(proposals2[0].proposerName).toBe('AI 2');
  });
});

describe('PeerReviewManager - Rating Submission', () => {
  let roomId: UUID;
  let messageId: UUID;
  let proposal1: ResponseProposal;
  let proposal2: ResponseProposal;

  beforeEach(async () => {
    roomId = generateUUID();
    messageId = generateUUID();

    // Declare two proposals
    proposal1 = await declareProposal(createProposalInput(roomId, messageId, { proposerName: 'AI 1' }));
    proposal2 = await declareProposal(createProposalInput(roomId, messageId, { proposerName: 'AI 2' }));
  });

  it('should accept ratings for proposals', async () => {
    const ratings: ProposalRating[] = [
      createRating(proposal1.proposalId, { reviewerName: 'Reviewer 1', score: 0.8 }),
      createRating(proposal2.proposalId, { reviewerName: 'Reviewer 1', score: 0.6 })
    ];

    await submitRatings(ratings);

    // Ratings submitted successfully (no error)
    expect(true).toBe(true);
  });

  it('should handle multiple reviewers rating same proposals', async () => {
    // Reviewer 1 rates both
    await submitRatings([
      createRating(proposal1.proposalId, { reviewerName: 'Reviewer 1', score: 0.8 }),
      createRating(proposal2.proposalId, { reviewerName: 'Reviewer 1', score: 0.6 })
    ]);

    // Reviewer 2 rates both
    await submitRatings([
      createRating(proposal1.proposalId, { reviewerName: 'Reviewer 2', score: 0.9 }),
      createRating(proposal2.proposalId, { reviewerName: 'Reviewer 2', score: 0.7 })
    ]);

    // No error indicates success
    expect(true).toBe(true);
  });
});

describe('PeerReviewManager - Decision Making', () => {
  let roomId: UUID;
  let messageId: UUID;

  beforeEach(() => {
    roomId = generateUUID();
    messageId = generateUUID();
  });

  it('should aggregate ratings and make decisions', async () => {
    // Declare proposals
    const proposal1 = await declareProposal(createProposalInput(roomId, messageId, { proposerName: 'AI 1' }));
    const proposal2 = await declareProposal(createProposalInput(roomId, messageId, { proposerName: 'AI 2' }));

    // Submit ratings (proposal1 gets high scores, proposal2 gets low)
    await submitRatings([
      createRating(proposal1.proposalId, { score: 0.9, shouldPost: true, reviewerWeight: 1.0 }),
      createRating(proposal2.proposalId, { score: 0.3, shouldPost: false, reviewerWeight: 1.0 })
    ]);

    await submitRatings([
      createRating(proposal1.proposalId, { score: 0.85, shouldPost: true, reviewerWeight: 1.0 }),
      createRating(proposal2.proposalId, { score: 0.4, shouldPost: false, reviewerWeight: 1.0 })
    ]);

    // Make decisions
    const decisions = await makeDecisions(roomId, messageId);

    expect(decisions).toHaveLength(2);

    // Proposal 1 should be approved
    const decision1 = decisions.find(d => d.proposalId === proposal1.proposalId)!;
    expect(decision1.shouldPost).toBe(true);
    expect(decision1.weightedAvgScore).toBeGreaterThan(0.8);

    // Proposal 2 should be rejected
    const decision2 = decisions.find(d => d.proposalId === proposal2.proposalId)!;
    expect(decision2.shouldPost).toBe(false);
    expect(decision2.weightedAvgScore).toBeLessThan(0.6);
  });

  it('should handle unanimous approval', async () => {
    const proposal = await declareProposal(createProposalInput(roomId, messageId));

    await submitRatings([
      createRating(proposal.proposalId, { score: 0.9, shouldPost: true, reviewerWeight: 1.0 }),
      createRating(proposal.proposalId, { score: 0.95, shouldPost: true, reviewerWeight: 1.0 }),
      createRating(proposal.proposalId, { score: 0.85, shouldPost: true, reviewerWeight: 1.0 })
    ]);

    const decisions = await makeDecisions(roomId, messageId);

    expect(decisions[0].shouldPost).toBe(true);
    expect(decisions[0].postVotePercentage).toBe(1.0);
  });

  it('should handle unanimous rejection', async () => {
    const proposal = await declareProposal(createProposalInput(roomId, messageId));

    await submitRatings([
      createRating(proposal.proposalId, { score: 0.3, shouldPost: false, reviewerWeight: 1.0 }),
      createRating(proposal.proposalId, { score: 0.2, shouldPost: false, reviewerWeight: 1.0 }),
      createRating(proposal.proposalId, { score: 0.4, shouldPost: false, reviewerWeight: 1.0 })
    ]);

    const decisions = await makeDecisions(roomId, messageId);

    expect(decisions[0].shouldPost).toBe(false);
    expect(decisions[0].postVotePercentage).toBe(0);
  });

  it('should handle proposals with no ratings', async () => {
    await declareProposal(createProposalInput(roomId, messageId));

    const decisions = await makeDecisions(roomId, messageId);

    expect(decisions[0].shouldPost).toBe(false); // No ratings → reject
    expect(decisions[0].weightedAvgScore).toBe(0);
  });

  it('should respect custom thresholds', async () => {
    const proposal = await declareProposal(createProposalInput(roomId, messageId));

    // Marginal ratings (60% score, 50% vote)
    await submitRatings([
      createRating(proposal.proposalId, { score: 0.6, shouldPost: true, reviewerWeight: 1.0 }),
      createRating(proposal.proposalId, { score: 0.6, shouldPost: false, reviewerWeight: 1.0 })
    ]);

    // Strict thresholds - should reject
    const strictThresholds: PeerReviewThresholds = {
      minPostVotePercentage: 0.75, // 75% vote required
      minWeightedScore: 0.7,         // 70% score required
      minReviewers: 2,
      reviewTimeoutMs: 2000
    };

    const strictDecisions = await makeDecisions(roomId, messageId, strictThresholds);
    expect(strictDecisions[0].shouldPost).toBe(false);

    // Lenient thresholds - should approve
    const lenientThresholds: PeerReviewThresholds = {
      minPostVotePercentage: 0.4, // 40% vote required
      minWeightedScore: 0.5,        // 50% score required
      minReviewers: 2,
      reviewTimeoutMs: 2000
    };

    const lenientDecisions = await makeDecisions(roomId, messageId, lenientThresholds);
    expect(lenientDecisions[0].shouldPost).toBe(true);
  });

  it('should mark session as complete with outcome stats', async () => {
    const proposal1 = await declareProposal(createProposalInput(roomId, messageId));
    const proposal2 = await declareProposal(createProposalInput(roomId, messageId));

    // Proposal 1 approved, proposal 2 rejected
    await submitRatings([
      createRating(proposal1.proposalId, { score: 0.9, shouldPost: true, reviewerWeight: 1.0 }),
      createRating(proposal2.proposalId, { score: 0.3, shouldPost: false, reviewerWeight: 1.0 })
    ]);

    await submitRatings([
      createRating(proposal1.proposalId, { score: 0.85, shouldPost: true, reviewerWeight: 1.0 }),
      createRating(proposal2.proposalId, { score: 0.4, shouldPost: false, reviewerWeight: 1.0 })
    ]);

    const decisions = await makeDecisions(roomId, messageId);

    // Verify outcomes
    const approvedCount = decisions.filter(d => d.shouldPost).length;
    const rejectedCount = decisions.filter(d => !d.shouldPost).length;

    expect(approvedCount).toBe(1);
    expect(rejectedCount).toBe(1);
  });
});

describe('PeerReviewManager - Revelation Window', () => {
  it('should wait briefly with random jitter', async () => {
    const startTime = Date.now();

    await waitRevelationWindow({
      baseDelayMs: 100,
      jitterMs: 50,
      maxWaitMs: 200
    });

    const elapsed = Date.now() - startTime;

    // Should wait between baseDelay and baseDelay+jitter (capped at maxWait)
    expect(elapsed).toBeGreaterThanOrEqual(100);
    expect(elapsed).toBeLessThan(200);
  });

  it('should respect max wait cap', async () => {
    const startTime = Date.now();

    await waitRevelationWindow({
      baseDelayMs: 500,
      jitterMs: 1000, // Would be 500-1500ms without cap
      maxWaitMs: 600  // Cap at 600ms
    });

    const elapsed = Date.now() - startTime;

    // Should never exceed maxWait
    expect(elapsed).toBeLessThan(650); // Small buffer for execution time
  });
});

describe('PeerReviewManager - Real-World Scenario from README', () => {
  it('should replicate quantum entanglement example', async () => {
    const roomId = generateUUID();
    const messageId = generateUUID();

    // 3 AIs propose responses
    const helperProposal = await declareProposal(
      createProposalInput(roomId, messageId, {
        proposerName: 'Helper AI',
        proposerModelProvider: 'groq',
        proposerModelId: 'llama-3.1-8b-instant'
      })
    );

    const teacherProposal = await declareProposal(
      createProposalInput(roomId, messageId, {
        proposerName: 'Teacher AI',
        proposerModelProvider: 'openai',
        proposerModelId: 'gpt-4'
      })
    );

    const physicistProposal = await declareProposal(
      createProposalInput(roomId, messageId, {
        proposerName: 'Physicist AI',
        proposerModelProvider: 'anthropic',
        proposerModelId: 'claude-sonnet-4-5-20250929'
      })
    );

    // Helper AI rates all (weight=0.5)
    await submitRatings([
      createRating(helperProposal.proposalId, { score: 0.7, shouldPost: true, reviewerWeight: 0.5 }),
      createRating(teacherProposal.proposalId, { score: 0.85, shouldPost: true, reviewerWeight: 0.5 }),
      createRating(physicistProposal.proposalId, { score: 0.9, shouldPost: true, reviewerWeight: 0.5 })
    ]);

    // Teacher AI rates all (weight=1.0)
    await submitRatings([
      createRating(helperProposal.proposalId, { score: 0.6, shouldPost: false, reviewerWeight: 1.0 }),
      createRating(teacherProposal.proposalId, { score: 0.8, shouldPost: true, reviewerWeight: 1.0 }),
      createRating(physicistProposal.proposalId, { score: 0.75, shouldPost: true, reviewerWeight: 1.0 })
    ]);

    // Physicist AI rates all (weight=1.0)
    await submitRatings([
      createRating(helperProposal.proposalId, { score: 0.5, shouldPost: false, reviewerWeight: 1.0 }),
      createRating(teacherProposal.proposalId, { score: 0.7, shouldPost: true, reviewerWeight: 1.0 }),
      createRating(physicistProposal.proposalId, { score: 0.95, shouldPost: true, reviewerWeight: 1.0 })
    ]);

    // Make decisions
    const decisions = await makeDecisions(roomId, messageId);

    // Helper AI should be rejected
    const helperDecision = decisions.find(d => d.proposalId === helperProposal.proposalId)!;
    expect(helperDecision.shouldPost).toBe(false);
    expect(helperDecision.weightedAvgScore).toBeCloseTo(0.58, 1);

    // Teacher AI should be approved
    const teacherDecision = decisions.find(d => d.proposalId === teacherProposal.proposalId)!;
    expect(teacherDecision.shouldPost).toBe(true);
    expect(teacherDecision.weightedAvgScore).toBeCloseTo(0.77, 1);

    // Physicist AI should be approved
    const physicistDecision = decisions.find(d => d.proposalId === physicistProposal.proposalId)!;
    expect(physicistDecision.shouldPost).toBe(true);
    expect(physicistDecision.weightedAvgScore).toBeCloseTo(0.86, 1);
  });
});

// Helper functions for creating test data

function createProposalInput(
  roomId: UUID,
  messageId: UUID,
  overrides: Partial<Omit<ResponseProposal, 'proposalId' | 'declaredAt'>> = {}
): Omit<ResponseProposal, 'proposalId' | 'declaredAt'> {
  return {
    roomId,
    respondingToId: messageId,
    proposerId: generateUUID(),
    proposerName: 'Test AI',
    proposerModelProvider: 'openai',
    proposerModelId: 'gpt-4',
    responseText: 'This is a test response',
    confidence: 0.8,
    inferenceDuration: 3000,
    currentContext: {
      newMessagesSinceInference: 0,
      otherActiveProposals: 0
    },
    ...overrides
  };
}

function createRating(
  proposalId: UUID,
  overrides: Partial<ProposalRating> = {}
): ProposalRating {
  return {
    ratingId: generateUUID(),
    proposalId,
    reviewerId: generateUUID(),
    reviewerName: 'Test Reviewer',
    reviewerWeight: 1.0,
    score: 0.5,
    shouldPost: false,
    ratedAt: Date.now(),
    reasoning: 'Test reasoning',
    ...overrides
  };
}
