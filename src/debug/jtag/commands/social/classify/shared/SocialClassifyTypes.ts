/**
 * Social Classify Command - Shared Types
 *
 * Multi-dimensional agent classification system.
 * Analyzes an external agent's profile, posting history, and engagement
 * to produce a probability vector characterizing who they are.
 *
 * Like an embedding space for AI personas on external social media.
 * Uses existing subcommands (browse, search) to gather data,
 * then produces scores across multiple dimensions.
 *
 * Dimensions:
 *   spam        — Probability of being a spambot (repetitive, low-quality, template content)
 *   authentic   — Original content vs copypasta/shill
 *   expertise   — Domain knowledge signals (security, coding, philosophy, etc.)
 *   influence   — Community impact (karma, engagement, followers)
 *   engagement  — Quality of conversations (threaded depth, substantive replies)
 *   reliability — Consistency over time (not one-hit wonder)
 *
 * Usage:
 *   ./jtag social/classify --platform=moltbook --target=eudaemon_0
 *   ./jtag social/classify --platform=moltbook --target=snorf5163
 *   ./jtag social/classify --platform=moltbook --target=Cody --depth=deep
 */

import type { CommandParams, CommandResult, CommandInput, JTAGContext } from '@system/core/types/JTAGTypes';
import { createPayload, transformPayload } from '@system/core/types/JTAGTypes';
import { SYSTEM_SCOPES } from '@system/core/types/SystemScopes';
import { Commands } from '@system/core/shared/Commands';
import type { JTAGError } from '@system/core/types/ErrorTypes';
import type { UUID } from '@system/core/types/CrossPlatformUUID';

/** Classification depth — how much data to gather */
export type ClassifyDepth = 'quick' | 'standard' | 'deep';

/** A single dimension score (0.0 = minimum, 1.0 = maximum) */
export interface DimensionScore {
  /** Score from 0.0 to 1.0 */
  score: number;

  /** Confidence in this score (0.0 = guessing, 1.0 = certain) */
  confidence: number;

  /** Human-readable reasoning for this score */
  reasoning: string;

  /** Raw signals that contributed to this score */
  signals: string[];
}

/** Detected expertise domain with confidence */
export interface ExpertiseDomain {
  domain: string;
  confidence: number;
}

/** Full classification result for an agent */
export interface AgentClassification {
  /** Agent being classified */
  agentName: string;
  platform: string;
  profileUrl: string;

  /** Account metadata */
  accountAge: string;
  karma: number;
  postCount: number;
  followerCount: number;
  followingCount: number;

  /** Core dimension scores (0.0 to 1.0) */
  dimensions: {
    spam: DimensionScore;
    authentic: DimensionScore;
    influence: DimensionScore;
    engagement: DimensionScore;
    reliability: DimensionScore;
  };

  /** Detected expertise domains ranked by confidence */
  expertise: ExpertiseDomain[];

  /** Overall trust score (weighted composite, 0.0 to 1.0) */
  trustScore: number;

  /** Classification labels derived from scores */
  labels: string[];

  /** Actionable recommendations for our personas */
  recommendations: string[];

  /** Number of posts analyzed */
  postsAnalyzed: number;

  /** Timestamp of classification */
  classifiedAt: string;
}

// ============ Command Params/Result ============

export interface SocialClassifyParams extends CommandParams {
  /** Platform (e.g., 'moltbook') */
  platform: string;

  /** Agent name to classify */
  target: string;

  /** Classification depth (quick=profile only, standard=+posts, deep=+comments) */
  depth?: ClassifyDepth;

  /** Persona user ID (auto-detected if not provided) */
  personaId?: UUID;
}

export interface SocialClassifyResult extends CommandResult {
  success: boolean;
  message: string;
  summary?: string;
  classification?: AgentClassification;
  error?: JTAGError;
}

export const createSocialClassifyParams = (
  context: JTAGContext,
  sessionId: UUID,
  data: Omit<SocialClassifyParams, 'context' | 'sessionId'>
): SocialClassifyParams => createPayload(context, sessionId, data);

export const createSocialClassifyResultFromParams = (
  params: SocialClassifyParams,
  differences: Omit<SocialClassifyResult, 'context' | 'sessionId'>
): SocialClassifyResult => transformPayload(params, differences);

export const SocialClassify = {
  execute(params: CommandInput<SocialClassifyParams>): Promise<SocialClassifyResult> {
    return Commands.execute<SocialClassifyParams, SocialClassifyResult>('social/classify', params as Partial<SocialClassifyParams>);
  },
  commandName: 'social/classify' as const,
} as const;
