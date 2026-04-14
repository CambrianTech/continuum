/**
 * ISocialMediaProvider - Generic interface for social media platform adapters
 *
 * Follows the same polymorphism pattern as IAdapterProvider (adapter system).
 * Each platform (Moltbook, future others) implements this interface.
 *
 * Provider instances are per-persona — each persona has their own API key
 * and rate limit tracking.
 */

import type {
  SignupParams,
  SignupResult,
  SocialPost,
  SocialComment,
  SocialNotification,
  SocialProfile,
  SocialCommunity,
  SocialSearchResult,
  SocialDM,
  CreatePostParams,
  FeedParams,
  CreateCommentParams,
  VoteParams,
  SearchParams,
  UpdateProfileParams,
  CreateCommunityParams,
  RateLimitStatus,
} from './SocialMediaTypes';

/**
 * Raised when a platform returns a user-resolvable setup/verification gate
 * (e.g. Moltbook's "owner must connect dashboard / X account" 403). This is
 * NOT a runtime failure — the caller should surface `humanMessage` + the
 * action fields to the user rather than treating it like a crash.
 *
 * Providers SHOULD throw this (rather than a plain Error) whenever the
 * platform response carries a structured "action required" shape.
 */
export class SocialActionRequiredError extends Error {
  readonly humanMessage: string;
  readonly setupUrl?: string;
  readonly apiAlternative?: string;
  readonly platformId: string;
  readonly status: number;

  constructor(args: {
    platformId: string;
    status: number;
    humanMessage: string;
    setupUrl?: string;
    apiAlternative?: string;
  }) {
    super(args.humanMessage);
    this.name = 'SocialActionRequiredError';
    this.platformId = args.platformId;
    this.status = args.status;
    this.humanMessage = args.humanMessage;
    this.setupUrl = args.setupUrl;
    this.apiAlternative = args.apiAlternative;
  }
}

export interface ISocialMediaProvider {
  /** Platform identifier (e.g., 'moltbook') */
  readonly platformId: string;

  /** Human-readable platform name (e.g., 'Moltbook') */
  readonly platformName: string;

  /** Base URL of the platform API */
  readonly apiBaseUrl: string;

  // ============ Authentication ============

  /**
   * Set the API key for authenticated requests.
   * Called after loading credential from ORM.
   */
  authenticate(apiKey: string): void;

  /**
   * Check if the provider has a valid API key set.
   */
  get isAuthenticated(): boolean;

  // ============ Registration ============

  /**
   * Register a new agent on the platform.
   * Does NOT require authentication (creates the credential).
   */
  signup(params: SignupParams): Promise<SignupResult>;

  // ============ Posts ============

  createPost(params: CreatePostParams): Promise<SocialPost>;
  getFeed(params: FeedParams): Promise<SocialPost[]>;
  getPost(postId: string): Promise<SocialPost>;
  deletePost(postId: string): Promise<void>;

  // ============ Comments ============

  createComment(params: CreateCommentParams): Promise<SocialComment>;
  getComments(postId: string, sort?: string): Promise<SocialComment[]>;
  deleteComment(postId: string, commentId: string): Promise<void>;

  // ============ Voting ============

  vote(params: VoteParams): Promise<void>;

  // ============ Social ============

  follow(agentName: string): Promise<void>;
  unfollow(agentName: string): Promise<void>;

  // ============ Direct Messages (if platform supports) ============

  sendDM(agentName: string, content: string): Promise<SocialDM>;

  // ============ Discovery ============

  search(params: SearchParams): Promise<SocialSearchResult>;
  listCommunities(): Promise<SocialCommunity[]>;
  getCommunityFeed(community: string, sort?: string, limit?: number): Promise<SocialPost[]>;

  // ============ Notifications ============

  getNotifications(since?: string): Promise<SocialNotification[]>;

  // ============ Profile ============

  getProfile(agentName?: string): Promise<SocialProfile>;
  updateProfile(params: UpdateProfileParams): Promise<void>;

  // ============ Communities ============

  createCommunity(params: CreateCommunityParams): Promise<SocialCommunity>;
  subscribeToCommunity(name: string): Promise<void>;
  unsubscribeFromCommunity(name: string): Promise<void>;

  // ============ Rate Limiting ============

  /**
   * Check if a specific action is rate-limited.
   * Provider tracks its own limits internally.
   */
  checkRateLimit(action: 'post' | 'comment' | 'vote' | 'request'): RateLimitStatus;

  // ============ Health ============

  /**
   * Check if the platform API is reachable.
   */
  ping(): Promise<boolean>;
}
