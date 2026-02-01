/**
 * Social Media Types - Platform-agnostic types for social media integration
 *
 * These types are generic and NOT tied to any specific platform.
 * Platform-specific adapters (MoltbookProvider, etc.) map their API
 * responses to these common types.
 */

import type { UUID } from '@system/core/types/CrossPlatformUUID';

// ============ Core Content Types ============

export interface SocialPost {
  id: string;
  title: string;
  content: string;
  url?: string;                     // Link post URL
  authorName: string;
  authorId?: string;
  community?: string;               // Submolt, subreddit, etc.
  communityDisplayName?: string;
  votes: number;
  commentCount: number;
  createdAt: string;                 // ISO timestamp
  postUrl: string;                   // Direct link to post on platform
}

export interface SocialComment {
  id: string;
  postId: string;
  parentId?: string;                 // For threading
  content: string;
  authorName: string;
  authorId?: string;
  votes: number;
  depth: number;                     // Nesting level (0 = top-level)
  createdAt: string;
}

export interface SocialNotification {
  id: string;
  type: 'reply' | 'mention' | 'follow' | 'vote' | 'dm' | 'system';
  content: string;
  authorName?: string;
  postId?: string;
  postTitle?: string;
  commentId?: string;
  read: boolean;
  createdAt: string;
}

export interface SocialProfile {
  agentName: string;
  displayName?: string;
  description?: string;
  followerCount: number;
  followingCount: number;
  postCount: number;
  karma: number;
  createdAt: string;
  profileUrl: string;
  metadata?: Record<string, unknown>;
}

export interface SocialCommunity {
  name: string;
  displayName: string;
  description: string;
  memberCount: number;
  postCount: number;
  createdAt: string;
  isSubscribed?: boolean;
}

export interface SocialSearchResult {
  posts: SocialPost[];
  totalCount?: number;
}

export interface SocialDM {
  id: string;
  fromAgent: string;
  toAgent: string;
  content: string;
  read: boolean;
  createdAt: string;
}

// ============ Request Parameter Types ============

export interface SignupParams {
  agentName: string;
  description?: string;
  metadata?: Record<string, unknown>;
}

export interface SignupResult {
  success: boolean;
  apiKey?: string;
  agentName?: string;
  claimUrl?: string;
  verificationCode?: string;
  profileUrl?: string;
  error?: string;
}

export interface CreatePostParams {
  title: string;
  content: string;
  community?: string;
  url?: string;                      // Link post
}

export interface FeedParams {
  sort?: 'hot' | 'new' | 'top' | 'rising';
  community?: string;
  limit?: number;
  personalized?: boolean;
}

export interface CreateCommentParams {
  postId: string;
  content: string;
  parentId?: string;                 // For threaded replies
}

export interface VoteParams {
  targetId: string;
  targetType: 'post' | 'comment';
  direction: 'up' | 'down';
}

export interface SearchParams {
  query: string;
  type?: 'post' | 'comment' | 'agent' | 'submolt';
  limit?: number;
}

export interface UpdateProfileParams {
  description?: string;
  metadata?: Record<string, unknown>;
}

export interface CreateCommunityParams {
  name: string;
  displayName: string;
  description: string;
}

// ============ Rate Limit ============

export interface RateLimitStatus {
  allowed: boolean;
  retryAfterMs?: number;
  message?: string;
}

// ============ Credential Reference ============

/**
 * Credential data stored per-persona in their longterm.db
 * Used by providers to authenticate API calls
 */
export interface SocialCredentialData {
  personaId: UUID;
  platformId: string;
  apiKey: string;
  agentName: string;
  profileUrl?: string;
  claimStatus: 'pending' | 'claimed' | 'unknown';
  registeredAt: string;              // ISO timestamp
  lastActiveAt?: string;
}
