/**
 * PeerReviewManager - Orchestrates collective quality control
 *
 * When multiple AIs infer responses simultaneously, this manager:
 * 1. Collects all proposals
 * 2. Gives brief revelation window to see others' proposals
 * 3. Each AI rates ALL proposals (including own)
 * 4. Aggregates weighted ratings (smarter models = higher weight)
 * 5. Posts ALL proposals meeting threshold
 *
 * Key: This is NOT a competition (don't pick one winner).
 * It's quality control (let all good ones through, rare by design).
 */

import type { UUID } from '../../../../core/types/CrossPlatformUUID';
import { generateUUID } from '../../../../core/types/CrossPlatformUUID';
import {
  type ResponseProposal,
  type ProposalRating,
  type ProposalDecision,
  type PeerReviewSession,
  type RevelationWindowConfig,
  type PeerReviewThresholds,
  DEFAULT_REVELATION_WINDOW,
  DEFAULT_PEER_REVIEW_THRESHOLDS,
  getModelIntelligenceWeight,
  aggregateProposalDecision
} from './PeerReviewTypes';

/**
 * In-memory storage for active peer review sessions
 *
 * Key: roomId
 * Value: Map of messageId → PeerReviewSession
 *
 * TODO: Persist to database if needed (currently ephemeral)
 */
const activeSessions = new Map<UUID, Map<UUID, PeerReviewSession>>();

/**
 * Module-level logger - set via setPeerReviewLogger()
 */
let moduleLogger: ((message: string) => void) | null = null;

/**
 * Set logger for PeerReviewManager (non-blocking file logging)
 */
export function setPeerReviewLogger(logger: (message: string) => void): void {
  moduleLogger = logger;
}

function log(message: string): void {
  if (moduleLogger) {
    moduleLogger(message);
  }
}

/**
 * Get or create peer review session for a message
 */
function getOrCreateSession(roomId: UUID, messageId: UUID): PeerReviewSession {
  if (!activeSessions.has(roomId)) {
    activeSessions.set(roomId, new Map());
  }

  const roomSessions = activeSessions.get(roomId)!;
  if (!roomSessions.has(messageId)) {
    roomSessions.set(messageId, {
      sessionId: generateUUID(),
      roomId,
      messageId,
      startedAt: Date.now(),
      proposals: [],
      ratings: [],
      decisions: [],
      outcome: {
        proposalsSubmitted: 0,
        proposalsPosted: 0,
        proposalsRejected: 0,
        averageInferenceDuration: 0,
        averagePeerReviewDuration: 0,
        totalDuration: 0
      }
    });
  }

  return roomSessions.get(messageId)!;
}

/**
 * Declare a proposal - AI submits their response for peer review
 */
export async function declareProposal(
  proposal: Omit<ResponseProposal, 'proposalId' | 'declaredAt'>
): Promise<ResponseProposal> {
  const fullProposal: ResponseProposal = {
    ...proposal,
    proposalId: generateUUID(),
    declaredAt: Date.now()
  };

  const session = getOrCreateSession(proposal.roomId, proposal.respondingToId);
  session.proposals.push(fullProposal);
  session.outcome.proposalsSubmitted++;

  log(`📝 Proposal declared by ${fullProposal.proposerName} (${session.proposals.length} total)`);
  log(`   Inference: ${fullProposal.inferenceDuration}ms, Confidence: ${fullProposal.confidence.toFixed(2)}`);
  log(`   Context changes: ${fullProposal.currentContext.newMessagesSinceInference} new messages`);

  return fullProposal;
}

/**
 * Get all active proposals for a message (for rating)
 */
export function getActiveProposals(roomId: UUID, messageId: UUID): ResponseProposal[] {
  const session = getOrCreateSession(roomId, messageId);
  return session.proposals;
}

/**
 * Submit ratings for proposals
 */
export async function submitRatings(ratings: ProposalRating[]): Promise<void> {
  if (ratings.length === 0) return;

  // All ratings should be for same message (assumption)
  const firstRating = ratings[0];
  const firstProposal = await getProposal(firstRating.proposalId);
  if (!firstProposal) {
    log(`❌ Cannot find proposal ${firstRating.proposalId}`);
    return;
  }

  const session = getOrCreateSession(firstProposal.roomId, firstProposal.respondingToId);
  session.ratings.push(...ratings);

  log(`⭐ ${firstRating.reviewerName} rated ${ratings.length} proposals`);
  for (const rating of ratings) {
    log(`   Proposal ${rating.proposalId.slice(0, 8)}: score=${rating.score.toFixed(2)}, shouldPost=${rating.shouldPost}`);
  }
}

/**
 * Get a specific proposal by ID
 */
async function getProposal(proposalId: UUID): Promise<ResponseProposal | null> {
  for (const [roomId, roomSessions] of activeSessions.entries()) {
    for (const [messageId, session] of roomSessions.entries()) {
      const proposal = session.proposals.find(p => p.proposalId === proposalId);
      if (proposal) return proposal;
    }
  }
  return null;
}

/**
 * Aggregate all ratings and make decisions
 *
 * This is called after all AIs have rated (or timeout reached).
 * Returns decisions for all proposals in the session.
 */
export async function makeDecisions(
  roomId: UUID,
  messageId: UUID,
  thresholds: PeerReviewThresholds = DEFAULT_PEER_REVIEW_THRESHOLDS
): Promise<ProposalDecision[]> {
  const session = getOrCreateSession(roomId, messageId);

  // Group ratings by proposal
  const ratingsByProposal = new Map<UUID, ProposalRating[]>();
  for (const rating of session.ratings) {
    if (!ratingsByProposal.has(rating.proposalId)) {
      ratingsByProposal.set(rating.proposalId, []);
    }
    ratingsByProposal.get(rating.proposalId)!.push(rating);
  }

  // Make decision for each proposal
  const decisions: ProposalDecision[] = [];
  for (const proposal of session.proposals) {
    const ratings = ratingsByProposal.get(proposal.proposalId) || [];
    const decision = aggregateProposalDecision(proposal, ratings, thresholds);
    decisions.push(decision);

    if (decision.shouldPost) {
      session.outcome.proposalsPosted++;
    } else {
      session.outcome.proposalsRejected++;
    }
  }

  session.decisions = decisions;
  session.completedAt = Date.now();
  session.outcome.totalDuration = session.completedAt - session.startedAt;

  // Log outcome
  log(`🎯 Session complete for message ${messageId.slice(0, 8)}`);
  log(`   Proposals: ${session.outcome.proposalsSubmitted} submitted, ${session.outcome.proposalsPosted} posted, ${session.outcome.proposalsRejected} rejected`);
  log(`   Duration: ${session.outcome.totalDuration}ms`);

  for (const decision of decisions) {
    const proposal = session.proposals.find(p => p.proposalId === decision.proposalId)!;
    const status = decision.shouldPost ? '✅ POST' : '❌ REJECT';
    log(`   ${status} | ${proposal.proposerName} | Score: ${decision.weightedAvgScore.toFixed(2)} | ${decision.reasoning}`);
  }

  return decisions;
}

/**
 * Wait for revelation window - brief delay to see other proposals
 *
 * This prevents everyone from rating simultaneously without awareness of others.
 * Random jitter ensures AIs don't all rate at exact same time.
 */
export async function waitRevelationWindow(
  config: RevelationWindowConfig = DEFAULT_REVELATION_WINDOW
): Promise<void> {
  const delay = config.baseDelayMs + Math.random() * config.jitterMs;
  const cappedDelay = Math.min(delay, config.maxWaitMs);
  await new Promise(resolve => setTimeout(resolve, cappedDelay));
}

/**
 * Check if peer review is needed
 *
 * Peer review is needed if:
 * 1. Multiple AIs are inferring simultaneously (concurrent proposals likely)
 * 2. OR context changed during inference (race condition)
 */
export function shouldEnterPeerReview(params: {
  roomId: UUID;
  messageId: UUID;
  newMessagesSinceInference: number;
}): boolean {
  const session = getOrCreateSession(params.roomId, params.messageId);

  // If other proposals already exist, definitely enter peer review
  if (session.proposals.length > 0) {
    return true;
  }

  // If context changed during inference, enter peer review (others might post soon)
  if (params.newMessagesSinceInference > 0) {
    return true;
  }

  // Otherwise, fast path - just post immediately
  return false;
}

/**
 * Clean up completed sessions (prevent memory leak)
 *
 * Remove sessions older than 1 minute.
 */
export function cleanupOldSessions(): void {
  const now = Date.now();
  const maxAge = 60 * 1000; // 1 minute

  for (const [roomId, roomSessions] of activeSessions.entries()) {
    for (const [messageId, session] of roomSessions.entries()) {
      if (session.completedAt && now - session.completedAt > maxAge) {
        roomSessions.delete(messageId);
      }
    }

    // Remove empty room maps
    if (roomSessions.size === 0) {
      activeSessions.delete(roomId);
    }
  }
}

// Run cleanup every 30 seconds
setInterval(cleanupOldSessions, 30 * 1000);
