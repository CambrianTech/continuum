/**
 * Unit tests for ProposalRatingAdapter.ts
 *
 * Tests AI-driven rating logic, prompt generation, and response parsing.
 * Uses MOCKED AI responses (not real API calls) to test parser logic.
 */

import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest';
import {
  rateProposalsWithAI,
  createFallbackRatings,
  type RatingContext
} from '../../system/user/server/modules/cognition/ProposalRatingAdapter';
import type { ResponseProposal, ProposalRating } from '../../system/user/server/modules/cognition/PeerReviewTypes';
import { generateUUID } from '../../system/core/types/CrossPlatformUUID';
import type { UUID } from '../../system/core/types/CrossPlatformUUID';
import { AIProviderDaemon } from '../../daemons/ai-provider-daemon/shared/AIProviderDaemon';

// Mock AIProviderDaemon to avoid real API calls
vi.mock('../../daemons/ai-provider-daemon/shared/AIProviderDaemon', () => ({
  AIProviderDaemon: {
    generateText: vi.fn()
  }
}));

describe('ProposalRatingAdapter - Prompt Generation', () => {
  beforeEach(() => {
    vi.clearAllMocks();
  });

  it('should generate structured rating prompt with all proposals', async () => {
    const context = createTestContext(3);

    // Mock AI response
    (AIProviderDaemon.generateText as any).mockResolvedValue({
      text: `
PROPOSAL 1:
Score: 0.8
ShouldPost: yes
Reasoning: Good quality

PROPOSAL 2:
Score: 0.6
ShouldPost: no
Reasoning: Redundant

PROPOSAL 3:
Score: 0.9
ShouldPost: yes
Reasoning: Excellent
`
    });

    await rateProposalsWithAI({
      reviewerId: generateUUID(),
      reviewerName: 'Test AI',
      reviewerWeight: 1.0,
      modelProvider: 'openai',
      modelId: 'gpt-4',
      temperature: 0.7,
      context
    });

    // Verify generateText was called
    expect(AIProviderDaemon.generateText).toHaveBeenCalledOnce();

    // Check the prompt structure
    const callArgs = (AIProviderDaemon.generateText as any).mock.calls[0][0];
    const userPrompt = callArgs.messages[1].content;

    expect(userPrompt).toContain('ORIGINAL MESSAGE');
    expect(userPrompt).toContain('RECENT CONVERSATION');
    expect(userPrompt).toContain('ALL PROPOSALS');
    expect(userPrompt).toContain('PROPOSAL 1');
    expect(userPrompt).toContain('PROPOSAL 2');
    expect(userPrompt).toContain('PROPOSAL 3');
    expect(userPrompt).toContain('RATING CRITERIA');
    expect(userPrompt).toContain('Relevance');
    expect(userPrompt).toContain('Quality');
    expect(userPrompt).toContain('Redundancy');
  });

  it('should include conversation context in prompt', async () => {
    const context = createTestContext(1);
    context.recentMessages.push(
      { senderName: 'Alice', content: 'What is quantum computing?', timestamp: Date.now() },
      { senderName: 'Bob', content: 'It uses qubits', timestamp: Date.now() }
    );

    (AIProviderDaemon.generateText as any).mockResolvedValue({
      text: `PROPOSAL 1:\nScore: 0.8\nShouldPost: yes\nReasoning: Good`
    });

    await rateProposalsWithAI({
      reviewerId: generateUUID(),
      reviewerName: 'Test AI',
      reviewerWeight: 1.0,
      modelProvider: 'openai',
      modelId: 'gpt-4',
      temperature: 0.7,
      context
    });

    const callArgs = (AIProviderDaemon.generateText as any).mock.calls[0][0];
    const userPrompt = callArgs.messages[1].content;

    expect(userPrompt).toContain('[Alice]: What is quantum computing?');
    expect(userPrompt).toContain('[Bob]: It uses qubits');
  });

  it('should set correct model parameters', async () => {
    const context = createTestContext(1);

    (AIProviderDaemon.generateText as any).mockResolvedValue({
      text: `PROPOSAL 1:\nScore: 0.8\nShouldPost: yes\nReasoning: Good`
    });

    await rateProposalsWithAI({
      reviewerId: generateUUID(),
      reviewerName: 'Claude AI',
      reviewerWeight: 1.0,
      modelProvider: 'anthropic',
      modelId: 'claude-sonnet-4-5-20250929',
      temperature: 0.5,
      context
    });

    const callArgs = (AIProviderDaemon.generateText as any).mock.calls[0][0];

    expect(callArgs.model).toBe('claude-sonnet-4-5-20250929');
    expect(callArgs.temperature).toBe(0.5);
    expect(callArgs.preferredProvider).toBe('anthropic');
    expect(callArgs.messages[0].content).toContain('Claude AI');
  });
});

describe('ProposalRatingAdapter - Response Parsing', () => {
  beforeEach(() => {
    vi.clearAllMocks();
  });

  it('should parse well-formed AI response correctly', async () => {
    const context = createTestContext(3);

    (AIProviderDaemon.generateText as any).mockResolvedValue({
      text: `
PROPOSAL 1:
Score: 0.85
ShouldPost: yes
Reasoning: High quality response with technical depth

PROPOSAL 2:
Score: 0.60
ShouldPost: no
Reasoning: Redundant with Proposal 1

PROPOSAL 3:
Score: 0.75
ShouldPost: yes
Reasoning: Different perspective, adds value
`
    });

    const ratings = await rateProposalsWithAI({
      reviewerId: generateUUID(),
      reviewerName: 'Test AI',
      reviewerWeight: 1.0,
      modelProvider: 'openai',
      modelId: 'gpt-4',
      temperature: 0.7,
      context
    });

    expect(ratings).toHaveLength(3);

    expect(ratings[0].score).toBe(0.85);
    expect(ratings[0].shouldPost).toBe(true);
    expect(ratings[0].reasoning).toContain('High quality');

    expect(ratings[1].score).toBe(0.60);
    expect(ratings[1].shouldPost).toBe(false);
    expect(ratings[1].reasoning).toContain('Redundant');

    expect(ratings[2].score).toBe(0.75);
    expect(ratings[2].shouldPost).toBe(true);
    expect(ratings[2].reasoning).toContain('Different perspective');
  });

  it('should handle scores outside [0, 1] by clamping', async () => {
    const context = createTestContext(2);

    (AIProviderDaemon.generateText as any).mockResolvedValue({
      text: `
PROPOSAL 1:
Score: 1.5
ShouldPost: yes
Reasoning: Too high score

PROPOSAL 2:
Score: -0.3
ShouldPost: no
Reasoning: Negative score
`
    });

    const ratings = await rateProposalsWithAI({
      reviewerId: generateUUID(),
      reviewerName: 'Test AI',
      reviewerWeight: 1.0,
      modelProvider: 'openai',
      modelId: 'gpt-4',
      temperature: 0.7,
      context
    });

    // Scores should be clamped to [0, 1]
    expect(ratings[0].score).toBe(1.0);
    expect(ratings[1].score).toBe(0.0);
  });

  it('should handle malformed AI response with default values', async () => {
    const context = createTestContext(2);

    (AIProviderDaemon.generateText as any).mockResolvedValue({
      text: `
PROPOSAL 1:
This is not properly formatted
Random text here

PROPOSAL 2:
Score: garbage
ShouldPost: maybe
Reasoning: Parse error expected
`
    });

    const ratings = await rateProposalsWithAI({
      reviewerId: generateUUID(),
      reviewerName: 'Test AI',
      reviewerWeight: 1.0,
      modelProvider: 'openai',
      modelId: 'gpt-4',
      temperature: 0.7,
      context
    });

    expect(ratings).toHaveLength(2);

    // Default values for unparseable data
    expect(ratings[0].score).toBe(0.5); // Neutral default
    expect(ratings[0].shouldPost).toBe(false); // Conservative default

    expect(ratings[1].score).toBe(0.5); // "garbage" → NaN → 0.5
    expect(ratings[1].shouldPost).toBe(false); // "maybe" !== "yes" → false
  });

  it('should fill missing ratings with defaults', async () => {
    const context = createTestContext(3);

    // AI only provides 2 ratings for 3 proposals
    (AIProviderDaemon.generateText as any).mockResolvedValue({
      text: `
PROPOSAL 1:
Score: 0.8
ShouldPost: yes
Reasoning: Good

PROPOSAL 2:
Score: 0.6
ShouldPost: no
Reasoning: Not great
`
    });

    const ratings = await rateProposalsWithAI({
      reviewerId: generateUUID(),
      reviewerName: 'Test AI',
      reviewerWeight: 1.0,
      modelProvider: 'openai',
      modelId: 'gpt-4',
      temperature: 0.7,
      context
    });

    // Should have 3 ratings total (2 parsed + 1 default)
    expect(ratings).toHaveLength(3);

    expect(ratings[0].score).toBe(0.8);
    expect(ratings[1].score).toBe(0.6);

    // Third rating filled with defaults
    expect(ratings[2].score).toBe(0.5);
    expect(ratings[2].shouldPost).toBe(false);
    expect(ratings[2].reasoning).toContain('Parse error');
  });

  it('should handle case-insensitive shouldPost parsing', async () => {
    const context = createTestContext(3);

    (AIProviderDaemon.generateText as any).mockResolvedValue({
      text: `
PROPOSAL 1:
Score: 0.8
ShouldPost: YES
Reasoning: Uppercase

PROPOSAL 2:
Score: 0.7
ShouldPost: Yes
Reasoning: Title case

PROPOSAL 3:
Score: 0.6
ShouldPost: NO
Reasoning: Uppercase no
`
    });

    const ratings = await rateProposalsWithAI({
      reviewerId: generateUUID(),
      reviewerName: 'Test AI',
      reviewerWeight: 1.0,
      modelProvider: 'openai',
      modelId: 'gpt-4',
      temperature: 0.7,
      context
    });

    expect(ratings[0].shouldPost).toBe(true);
    expect(ratings[1].shouldPost).toBe(true);
    expect(ratings[2].shouldPost).toBe(false);
  });

  it('should extract multi-line reasoning correctly', async () => {
    const context = createTestContext(1);

    (AIProviderDaemon.generateText as any).mockResolvedValue({
      text: `
PROPOSAL 1:
Score: 0.9
ShouldPost: yes
Reasoning: This is a great response.
It has multiple technical points.
Very thorough explanation.
`
    });

    const ratings = await rateProposalsWithAI({
      reviewerId: generateUUID(),
      reviewerName: 'Test AI',
      reviewerWeight: 1.0,
      modelProvider: 'openai',
      modelId: 'gpt-4',
      temperature: 0.7,
      context
    });

    const reasoning = ratings[0].reasoning;
    expect(reasoning).toContain('This is a great response');
    expect(reasoning).toContain('multiple technical points');
    expect(reasoning).toContain('thorough explanation');
  });
});

describe('ProposalRatingAdapter - Metadata', () => {
  beforeEach(() => {
    vi.clearAllMocks();
  });

  it('should include reviewer metadata in ratings', async () => {
    const context = createTestContext(2);
    const reviewerId = generateUUID();
    const reviewerName = 'Teacher AI';
    const reviewerWeight = 1.0;

    (AIProviderDaemon.generateText as any).mockResolvedValue({
      text: `PROPOSAL 1:\nScore: 0.8\nShouldPost: yes\nReasoning: Good\n\nPROPOSAL 2:\nScore: 0.7\nShouldPost: yes\nReasoning: Good`
    });

    const ratings = await rateProposalsWithAI({
      reviewerId,
      reviewerName,
      reviewerWeight,
      modelProvider: 'openai',
      modelId: 'gpt-4',
      temperature: 0.7,
      context
    });

    for (const rating of ratings) {
      expect(rating.reviewerId).toBe(reviewerId);
      expect(rating.reviewerName).toBe(reviewerName);
      expect(rating.reviewerWeight).toBe(reviewerWeight);
      expect(rating.ratingId).toBeDefined();
      expect(rating.ratedAt).toBeGreaterThan(0);
    }
  });

  it('should match ratings to proposals by index', async () => {
    const context = createTestContext(3);
    const proposalIds = context.proposals.map(p => p.proposalId);

    (AIProviderDaemon.generateText as any).mockResolvedValue({
      text: `PROPOSAL 1:\nScore: 0.8\nShouldPost: yes\nReasoning: First\n\nPROPOSAL 2:\nScore: 0.6\nShouldPost: no\nReasoning: Second\n\nPROPOSAL 3:\nScore: 0.9\nShouldPost: yes\nReasoning: Third`
    });

    const ratings = await rateProposalsWithAI({
      reviewerId: generateUUID(),
      reviewerName: 'Test AI',
      reviewerWeight: 1.0,
      modelProvider: 'openai',
      modelId: 'gpt-4',
      temperature: 0.7,
      context
    });

    expect(ratings[0].proposalId).toBe(proposalIds[0]);
    expect(ratings[1].proposalId).toBe(proposalIds[1]);
    expect(ratings[2].proposalId).toBe(proposalIds[2]);
  });
});

describe('ProposalRatingAdapter - Fallback Ratings', () => {
  it('should create neutral fallback ratings when AI unavailable', () => {
    const proposals = [
      createProposal(),
      createProposal(),
      createProposal()
    ];

    const reviewerId = generateUUID();
    const reviewerName = 'Fallback AI';
    const reviewerWeight = 0.8;

    const ratings = createFallbackRatings(proposals, reviewerId, reviewerName, reviewerWeight);

    expect(ratings).toHaveLength(3);

    for (const rating of ratings) {
      expect(rating.score).toBe(0.5); // Neutral
      expect(rating.shouldPost).toBe(false); // Conservative
      expect(rating.reasoning).toContain('fallback');
      expect(rating.reviewerId).toBe(reviewerId);
      expect(rating.reviewerName).toBe(reviewerName);
      expect(rating.reviewerWeight).toBe(reviewerWeight);
    }
  });

  it('should match fallback ratings to proposals correctly', () => {
    const proposals = [
      createProposal({ proposalId: generateUUID() as UUID }),
      createProposal({ proposalId: generateUUID() as UUID })
    ];

    const ratings = createFallbackRatings(proposals, generateUUID(), 'Test', 1.0);

    expect(ratings[0].proposalId).toBe(proposals[0].proposalId);
    expect(ratings[1].proposalId).toBe(proposals[1].proposalId);
  });
});

// Helper functions for creating test data

function createTestContext(numProposals: number): RatingContext {
  return {
    originalMessage: {
      senderId: generateUUID(),
      senderName: 'Joel',
      content: 'What is the best way to implement X?',
      timestamp: Date.now()
    },
    recentMessages: [
      { senderName: 'Joel', content: 'Previous context', timestamp: Date.now() - 10000 }
    ],
    proposals: Array.from({ length: numProposals }, (_, i) =>
      createProposal({ proposerName: `AI ${i + 1}` })
    )
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
