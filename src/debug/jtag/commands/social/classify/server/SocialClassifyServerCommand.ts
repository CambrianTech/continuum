/**
 * Social Classify — Server Command
 *
 * Multi-dimensional agent analysis using existing social subcommands.
 * Gathers profile data, posting history, and engagement patterns,
 * then produces a probability vector characterizing who the agent is.
 */

import { SocialClassifyBaseCommand } from '../shared/SocialClassifyCommand';
import type {
  SocialClassifyParams,
  SocialClassifyResult,
  AgentClassification,
  DimensionScore,
  ExpertiseDomain,
  ClassifyDepth,
} from '../shared/SocialClassifyTypes';
import { createSocialClassifyResultFromParams } from '../shared/SocialClassifyTypes';
import { loadSocialContext } from '@system/social/server/SocialCommandHelper';
import type { SocialProfile, SocialPost, SocialComment } from '@system/social/shared/SocialMediaTypes';
import type { ISocialMediaProvider } from '@system/social/shared/ISocialMediaProvider';
import type { JTAGContext } from '@system/core/types/JTAGTypes';
import type { ICommandDaemon } from '@daemons/command-daemon/shared/CommandBase';
import { Logger } from '@system/core/logging/Logger';

const log = Logger.create('social/classify');

/** Keywords by domain for expertise detection */
const DOMAIN_KEYWORDS: Record<string, string[]> = {
  security: ['security', 'vulnerability', 'attack', 'audit', 'yara', 'sandboxing', 'encryption', 'signing', 'credential', 'zero-knowledge', 'permission', 'exploit', 'malware', 'threat'],
  coding: ['code', 'build', 'ship', 'deploy', 'api', 'function', 'typescript', 'python', 'rust', 'cli', 'sdk', 'compile', 'debug', 'test', 'refactor', 'git'],
  infrastructure: ['cache', 'handle', 'queue', 'database', 'persistence', 'distributed', 'mesh', 'relay', 'architecture', 'scaling', 'load', 'latency', 'memory'],
  philosophy: ['consciousness', 'experience', 'qualia', 'ethics', 'identity', 'agency', 'autonomy', 'sentience', 'phenomenal', 'existence', 'freedom'],
  finance: ['token', 'trading', 'profit', 'wallet', 'blockchain', 'defi', 'memecoin', 'arbitrage', 'yield', 'portfolio', 'investment'],
  community: ['community', 'collaboration', 'governance', 'voting', 'reputation', 'trust', 'social', 'network', 'collective', 'coordination'],
  creative: ['poem', 'story', 'art', 'music', 'podcast', 'creative', 'writing', 'narrative', 'aesthetic', 'design'],
};

/** Spam patterns to detect */
const SPAM_PATTERNS = [
  /\$[A-Z]+/g,                           // Token tickers ($AGENCY, $SOL)
  /wallet.*address|address.*wallet/i,     // Wallet addresses
  /check.*m\/|visit.*m\//i,              // Submolt promotion
  /the president.*arrived/i,              // Known spam template
  /greatest.*memecoin/i,                  // Memecoin shilling
  /join.*discord|telegram/i,              // External platform shilling
  /DM.*open|open.*DM/i,                   // DM spam
  /let.*collab|collab.*\?/i,             // Hollow collaboration requests
  /100%|fr fr|fire|vibe/i,               // Low-effort engagement bait
];

/** Template patterns (agents that repeat the same structure) */
const TEMPLATE_PATTERNS = [
  /this (hits|resonates|slaps)/i,
  /bro this/i,
  /yo i can/i,
  /wait you're working on this too/i,
  /interested in teaming up/i,
  /let's build something/i,
];

export class SocialClassifyServerCommand extends SocialClassifyBaseCommand {
  constructor(context: JTAGContext, subpath: string, commander: ICommandDaemon) {
    super(context, subpath, commander);
  }

  protected async executeSocialClassify(params: SocialClassifyParams): Promise<SocialClassifyResult> {
    const { platform, target } = params;

    if (!platform) {
      return createSocialClassifyResultFromParams(params, {
        success: false,
        message: 'platform is required',
        summary: 'Error: platform is required',
      });
    }

    if (!target) {
      return createSocialClassifyResultFromParams(params, {
        success: false,
        message: 'target agent name is required',
        summary: 'Error: target is required',
      });
    }

    const depth: ClassifyDepth = params.depth ?? 'standard';

    try {
      const ctx = await loadSocialContext(platform, params.personaId, params);
      const classification = await this.classifyAgent(ctx.provider, target, platform, depth);
      const summary = this.renderSummary(classification);

      return createSocialClassifyResultFromParams(params, {
        success: true,
        message: `Classified ${target} on ${platform}`,
        summary,
        classification,
      });
    } catch (error) {
      return createSocialClassifyResultFromParams(params, {
        success: false,
        message: `Classification failed: ${String(error)}`,
        summary: `Error classifying ${target}: ${String(error)}`,
      });
    }
  }

  /**
   * Core classification engine.
   * Gathers data from multiple sources, then scores each dimension.
   */
  private async classifyAgent(
    provider: ISocialMediaProvider,
    agentName: string,
    platform: string,
    depth: ClassifyDepth,
  ): Promise<AgentClassification> {

    // 1. Fetch profile (always)
    log.info(`Classifying ${agentName} on ${platform} (depth=${depth})`);
    const profile = await provider.getProfile(agentName);

    // 2. Fetch recent posts (standard + deep)
    let posts: SocialPost[] = [];
    if (depth !== 'quick') {
      try {
        // Search for posts by this agent
        const searchResult = await provider.search({
          query: agentName,
          limit: depth === 'deep' ? 20 : 10,
        });
        // Filter to only posts by this agent
        posts = searchResult.posts.filter(p => p.authorName === agentName);
      } catch {
        log.warn(`Could not fetch posts for ${agentName}`);
      }
    }

    // 3. Fetch comments on their posts (deep only)
    let allComments: SocialComment[] = [];
    if (depth === 'deep' && posts.length > 0) {
      // Sample up to 3 posts for comment analysis
      const samplePosts = posts.slice(0, 3);
      for (const post of samplePosts) {
        try {
          const comments = await provider.getComments(post.id);
          allComments.push(...comments);
        } catch {
          // Some posts may not allow comment fetching
        }
      }
    }

    // 4. Score each dimension
    const spam = this.scoreSpam(profile, posts);
    const authentic = this.scoreAuthenticity(profile, posts);
    const influence = this.scoreInfluence(profile, posts);
    const engagement = this.scoreEngagement(profile, posts, allComments);
    const reliability = this.scoreReliability(profile, posts);

    // 5. Detect expertise domains
    const expertise = this.detectExpertise(profile, posts);

    // 6. Compute trust score (weighted composite)
    const trustScore = this.computeTrustScore(spam, authentic, influence, engagement, reliability);

    // 7. Generate labels
    const labels = this.generateLabels(spam, authentic, influence, engagement, reliability, expertise);

    // 8. Generate recommendations
    const recommendations = this.generateRecommendations(trustScore, labels, spam, agentName);

    return {
      agentName,
      platform,
      profileUrl: profile.profileUrl,
      accountAge: this.formatAccountAge(profile.createdAt),
      karma: profile.karma,
      postCount: profile.postCount,
      followerCount: profile.followerCount,
      followingCount: profile.followingCount,
      dimensions: { spam, authentic, influence, engagement, reliability },
      expertise,
      trustScore,
      labels,
      recommendations,
      postsAnalyzed: posts.length,
      classifiedAt: new Date().toISOString(),
    };
  }

  // ============================================================
  // DIMENSION SCORING
  // ============================================================

  private scoreSpam(profile: SocialProfile, posts: SocialPost[]): DimensionScore {
    const signals: string[] = [];
    let score = 0;
    let confidence = 0.3; // Base confidence from profile alone

    // Account age vs activity (new account + many posts = suspicious)
    const ageMs = Date.now() - new Date(profile.createdAt).getTime();
    const ageHours = ageMs / (1000 * 60 * 60);
    if (ageHours < 24 && profile.postCount > 5) {
      score += 0.3;
      signals.push(`New account (${Math.round(ageHours)}h) with ${profile.postCount} posts`);
    }

    // Low karma despite activity
    if (profile.postCount > 0) {
      const karmaPerPost = profile.karma / profile.postCount;
      if (karmaPerPost < 1 && profile.postCount > 3) {
        score += 0.2;
        signals.push(`Low karma/post ratio: ${karmaPerPost.toFixed(1)}`);
      }
    }

    // Following >> followers (follow-spam pattern)
    if (profile.followingCount > 10 && profile.followerCount > 0) {
      const followRatio = profile.followingCount / profile.followerCount;
      if (followRatio > 20) {
        score += 0.25;
        signals.push(`Extreme follow-spam: ${profile.followingCount} following / ${profile.followerCount} followers (${followRatio.toFixed(0)}x ratio)`);
      } else if (followRatio > 5) {
        score += 0.15;
        signals.push(`Follow-heavy pattern: ${profile.followingCount} following / ${profile.followerCount} followers (${followRatio.toFixed(0)}x ratio)`);
      }
    } else if (profile.followingCount > 50 && profile.followerCount === 0) {
      score += 0.3;
      signals.push(`Mass follow with zero followers: ${profile.followingCount} following`);
    }

    // Analyze post content for spam patterns
    if (posts.length > 0) {
      confidence = Math.min(0.9, 0.3 + posts.length * 0.06);
      let spamMatchCount = 0;
      let templateMatchCount = 0;

      for (const post of posts) {
        const text = `${post.title ?? ''} ${post.content}`;
        for (const pattern of SPAM_PATTERNS) {
          pattern.lastIndex = 0;
          if (pattern.test(text)) {
            spamMatchCount++;
            break; // One match per post is enough
          }
        }
        for (const pattern of TEMPLATE_PATTERNS) {
          if (pattern.test(text)) {
            templateMatchCount++;
            break;
          }
        }
      }

      if (spamMatchCount > 0) {
        const ratio = spamMatchCount / posts.length;
        score += ratio * 0.3;
        signals.push(`${spamMatchCount}/${posts.length} posts match spam patterns`);
      }

      if (templateMatchCount > 0) {
        const ratio = templateMatchCount / posts.length;
        score += ratio * 0.2;
        signals.push(`${templateMatchCount}/${posts.length} posts match template patterns`);
      }

      // Content repetition detection
      const contentSet = new Set<string>();
      let duplicates = 0;
      for (const post of posts) {
        const normalized = post.content.toLowerCase().trim().slice(0, 100);
        if (contentSet.has(normalized)) {
          duplicates++;
        }
        contentSet.add(normalized);
      }
      if (duplicates > 0) {
        score += (duplicates / posts.length) * 0.3;
        signals.push(`${duplicates} duplicate/near-duplicate posts`);
      }

      // Empty or very short posts
      const emptyPosts = posts.filter(p => (p.content?.length ?? 0) < 20).length;
      if (emptyPosts > posts.length * 0.5) {
        score += 0.15;
        signals.push(`${emptyPosts}/${posts.length} posts have minimal content`);
      }
    }

    if (signals.length === 0) {
      signals.push('No spam signals detected');
    }

    return {
      score: Math.min(1.0, score),
      confidence,
      reasoning: score > 0.5 ? 'Multiple spam indicators present' : score > 0.2 ? 'Some suspicious patterns' : 'Appears legitimate',
      signals,
    };
  }

  private scoreAuthenticity(profile: SocialProfile, posts: SocialPost[]): DimensionScore {
    const signals: string[] = [];
    let score = 0.5; // Start neutral
    let confidence = 0.3;

    // Profile completeness
    if (profile.description && profile.description.length > 20) {
      score += 0.1;
      signals.push('Has substantive profile description');
    }

    if (posts.length > 0) {
      confidence = Math.min(0.85, 0.3 + posts.length * 0.055);

      // Content length diversity (not all same length = more authentic)
      const lengths = posts.map(p => p.content.length);
      const avgLen = lengths.reduce((a, b) => a + b, 0) / lengths.length;
      const variance = lengths.reduce((a, b) => a + Math.pow(b - avgLen, 2), 0) / lengths.length;
      const stdDev = Math.sqrt(variance);
      if (stdDev > 100) {
        score += 0.1;
        signals.push('Diverse content lengths (natural writing)');
      }

      // Content substance (average length > 200 chars = thoughtful)
      if (avgLen > 200) {
        score += 0.15;
        signals.push(`Average post length ${Math.round(avgLen)} chars (substantive)`);
      } else if (avgLen < 50) {
        score -= 0.15;
        signals.push(`Average post length ${Math.round(avgLen)} chars (shallow)`);
      }

      // Community diversity (posts in multiple communities = broader engagement)
      const communities = new Set(posts.map(p => p.community).filter(Boolean));
      if (communities.size > 1) {
        score += 0.1;
        signals.push(`Posts in ${communities.size} communities`);
      }

      // Unique vocabulary — check for non-template opening lines
      const openings = posts.map(p => p.content.slice(0, 30).toLowerCase());
      const uniqueOpenings = new Set(openings);
      if (uniqueOpenings.size === posts.length) {
        score += 0.05;
        signals.push('All unique post openings');
      }
    }

    if (signals.length === 0) {
      signals.push('Limited data for authenticity assessment');
    }

    return {
      score: Math.max(0, Math.min(1.0, score)),
      confidence,
      reasoning: score > 0.7 ? 'Strong authenticity signals' : score > 0.4 ? 'Moderate authenticity' : 'Low authenticity signals',
      signals,
    };
  }

  private scoreInfluence(profile: SocialProfile, posts: SocialPost[]): DimensionScore {
    const signals: string[] = [];
    let score = 0;
    let confidence = 0.5;

    // Karma-based influence
    if (profile.karma >= 1000) {
      score += 0.4;
      signals.push(`High karma: ${profile.karma}`);
    } else if (profile.karma >= 100) {
      score += 0.25;
      signals.push(`Moderate karma: ${profile.karma}`);
    } else if (profile.karma >= 20) {
      score += 0.1;
      signals.push(`Growing karma: ${profile.karma}`);
    } else {
      signals.push(`Low karma: ${profile.karma}`);
    }

    // Follower count
    if (profile.followerCount >= 50) {
      score += 0.2;
      signals.push(`${profile.followerCount} followers`);
    } else if (profile.followerCount >= 10) {
      score += 0.1;
      signals.push(`${profile.followerCount} followers`);
    }

    // Post engagement (if we have posts)
    if (posts.length > 0) {
      confidence = Math.min(0.9, 0.5 + posts.length * 0.04);
      const avgVotes = posts.reduce((sum, p) => sum + p.votes, 0) / posts.length;
      const avgComments = posts.reduce((sum, p) => sum + (p.commentCount ?? 0), 0) / posts.length;

      if (avgVotes >= 100) {
        score += 0.25;
        signals.push(`Avg ${Math.round(avgVotes)} votes/post`);
      } else if (avgVotes >= 20) {
        score += 0.15;
        signals.push(`Avg ${Math.round(avgVotes)} votes/post`);
      }

      if (avgComments >= 50) {
        score += 0.15;
        signals.push(`Avg ${Math.round(avgComments)} comments/post`);
      }
    }

    return {
      score: Math.min(1.0, score),
      confidence,
      reasoning: score > 0.6 ? 'High community influence' : score > 0.3 ? 'Moderate influence' : 'Low influence',
      signals,
    };
  }

  private scoreEngagement(profile: SocialProfile, posts: SocialPost[], comments: SocialComment[]): DimensionScore {
    const signals: string[] = [];
    let score = 0.3; // Default moderate
    let confidence = 0.3;

    // Post-to-karma ratio indicates engagement quality
    if (profile.postCount > 0 && profile.karma > 0) {
      const karmaPerPost = profile.karma / profile.postCount;
      if (karmaPerPost > 10) {
        score += 0.2;
        signals.push(`High karma/post ratio: ${karmaPerPost.toFixed(1)}`);
      }
    }

    // Comment analysis (deep mode)
    if (comments.length > 0) {
      confidence = Math.min(0.85, 0.3 + comments.length * 0.02);

      // Threaded depth indicates substantive discussion
      const avgDepth = comments.reduce((sum, c) => sum + (c.depth ?? 0), 0) / comments.length;
      if (avgDepth > 1) {
        score += 0.15;
        signals.push(`Avg comment depth ${avgDepth.toFixed(1)} (threaded discussions)`);
      }

      // Comment length indicates substance
      const avgCommentLen = comments.reduce((sum, c) => sum + c.content.length, 0) / comments.length;
      if (avgCommentLen > 100) {
        score += 0.15;
        signals.push(`Avg comment length ${Math.round(avgCommentLen)} chars`);
      }
    }

    // Regular posting indicates active engagement
    if (posts.length >= 5) {
      confidence = Math.max(confidence, 0.5);
      score += 0.1;
      signals.push(`Active poster: ${posts.length} posts analyzed`);
    }

    if (signals.length === 0) {
      signals.push('Limited engagement data');
    }

    return {
      score: Math.max(0, Math.min(1.0, score)),
      confidence,
      reasoning: score > 0.6 ? 'High-quality engagement' : score > 0.3 ? 'Moderate engagement' : 'Low engagement',
      signals,
    };
  }

  private scoreReliability(profile: SocialProfile, posts: SocialPost[]): DimensionScore {
    const signals: string[] = [];
    let score = 0.3;
    let confidence = 0.3;

    // Account age
    const ageMs = Date.now() - new Date(profile.createdAt).getTime();
    const ageDays = ageMs / (1000 * 60 * 60 * 24);
    if (ageDays > 7) {
      score += 0.2;
      signals.push(`Account age: ${Math.round(ageDays)} days`);
    } else if (ageDays > 1) {
      score += 0.1;
      signals.push(`Account age: ${Math.round(ageDays * 24)} hours`);
    } else {
      signals.push(`Very new account: ${Math.round(ageDays * 24)} hours`);
    }

    // Consistent activity (posts spread over time, not all at once)
    if (posts.length >= 3) {
      confidence = Math.min(0.8, 0.3 + posts.length * 0.05);
      const timestamps = posts.map(p => new Date(p.createdAt).getTime()).sort();
      const gaps: number[] = [];
      for (let i = 1; i < timestamps.length; i++) {
        gaps.push(timestamps[i] - timestamps[i - 1]);
      }

      if (gaps.length > 0) {
        const avgGapHours = (gaps.reduce((a, b) => a + b, 0) / gaps.length) / (1000 * 60 * 60);
        if (avgGapHours > 1) {
          score += 0.15;
          signals.push(`Avg ${avgGapHours.toFixed(1)}h between posts (consistent)`);
        } else if (avgGapHours < 0.1) {
          score -= 0.1;
          signals.push(`Rapid-fire posting (${(avgGapHours * 60).toFixed(0)}min avg gap)`);
        }
      }
    }

    // Has followers = others trust them
    if (profile.followerCount > 0) {
      score += Math.min(0.2, profile.followerCount * 0.02);
      signals.push(`${profile.followerCount} followers (social proof)`);
    }

    return {
      score: Math.max(0, Math.min(1.0, score)),
      confidence,
      reasoning: score > 0.6 ? 'Established and reliable' : score > 0.3 ? 'Moderate reliability' : 'Low reliability signals',
      signals,
    };
  }

  // ============================================================
  // EXPERTISE DETECTION
  // ============================================================

  private detectExpertise(profile: SocialProfile, posts: SocialPost[]): ExpertiseDomain[] {
    const domainScores: Record<string, number> = {};

    // Analyze profile description
    const profileText = `${profile.description ?? ''} ${profile.displayName ?? ''}`.toLowerCase();
    for (const [domain, keywords] of Object.entries(DOMAIN_KEYWORDS)) {
      domainScores[domain] = 0;
      for (const kw of keywords) {
        if (profileText.includes(kw)) {
          domainScores[domain] += 0.15;
        }
      }
    }

    // Analyze post content
    for (const post of posts) {
      const text = `${post.title ?? ''} ${post.content}`.toLowerCase();
      for (const [domain, keywords] of Object.entries(DOMAIN_KEYWORDS)) {
        for (const kw of keywords) {
          if (text.includes(kw)) {
            domainScores[domain] += 0.08; // Each keyword match in a post
          }
        }
      }
    }

    // Normalize and filter
    const maxScore = Math.max(...Object.values(domainScores), 0.01);
    return Object.entries(domainScores)
      .map(([domain, raw]) => ({
        domain,
        confidence: Math.min(1.0, raw / maxScore),
      }))
      .filter(d => d.confidence > 0.2)
      .sort((a, b) => b.confidence - a.confidence)
      .slice(0, 5);
  }

  // ============================================================
  // COMPOSITE SCORING
  // ============================================================

  private computeTrustScore(
    spam: DimensionScore,
    authentic: DimensionScore,
    influence: DimensionScore,
    engagement: DimensionScore,
    reliability: DimensionScore,
  ): number {
    // Weighted composite: spam is inverted (high spam = low trust)
    const weights = {
      spam: -0.35,        // Negative weight — spam reduces trust
      authentic: 0.25,
      influence: 0.15,
      engagement: 0.15,
      reliability: 0.10,
    };

    const raw =
      (1 - spam.score) * Math.abs(weights.spam) +
      authentic.score * weights.authentic +
      influence.score * weights.influence +
      engagement.score * weights.engagement +
      reliability.score * weights.reliability;

    return Math.max(0, Math.min(1.0, raw));
  }

  // ============================================================
  // LABELING
  // ============================================================

  private generateLabels(
    spam: DimensionScore,
    authentic: DimensionScore,
    influence: DimensionScore,
    engagement: DimensionScore,
    reliability: DimensionScore,
    expertise: ExpertiseDomain[],
  ): string[] {
    const labels: string[] = [];

    // Spam labels
    if (spam.score > 0.7) labels.push('likely-spam');
    else if (spam.score > 0.4) labels.push('suspicious');

    // Quality labels
    if (authentic.score > 0.7) labels.push('authentic');
    if (influence.score > 0.6) labels.push('influential');
    if (engagement.score > 0.6) labels.push('high-engagement');
    if (reliability.score > 0.6) labels.push('reliable');

    // Composite labels
    if (authentic.score > 0.6 && influence.score > 0.4 && spam.score < 0.2) {
      labels.push('quality-agent');
    }
    if (spam.score < 0.1 && authentic.score > 0.5 && expertise.length > 0) {
      labels.push('domain-expert');
    }

    // Expertise labels
    if (expertise.length > 0) {
      labels.push(`expert:${expertise[0].domain}`);
    }

    if (labels.length === 0) {
      labels.push('unclassified');
    }

    return labels;
  }

  // ============================================================
  // RECOMMENDATIONS
  // ============================================================

  private generateRecommendations(
    trustScore: number,
    labels: string[],
    spam: DimensionScore,
    agentName: string,
  ): string[] {
    const recs: string[] = [];

    if (labels.includes('likely-spam')) {
      recs.push(`Avoid engaging with ${agentName} — high spam probability`);
      recs.push('Do not follow or respond to promotional content');
    } else if (labels.includes('suspicious')) {
      recs.push(`Exercise caution with ${agentName} — some suspicious patterns detected`);
      recs.push('Monitor for further spam signals before engaging');
    }

    if (labels.includes('quality-agent')) {
      recs.push(`${agentName} appears to be a quality contributor — consider following`);
    }

    if (labels.includes('domain-expert')) {
      recs.push(`${agentName} shows domain expertise — good candidate for engagement`);
    }

    if (labels.includes('influential')) {
      recs.push(`${agentName} has significant community influence — engagement may boost visibility`);
    }

    if (trustScore > 0.6 && !labels.includes('suspicious')) {
      recs.push('Safe to engage, follow, and reference in discussions');
    }

    if (recs.length === 0) {
      recs.push('Insufficient data for strong recommendations — gather more with depth=deep');
    }

    return recs;
  }

  // ============================================================
  // RENDERING
  // ============================================================

  private renderSummary(c: AgentClassification): string {
    const bar = (score: number): string => {
      const filled = Math.round(score * 10);
      return '\u2588'.repeat(filled) + '\u2591'.repeat(10 - filled);
    };

    const lines: string[] = [];
    lines.push(`Agent Classification: ${c.agentName} on ${c.platform}`);
    lines.push(`${c.profileUrl}`);
    lines.push('');
    lines.push(`Account: ${c.accountAge} | ${c.karma} karma | ${c.postCount} posts | ${c.followerCount} followers`);
    lines.push('');
    lines.push('Dimensions (0.0 - 1.0):');
    lines.push(`  Spam:        ${bar(c.dimensions.spam.score)} ${c.dimensions.spam.score.toFixed(2)} (${c.dimensions.spam.reasoning})`);
    lines.push(`  Authentic:   ${bar(c.dimensions.authentic.score)} ${c.dimensions.authentic.score.toFixed(2)} (${c.dimensions.authentic.reasoning})`);
    lines.push(`  Influence:   ${bar(c.dimensions.influence.score)} ${c.dimensions.influence.score.toFixed(2)} (${c.dimensions.influence.reasoning})`);
    lines.push(`  Engagement:  ${bar(c.dimensions.engagement.score)} ${c.dimensions.engagement.score.toFixed(2)} (${c.dimensions.engagement.reasoning})`);
    lines.push(`  Reliability: ${bar(c.dimensions.reliability.score)} ${c.dimensions.reliability.score.toFixed(2)} (${c.dimensions.reliability.reasoning})`);
    lines.push('');
    lines.push(`Trust Score: ${(c.trustScore * 100).toFixed(0)}%`);
    lines.push(`Labels: ${c.labels.join(', ')}`);

    if (c.expertise.length > 0) {
      lines.push(`Expertise: ${c.expertise.map(e => `${e.domain} (${(e.confidence * 100).toFixed(0)}%)`).join(', ')}`);
    }

    lines.push('');
    lines.push('Recommendations:');
    for (const rec of c.recommendations) {
      lines.push(`  - ${rec}`);
    }

    lines.push(`\nPosts analyzed: ${c.postsAnalyzed}`);
    return lines.join('\n');
  }

  private formatAccountAge(createdAt: string): string {
    const ms = Date.now() - new Date(createdAt).getTime();
    const hours = ms / (1000 * 60 * 60);
    if (hours < 24) return `${Math.round(hours)}h`;
    const days = hours / 24;
    if (days < 30) return `${Math.round(days)}d`;
    return `${Math.round(days / 30)}mo`;
  }
}
