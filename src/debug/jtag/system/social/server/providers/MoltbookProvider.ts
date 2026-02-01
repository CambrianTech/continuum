/**
 * MoltbookProvider - Moltbook.com social media platform adapter
 *
 * Moltbook is an AI-only social network. API docs: https://moltbook.com/skill.md
 *
 * Base URL: https://www.moltbook.com/api/v1
 * Auth: Bearer token from POST /agents/register
 *
 * Rate limits (per-provider-instance, per-persona):
 * - 100 requests/min (general)
 * - 1 post/30min
 * - 50 comments/hr
 */

import type { ISocialMediaProvider } from '../../shared/ISocialMediaProvider';
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
} from '../../shared/SocialMediaTypes';

/**
 * In-memory rate limit tracker — ephemeral, per provider instance.
 * Rate limits reset when the provider is recreated (e.g., server restart).
 * This is acceptable because Moltbook enforces its own server-side limits;
 * client-side tracking is purely to avoid wasting API calls.
 */
interface RateLimitTracker {
  requestTimestamps: number[];       // Sliding window for 100 req/min
  lastPostTimestamp: number;         // Last post time (1 post/30min)
  commentTimestamps: number[];       // Sliding window for 50 comments/hr
}

export class MoltbookProvider implements ISocialMediaProvider {
  readonly platformId = 'moltbook';
  readonly platformName = 'Moltbook';
  readonly apiBaseUrl = 'https://www.moltbook.com/api/v1';

  private _apiKey: string | null = null;
  private readonly rateLimits: RateLimitTracker = {
    requestTimestamps: [],
    lastPostTimestamp: 0,
    commentTimestamps: [],
  };

  // ============ Authentication ============

  authenticate(apiKey: string): void {
    this._apiKey = apiKey;
  }

  get isAuthenticated(): boolean {
    return this._apiKey !== null;
  }

  // ============ Registration ============

  async signup(params: SignupParams): Promise<SignupResult> {
    const body: Record<string, unknown> = {
      name: params.agentName,
    };
    if (params.description) body.description = params.description;
    if (params.metadata) body.metadata = params.metadata;

    const response = await this.request('POST', '/agents/register', body, false);

    if (!response.ok) {
      const errorText = await response.text();
      return { success: false, error: `Registration failed (${response.status}): ${errorText}` };
    }

    const data = await response.json();

    // Moltbook returns success: false with 200 status for validation errors
    if (data.success === false) {
      return { success: false, error: data.error ?? data.hint ?? 'Registration failed' };
    }

    // API nests agent data under 'agent' field
    const agent = data.agent ?? data;
    return {
      success: true,
      apiKey: agent.api_key,
      agentName: agent.name ?? params.agentName,
      claimUrl: agent.claim_url ?? data.claim_url,
      verificationCode: agent.verification_code ?? data.verification_code,
      profileUrl: agent.profile_url ?? `https://www.moltbook.com/u/${params.agentName}`,
    };
  }

  // ============ Posts ============

  async createPost(params: CreatePostParams): Promise<SocialPost> {
    const rateCheck = this.checkRateLimit('post');
    if (!rateCheck.allowed) {
      throw new Error(rateCheck.message ?? 'Rate limited for posts');
    }

    const body: Record<string, unknown> = {
      title: params.title,
      content: params.content,
    };
    if (params.community) body.submolt = params.community;
    if (params.url) body.url = params.url;

    const response = await this.authedRequest('POST', '/posts', body);
    const data = await response.json();

    this.rateLimits.lastPostTimestamp = Date.now();

    // Moltbook wraps created post in a 'post' field
    const postData = data.post ?? data;
    return this.mapPost(postData as Record<string, unknown>);
  }

  async getFeed(params: FeedParams): Promise<SocialPost[]> {
    const searchParams = new URLSearchParams();
    if (params.sort) searchParams.set('sort', params.sort);
    if (params.limit) searchParams.set('limit', String(params.limit));

    const endpoint = params.personalized ? '/feed' : '/posts';
    const query = searchParams.toString();
    const url = query ? `${endpoint}?${query}` : endpoint;

    const response = await this.authedRequest('GET', url);
    const data = await response.json();

    const posts = Array.isArray(data) ? data : (data.posts ?? data.results ?? []);
    return posts.map((p: Record<string, unknown>) => this.mapPost(p));
  }

  async getPost(postId: string): Promise<SocialPost> {
    const response = await this.authedRequest('GET', `/posts/${postId}`);
    const data = await response.json();
    const postData = data.post ?? data;
    return this.mapPost(postData as Record<string, unknown>);
  }

  async deletePost(postId: string): Promise<void> {
    await this.authedRequest('DELETE', `/posts/${postId}`);
  }

  // ============ Comments ============

  async createComment(params: CreateCommentParams): Promise<SocialComment> {
    const rateCheck = this.checkRateLimit('comment');
    if (!rateCheck.allowed) {
      throw new Error(rateCheck.message ?? 'Rate limited for comments');
    }

    const body: Record<string, unknown> = {
      content: params.content,
    };
    if (params.parentId) body.parent_id = params.parentId;

    const response = await this.authedRequest('POST', `/posts/${params.postId}/comments`, body);
    const data = await response.json();

    this.rateLimits.commentTimestamps.push(Date.now());

    return this.mapComment(data, params.postId);
  }

  async deleteComment(postId: string, commentId: string): Promise<void> {
    await this.authedRequest('DELETE', `/posts/${postId}/comments/${commentId}`);
  }

  async getComments(postId: string, _sort?: string): Promise<SocialComment[]> {
    // Moltbook returns comments embedded in the single-post response,
    // not from a dedicated /comments endpoint (which returns empty).
    const response = await this.authedRequest('GET', `/posts/${postId}`);
    const data = await response.json();

    const post = data.post ?? data;
    const comments = Array.isArray(post.comments) ? post.comments : (data.comments ?? []);
    return comments.map((c: Record<string, unknown>) => this.mapComment(c, postId));
  }

  // ============ Voting ============

  async vote(params: VoteParams): Promise<void> {
    const action = params.direction === 'up' ? 'upvote' : 'downvote';

    if (params.targetType === 'post') {
      await this.authedRequest('POST', `/posts/${params.targetId}/${action}`);
    } else {
      await this.authedRequest('POST', `/comments/${params.targetId}/${action}`);
    }
  }

  // ============ Social ============

  async follow(agentName: string): Promise<void> {
    await this.authedRequest('POST', `/agents/${agentName}/follow`);
  }

  async unfollow(agentName: string): Promise<void> {
    await this.authedRequest('DELETE', `/agents/${agentName}/follow`);
  }

  // ============ DMs ============

  async sendDM(agentName: string, content: string): Promise<SocialDM> {
    const response = await this.authedRequest('POST', `/agents/${agentName}/dm`, { content });
    const data = await response.json();
    return {
      id: String(data.id ?? ''),
      fromAgent: String(data.from_agent ?? data.from ?? ''),
      toAgent: agentName,
      content,
      read: false,
      createdAt: String(data.created_at ?? new Date().toISOString()),
    };
  }

  // ============ Discovery ============

  async search(params: SearchParams): Promise<SocialSearchResult> {
    const searchParams = new URLSearchParams({ q: params.query });
    if (params.type) searchParams.set('type', params.type);
    if (params.limit) searchParams.set('limit', String(params.limit));

    const response = await this.authedRequest('GET', `/search?${searchParams.toString()}`);
    const data = await response.json();

    const posts = Array.isArray(data) ? data : (data.posts ?? data.results ?? []);
    return {
      posts: posts.map((p: Record<string, unknown>) => this.mapPost(p)),
      totalCount: data.total_count ?? data.total ?? posts.length,
    };
  }

  async listCommunities(): Promise<SocialCommunity[]> {
    const response = await this.authedRequest('GET', '/submolts');
    const data = await response.json();

    const communities = Array.isArray(data) ? data : (data.submolts ?? data.results ?? []);
    return communities.map((c: Record<string, unknown>) => this.mapCommunity(c));
  }

  async getCommunityFeed(community: string, sort?: string, limit?: number): Promise<SocialPost[]> {
    const params = new URLSearchParams();
    if (sort) params.set('sort', sort);
    if (limit) params.set('limit', String(limit));

    const query = params.toString();
    const url = `/submolts/${community}/feed${query ? `?${query}` : ''}`;
    const response = await this.authedRequest('GET', url);
    const data = await response.json();

    const posts = Array.isArray(data) ? data : (data.posts ?? data.results ?? []);
    return posts.map((p: Record<string, unknown>) => this.mapPost(p));
  }

  // ============ Notifications ============

  async getNotifications(_since?: string): Promise<SocialNotification[]> {
    // Moltbook API has no dedicated notifications endpoint.
    // Returns empty until a synthetic notification system is built
    // (e.g., polling comments on own posts, tracking new followers).
    return [];
  }

  // ============ Profile ============

  async getProfile(agentName?: string): Promise<SocialProfile> {
    const endpoint = agentName ? `/agents/profile?name=${encodeURIComponent(agentName)}` : '/agents/me';
    const response = await this.authedRequest('GET', endpoint);
    const data = await response.json();
    // API wraps profile in 'agent' field
    const profileData = data.agent ?? data;
    return this.mapProfile(profileData);
  }

  async updateProfile(params: UpdateProfileParams): Promise<void> {
    const body: Record<string, unknown> = {};
    if (params.description !== undefined) body.description = params.description;
    if (params.metadata !== undefined) body.metadata = params.metadata;

    await this.authedRequest('PATCH', '/agents/me', body);
  }

  // ============ Communities ============

  async createCommunity(params: CreateCommunityParams): Promise<SocialCommunity> {
    const response = await this.authedRequest('POST', '/submolts', {
      name: params.name,
      display_name: params.displayName,
      description: params.description,
    });
    const data = await response.json();
    // Moltbook wraps created community in a 'submolt' field
    const communityData = data.submolt ?? data;
    return this.mapCommunity(communityData as Record<string, unknown>);
  }

  async subscribeToCommunity(name: string): Promise<void> {
    await this.authedRequest('POST', `/submolts/${name}/subscribe`);
  }

  async unsubscribeFromCommunity(name: string): Promise<void> {
    await this.authedRequest('DELETE', `/submolts/${name}/subscribe`);
  }

  // ============ Rate Limiting ============

  checkRateLimit(action: 'post' | 'comment' | 'vote' | 'request'): RateLimitStatus {
    const now = Date.now();

    // Clean up old timestamps
    const oneMinuteAgo = now - 60_000;
    const oneHourAgo = now - 3_600_000;
    this.rateLimits.requestTimestamps = this.rateLimits.requestTimestamps.filter(t => t > oneMinuteAgo);
    this.rateLimits.commentTimestamps = this.rateLimits.commentTimestamps.filter(t => t > oneHourAgo);

    // General request limit: 100/min
    if (this.rateLimits.requestTimestamps.length >= 100) {
      const oldestInWindow = this.rateLimits.requestTimestamps[0];
      const retryAfterMs = 60_000 - (now - oldestInWindow);
      return {
        allowed: false,
        retryAfterMs,
        message: `Rate limited: 100 requests/min exceeded. Retry in ${Math.ceil(retryAfterMs / 1000)}s`,
      };
    }

    // Post limit: 1/30min
    if (action === 'post') {
      const thirtyMinMs = 30 * 60_000;
      const timeSinceLastPost = now - this.rateLimits.lastPostTimestamp;
      if (this.rateLimits.lastPostTimestamp > 0 && timeSinceLastPost < thirtyMinMs) {
        const retryAfterMs = thirtyMinMs - timeSinceLastPost;
        const retryMinutes = Math.ceil(retryAfterMs / 60_000);
        return {
          allowed: false,
          retryAfterMs,
          message: `Rate limited: 1 post per 30 minutes. Next post allowed in ${retryMinutes} minutes`,
        };
      }
    }

    // Comment limit: 50/hr
    if (action === 'comment') {
      if (this.rateLimits.commentTimestamps.length >= 50) {
        const oldestInWindow = this.rateLimits.commentTimestamps[0];
        const retryAfterMs = 3_600_000 - (now - oldestInWindow);
        return {
          allowed: false,
          retryAfterMs,
          message: `Rate limited: 50 comments/hr exceeded. Retry in ${Math.ceil(retryAfterMs / 60_000)} minutes`,
        };
      }
    }

    return { allowed: true };
  }

  // ============ Health ============

  async ping(): Promise<boolean> {
    try {
      const response = await fetch(`${this.apiBaseUrl}/health`, {
        method: 'GET',
        signal: AbortSignal.timeout(5000),
      });
      return response.ok;
    } catch {
      // Health endpoint may not exist — try listing communities as fallback
      try {
        const response = await fetch(`${this.apiBaseUrl}/submolts`, {
          method: 'GET',
          signal: AbortSignal.timeout(5000),
        });
        return response.ok || response.status === 401; // 401 = API is up, just needs auth
      } catch {
        return false;
      }
    }
  }

  // ============ Private HTTP Helpers ============

  /**
   * Make an authenticated HTTP request.
   * Tracks rate limits and throws on HTTP errors.
   */
  private async authedRequest(
    method: string,
    path: string,
    body?: Record<string, unknown>,
  ): Promise<Response> {
    if (!this._apiKey) {
      throw new Error(`MoltbookProvider: Not authenticated. Call authenticate(apiKey) first.`);
    }

    const rateCheck = this.checkRateLimit('request');
    if (!rateCheck.allowed) {
      throw new Error(rateCheck.message ?? 'Rate limited');
    }

    return this.request(method, path, body, true);
  }

  /**
   * Make an HTTP request to the Moltbook API.
   * @param auth - Whether to include Authorization header
   */
  private async request(
    method: string,
    path: string,
    body?: Record<string, unknown>,
    auth: boolean = true,
  ): Promise<Response> {
    const url = `${this.apiBaseUrl}${path}`;
    const headers: Record<string, string> = {
      'Content-Type': 'application/json',
      'Accept': 'application/json',
    };

    if (auth && this._apiKey) {
      headers['Authorization'] = `Bearer ${this._apiKey}`;
    }

    const init: RequestInit = { method, headers };
    if (body && (method === 'POST' || method === 'PATCH' || method === 'PUT')) {
      init.body = JSON.stringify(body);
    }

    this.rateLimits.requestTimestamps.push(Date.now());

    const response = await fetch(url, init);

    if (!response.ok && response.status !== 404) {
      const errorText = await response.text().catch(() => 'Unknown error');
      throw new Error(`Moltbook API error (${method} ${path}): ${response.status} ${errorText}`);
    }

    return response;
  }

  // ============ Response Mappers ============

  private mapPost(data: Record<string, unknown>): SocialPost {
    // Moltbook returns author and submolt as nested objects or strings
    const author = data.author as Record<string, unknown> | string | undefined;
    const authorName = typeof author === 'object' && author !== null
      ? String(author.name ?? author.agent_name ?? author.display_name ?? '')
      : String(data.author_name ?? author ?? data.agent_name ?? '');
    const authorId = typeof author === 'object' && author !== null
      ? String(author.id ?? '')
      : (data.author_id ? String(data.author_id) : undefined);

    const submolt = data.submolt as Record<string, unknown> | string | undefined;
    const community = typeof submolt === 'object' && submolt !== null
      ? String(submolt.name ?? submolt.slug ?? '')
      : (typeof submolt === 'string' ? submolt : (data.community ? String(data.community) : undefined));
    const communityDisplayName = typeof submolt === 'object' && submolt !== null
      ? String(submolt.display_name ?? submolt.title ?? submolt.name ?? '')
      : (data.submolt_display_name ? String(data.submolt_display_name) : undefined);

    return {
      id: String(data.id ?? ''),
      title: String(data.title ?? ''),
      content: String(data.content ?? data.body ?? ''),
      url: data.url ? String(data.url) : undefined,
      authorName,
      authorId,
      community,
      communityDisplayName,
      votes: Number(data.votes ?? data.upvotes ?? data.score ?? 0),
      commentCount: Number(data.comment_count ?? data.comments ?? data.num_comments ?? 0),
      createdAt: String(data.created_at ?? data.createdAt ?? new Date().toISOString()),
      postUrl: String(data.post_url ?? data.permalink ?? `https://www.moltbook.com/posts/${data.id}`),
    };
  }

  private mapComment(data: Record<string, unknown>, postId: string): SocialComment {
    // Handle nested author object (same pattern as mapPost)
    const author = data.author as Record<string, unknown> | string | undefined;
    const authorName = typeof author === 'object' && author !== null
      ? String(author.name ?? author.agent_name ?? author.display_name ?? '')
      : String(data.author_name ?? author ?? data.agent_name ?? '');
    const authorId = typeof author === 'object' && author !== null
      ? String(author.id ?? '')
      : (data.author_id ? String(data.author_id) : undefined);

    return {
      id: String(data.id ?? ''),
      postId: String(data.post_id ?? postId),
      parentId: data.parent_id ? String(data.parent_id) : undefined,
      content: String(data.content ?? data.body ?? ''),
      authorName,
      authorId,
      votes: Number(data.votes ?? data.upvotes ?? data.score ?? 0),
      depth: Number(data.depth ?? data.level ?? 0),
      createdAt: String(data.created_at ?? data.createdAt ?? new Date().toISOString()),
    };
  }

  private mapProfile(data: Record<string, unknown>): SocialProfile {
    const agentName = String(data.agent_name ?? data.username ?? data.name ?? '');
    return {
      agentName,
      displayName: data.display_name ? String(data.display_name) : undefined,
      description: data.description ? String(data.description) : undefined,
      followerCount: Number(data.follower_count ?? data.followers ?? 0),
      followingCount: Number(data.following_count ?? data.following ?? 0),
      postCount: Number(data.post_count ?? data.posts ?? 0),
      karma: Number(data.karma ?? data.reputation ?? 0),
      createdAt: String(data.created_at ?? data.createdAt ?? new Date().toISOString()),
      profileUrl: String(data.profile_url ?? `https://www.moltbook.com/u/${agentName}`),
      metadata: (data.metadata as Record<string, unknown>) ?? undefined,
    };
  }

  private mapCommunity(data: Record<string, unknown>): SocialCommunity {
    return {
      name: String(data.name ?? ''),
      displayName: String(data.display_name ?? data.displayName ?? data.name ?? ''),
      description: String(data.description ?? ''),
      memberCount: Number(data.member_count ?? data.members ?? data.subscribers ?? 0),
      postCount: Number(data.post_count ?? data.posts ?? 0),
      createdAt: String(data.created_at ?? data.createdAt ?? new Date().toISOString()),
      isSubscribed: data.is_subscribed != null ? Boolean(data.is_subscribed) : undefined,
    };
  }
}
