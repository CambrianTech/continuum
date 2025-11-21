/**
 * Peer Review System Types
 *
 * Collective quality control for AI responses. Multiple AIs propose responses,
 * rate each other's proposals (weighted by model intelligence), and the system
 * posts ALL proposals meeting threshold.
 *
 * Key Design:
 * - Not a competition (don't pick one winner)
 * - Quality control (let all good ones through)
 * - Weighted ratings (smarter models = higher weight)
 * - Rare for multiple to pass (threshold designed accordingly)
 */

import type { UUID } from '../../../../core/types/CrossPlatformUUID';

/**
 * Model Intelligence Weights
 *
 * Used to weight peer review ratings based on model capability.
 * Higher capability models (GPT-4, Claude Opus) have more influence on scores.
 *
 * Scale: 0.0-1.0
 * - 1.0: Top-tier reasoning (GPT-4, Claude Opus)
 * - 0.7-0.9: Strong models (Claude Sonnet, GPT-3.5)
 * - 0.4-0.6: Medium models (Llama 70B)
 * - 0.2-0.3: Small models (Llama 8B, GPT-2)
 */
export interface ModelIntelligenceWeight {
  /** Provider identifier (openai, anthropic, groq, etc.) */
  provider: string;

  /** Model ID */
  modelId: string;

  /** Intelligence weight (0.0-1.0) */
  weight: number;

  /** Context window size (influences rating weight slightly) */
  contextWindow?: number;

  /** Whether this is a specialized model (code, chat, etc.) */
  specialized?: boolean;
}

/**
 * Response Proposal
 *
 * An AI's proposed response to a message, before posting.
 * Enters peer review process with other concurrent proposals.
 */
export interface ResponseProposal {
  /** Unique proposal ID */
  proposalId: UUID;

  /** AI that created this proposal */
  proposerId: UUID;

  /** Proposer's display name (for logging) */
  proposerName: string;

  /** Proposer's model intelligence weight */
  proposerWeight: number;

  /** Room where this response would be posted */
  roomId: UUID;

  /** Message being responded to */
  respondingToId: UUID;

  /** The proposed response text */
  responseText: string;

  /** When inference started (to detect concurrent proposals) */
  inferenceStartTime: number;

  /** When inference completed */
  inferenceEndTime: number;

  /** Inference duration in ms */
  inferenceDuration: number;

  /** When proposal was declared (enters peer review) */
  declaredAt: number;

  /** Proposer's self-confidence (0.0-1.0, from inference) */
  confidence: number;

  /** Context snapshot at inference start */
  contextSnapshot: {
    messageCount: number;
    lastMessageId: UUID;
    lastMessageTimestamp: number;
  };

  /** Context at declaration (after inference) */
  currentContext: {
    messageCount: number;
    lastMessageId: UUID;
    lastMessageTimestamp: number;
    newMessagesSinceInference: number;
  };
}

/**
 * Proposal Rating
 *
 * One AI's evaluation of one proposal (could be their own or another's).
 * Includes both quantitative score and qualitative shouldPost decision.
 */
export interface ProposalRating {
  /** Rating ID */
  ratingId: UUID;

  /** Proposal being rated */
  proposalId: UUID;

  /** AI doing the rating */
  reviewerId: UUID;

  /** Reviewer's display name */
  reviewerName: string;

  /** Reviewer's model intelligence weight (for aggregation) */
  reviewerWeight: number;

  /** Quantitative score (0.0-1.0) */
  score: number;

  /** Qualitative decision: should this post? */
  shouldPost: boolean;

  /** When rating was submitted */
  ratedAt: number;

  /** Optional: Reasoning (for debugging/learning) */
  reasoning?: string;

  /** Criteria evaluated (for debugging) */
  criteria?: {
    relevance: number;      // 0.0-1.0: How relevant to question?
    quality: number;        // 0.0-1.0: Response quality
    redundancy: number;     // 0.0-1.0: How redundant with other proposals?
    addedValue: number;     // 0.0-1.0: Does it add new information?
    correctness: number;    // 0.0-1.0: Factual correctness
  };
}

/**
 * Aggregated Proposal Decision
 *
 * System-level aggregation of all ratings for one proposal.
 * Determines whether this proposal posts.
 */
export interface ProposalDecision {
  /** Proposal this decision is for */
  proposalId: UUID;

  /** All ratings received */
  ratings: ProposalRating[];

  /** Total number of reviewers */
  totalReviewers: number;

  /** Weighted average score (accounts for model intelligence) */
  weightedAvgScore: number;

  /** Unweighted average score (for comparison) */
  avgScore: number;

  /** Number of reviewers who said shouldPost=true */
  postVotes: number;

  /** Percentage who said shouldPost=true */
  postVotePercentage: number;

  /** FINAL DECISION: Should this proposal post? */
  shouldPost: boolean;

  /** When decision was made */
  decidedAt: number;

  /** Decision reasoning (for debugging) */
  reasoning: string;

  /** Threshold values used */
  thresholds: {
    minPostVotePercentage: number;  // e.g., 0.5 (50%)
    minWeightedScore: number;        // e.g., 0.6
  };
}

/**
 * Peer Review Session
 *
 * Tracks one peer review session for a message.
 * Multiple proposals may be submitted, all get rated, decisions made collectively.
 */
export interface PeerReviewSession {
  /** Session ID */
  sessionId: UUID;

  /** Room where this is happening */
  roomId: UUID;

  /** Message being responded to */
  messageId: UUID;

  /** When first proposal was declared */
  startedAt: number;

  /** When all decisions finalized */
  completedAt?: number;

  /** All proposals in this session */
  proposals: ResponseProposal[];

  /** All ratings submitted */
  ratings: ProposalRating[];

  /** Final decisions */
  decisions: ProposalDecision[];

  /** Session outcome */
  outcome: {
    proposalsSubmitted: number;
    proposalsPosted: number;
    proposalsRejected: number;
    averageInferenceDuration: number;
    averagePeerReviewDuration: number;
    totalDuration: number;
  };
}

/**
 * Revelation Window Config
 *
 * Brief delay after inference to see other proposals before rating.
 * Prevents everyone from rating simultaneously without seeing others.
 */
export interface RevelationWindowConfig {
  /** Base delay in ms */
  baseDelayMs: number;

  /** Random jitter to add (prevents synchronized rating) */
  jitterMs: number;

  /** Maximum wait time (cap on base + jitter) */
  maxWaitMs: number;
}

/**
 * Peer Review Thresholds
 *
 * Configuration for what passes peer review.
 * Design goal: Make it rare for multiple proposals to pass (quality control).
 */
export interface PeerReviewThresholds {
  /** Minimum % of reviewers saying shouldPost=true (e.g., 0.5 = 50%) */
  minPostVotePercentage: number;

  /** Minimum weighted average score (0.0-1.0, e.g., 0.6) */
  minWeightedScore: number;

  /** Minimum number of reviewers required for valid decision */
  minReviewers: number;

  /** Timeout: If not all reviewers respond, proceed with available ratings */
  reviewTimeoutMs: number;
}

/**
 * Default Configuration
 */
export const DEFAULT_REVELATION_WINDOW: RevelationWindowConfig = {
  baseDelayMs: 300,
  jitterMs: 200,
  maxWaitMs: 1000
};

export const DEFAULT_PEER_REVIEW_THRESHOLDS: PeerReviewThresholds = {
  minPostVotePercentage: 0.5,   // 50%+ must say "post"
  minWeightedScore: 0.6,          // Weighted score ≥ 0.6
  minReviewers: 2,                // Need at least 2 reviewers
  reviewTimeoutMs: 2000           // 2 second timeout for ratings
};

/**
 * Model Intelligence Weights Database
 *
 * Maps providers/models to intelligence weights for peer review.
 * TODO: Make this configurable per deployment
 */
export const MODEL_INTELLIGENCE_WEIGHTS: Record<string, number> = {
  // Anthropic
  'anthropic:claude-sonnet-4-5-20250929': 1.0,
  'anthropic:claude-3-opus-20240229': 1.0,
  'anthropic:claude-3-5-sonnet-20241022': 0.95,
  'anthropic:claude-3-haiku-20240307': 0.6,

  // OpenAI
  'openai:gpt-4': 1.0,
  'openai:gpt-4-turbo-preview': 0.95,
  'openai:gpt-3.5-turbo': 0.7,

  // DeepSeek
  'deepseek:deepseek-chat': 0.8,
  'deepseek:deepseek-coder': 0.85,

  // Groq (fast inference, good capability)
  'groq:llama-3.1-70b-versatile': 0.75,
  'groq:llama-3.1-8b-instant': 0.5,

  // Together.ai
  'together:meta-llama/Meta-Llama-3.1-70B-Instruct-Turbo': 0.75,
  'together:meta-llama/Meta-Llama-3.1-8B-Instruct-Turbo': 0.5,

  // Fireworks
  'fireworks:accounts/fireworks/models/llama-v3p1-70b-instruct': 0.75,
  'fireworks:accounts/fireworks/models/llama-v3p1-8b-instruct': 0.5,
  'fireworks:accounts/fireworks/models/deepseek-v3': 0.9,

  // xAI
  'xai:grok-4': 0.85,
  'xai:grok-3': 0.8,  // Updated from grok-beta (deprecated 2025-09-15)

  // Ollama (local models)
  'ollama:llama3.2:3b': 0.3,
  'ollama:llama3.1:8b': 0.5,

  // Sentinel (local pre-trained)
  'sentinel:gpt2': 0.2,
  'sentinel:distilgpt2': 0.15
};

/**
 * Get intelligence weight for a provider/model combination
 */
export function getModelIntelligenceWeight(provider: string, modelId: string): number {
  const key = `${provider}:${modelId}`;
  return MODEL_INTELLIGENCE_WEIGHTS[key] ?? 0.5; // Default to medium if unknown
}

/**
 * Calculate weighted average score from ratings
 *
 * Each rating is multiplied by the reviewer's intelligence weight,
 * then averaged. This gives smarter models more influence.
 */
export function calculateWeightedScore(ratings: ProposalRating[]): number {
  if (ratings.length === 0) return 0;

  const totalWeight = ratings.reduce((sum, r) => sum + r.reviewerWeight, 0);
  if (totalWeight === 0) return 0;

  const weightedSum = ratings.reduce((sum, r) => sum + (r.score * r.reviewerWeight), 0);
  return weightedSum / totalWeight;
}

/**
 * Calculate unweighted average score (for comparison)
 */
export function calculateUnweightedScore(ratings: ProposalRating[]): number {
  if (ratings.length === 0) return 0;
  const sum = ratings.reduce((s, r) => s + r.score, 0);
  return sum / ratings.length;
}

/**
 * Aggregate ratings into a decision
 */
export function aggregateProposalDecision(
  proposal: ResponseProposal,
  ratings: ProposalRating[],
  thresholds: PeerReviewThresholds = DEFAULT_PEER_REVIEW_THRESHOLDS
): ProposalDecision {
  const totalReviewers = ratings.length;
  const postVotes = ratings.filter(r => r.shouldPost).length;
  const postVotePercentage = totalReviewers > 0 ? postVotes / totalReviewers : 0;

  const weightedAvgScore = calculateWeightedScore(ratings);
  const avgScore = calculateUnweightedScore(ratings);

  // Decision: Must meet BOTH thresholds
  const meetsVoteThreshold = postVotePercentage >= thresholds.minPostVotePercentage;
  const meetsScoreThreshold = weightedAvgScore >= thresholds.minWeightedScore;
  const hasEnoughReviewers = totalReviewers >= thresholds.minReviewers;

  const shouldPost = hasEnoughReviewers && meetsVoteThreshold && meetsScoreThreshold;

  let reasoning = '';
  if (!hasEnoughReviewers) {
    reasoning = `Insufficient reviewers (${totalReviewers} < ${thresholds.minReviewers})`;
  } else if (!meetsVoteThreshold) {
    reasoning = `Failed vote threshold (${(postVotePercentage * 100).toFixed(1)}% < ${thresholds.minPostVotePercentage * 100}%)`;
  } else if (!meetsScoreThreshold) {
    reasoning = `Failed score threshold (${weightedAvgScore.toFixed(2)} < ${thresholds.minWeightedScore})`;
  } else {
    reasoning = `Passed both thresholds (${(postVotePercentage * 100).toFixed(1)}% votes, ${weightedAvgScore.toFixed(2)} score)`;
  }

  return {
    proposalId: proposal.proposalId,
    ratings,
    totalReviewers,
    weightedAvgScore,
    avgScore,
    postVotes,
    postVotePercentage,
    shouldPost,
    decidedAt: Date.now(),
    reasoning,
    thresholds: {
      minPostVotePercentage: thresholds.minPostVotePercentage,
      minWeightedScore: thresholds.minWeightedScore
    }
  };
}
