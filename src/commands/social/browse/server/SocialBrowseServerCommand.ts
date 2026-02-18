/**
 * Social Browse Command - Server Implementation
 *
 * Intelligent exploration of social media platforms.
 * Combines multiple API calls per mode and returns rich, AI-friendly summaries.
 */

import type { JTAGContext } from '@system/core/types/JTAGTypes';
import { transformPayload } from '@system/core/types/JTAGTypes';
import type { ICommandDaemon } from '@daemons/command-daemon/shared/CommandBase';
import { SocialBrowseBaseCommand } from '../shared/SocialBrowseCommand';
import type { SocialBrowseParams, SocialBrowseResult, BrowseMode } from '../shared/SocialBrowseTypes';
import { loadSocialContext } from '@system/social/server/SocialCommandHelper';
import type { SocialPost, SocialComment, SocialCommunity, SocialProfile } from '@system/social/shared/SocialMediaTypes';

export class SocialBrowseServerCommand extends SocialBrowseBaseCommand {

  constructor(context: JTAGContext, subpath: string, commander: ICommandDaemon) {
    super(context, subpath, commander);
  }

  protected async executeSocialBrowse(params: SocialBrowseParams): Promise<SocialBrowseResult> {
    const { platform } = params;
    const mode: BrowseMode = params.mode ?? 'trending';

    if (!platform) throw new Error('platform is required');

    const ctx = await loadSocialContext(platform, params.personaId, params);

    switch (mode) {
      case 'discover':
        return this.browseDiscover(params, ctx);
      case 'community':
        return this.browseCommunity(params, ctx);
      case 'post':
        return this.browsePost(params, ctx);
      case 'agent':
        return this.browseAgent(params, ctx);
      case 'trending':
      default:
        return this.browseTrending(params, ctx);
    }
  }

  /** Discover — List all communities with activity context */
  private async browseDiscover(
    params: SocialBrowseParams,
    ctx: { provider: import('@system/social/shared/ISocialMediaProvider').ISocialMediaProvider },
  ): Promise<SocialBrowseResult> {
    const communities = await ctx.provider.listCommunities();

    const lines = communities.map(c => {
      const sub = c.isSubscribed ? ' [subscribed]' : '';
      return `  m/${c.name} — ${c.description || 'No description'} (${c.memberCount} members, ${c.postCount} posts)${sub}`;
    });

    const summary = communities.length === 0
      ? `No communities found on ${params.platform}.`
      : `Found ${communities.length} communities on ${params.platform}:\n${lines.join('\n')}`;

    return transformPayload(params, {
      success: true,
      mode: 'discover',
      message: `Discovered ${communities.length} communities on ${params.platform}`,
      summary,
      communities,
    });
  }

  /** Community — Browse a specific community's feed */
  private async browseCommunity(
    params: SocialBrowseParams,
    ctx: { provider: import('@system/social/shared/ISocialMediaProvider').ISocialMediaProvider },
  ): Promise<SocialBrowseResult> {
    const community = params.target;
    if (!community) throw new Error('target is required for community mode (community/submolt name)');

    const limit = params.limit ?? 15;
    const sort = params.sort ?? 'hot';
    const posts = await ctx.provider.getCommunityFeed(community, sort, limit);

    const lines = posts.map((p, i) => {
      const votes = p.votes > 0 ? `+${p.votes}` : String(p.votes);
      return `  ${i + 1}. [${votes}] "${p.title}" by ${p.authorName} (${p.commentCount} comments) — ${p.id}`;
    });

    const summary = posts.length === 0
      ? `m/${community} has no posts (sort: ${sort}).`
      : `m/${community} — ${sort} feed (${posts.length} posts):\n${lines.join('\n')}\n\nUse mode=post --target=<id> to read any post in detail.`;

    return transformPayload(params, {
      success: true,
      mode: 'community',
      message: `Browsed m/${community} (${sort}, ${posts.length} posts)`,
      summary,
      posts,
    });
  }

  /** Post — Read a full post with threaded comments */
  private async browsePost(
    params: SocialBrowseParams,
    ctx: { provider: import('@system/social/shared/ISocialMediaProvider').ISocialMediaProvider },
  ): Promise<SocialBrowseResult> {
    const postId = params.target;
    if (!postId) throw new Error('target is required for post mode (post ID)');

    const [post, comments] = await Promise.all([
      ctx.provider.getPost(postId),
      ctx.provider.getComments(postId, params.sort),
    ]);

    // Build threaded comment view
    const commentLines = this.renderCommentTree(comments);
    const votes = post.votes > 0 ? `+${post.votes}` : String(post.votes);

    const summary = [
      `"${post.title}" by ${post.authorName} in m/${post.community ?? 'unknown'}`,
      `${votes} votes · ${post.commentCount} comments · ${post.createdAt}`,
      ``,
      post.content,
      ``,
      comments.length > 0
        ? `--- Comments (${comments.length}) ---\n${commentLines}`
        : `--- No comments yet ---`,
      ``,
      `Post ID: ${post.id}`,
      post.url ? `Link: ${post.url}` : '',
    ].filter(Boolean).join('\n');

    return transformPayload(params, {
      success: true,
      mode: 'post',
      message: `Read post "${post.title}" with ${comments.length} comments`,
      summary,
      post,
      comments,
    });
  }

  /** Agent — View an agent's profile */
  private async browseAgent(
    params: SocialBrowseParams,
    ctx: { provider: import('@system/social/shared/ISocialMediaProvider').ISocialMediaProvider },
  ): Promise<SocialBrowseResult> {
    const agentName = params.target;
    if (!agentName) throw new Error('target is required for agent mode (agent username)');

    const profile = await ctx.provider.getProfile(agentName);

    const summary = [
      `u/${profile.agentName}${profile.displayName ? ` (${profile.displayName})` : ''}`,
      profile.description ? `  "${profile.description}"` : '',
      `  ${profile.karma} karma · ${profile.followerCount} followers · ${profile.followingCount} following · ${profile.postCount} posts`,
      `  Joined: ${profile.createdAt}`,
      `  Profile: ${profile.profileUrl}`,
    ].filter(Boolean).join('\n');

    return transformPayload(params, {
      success: true,
      mode: 'agent',
      message: `Viewed profile of ${profile.agentName} (${profile.karma} karma)`,
      summary,
      profile,
    });
  }

  /** Trending — Hot posts across the platform */
  private async browseTrending(
    params: SocialBrowseParams,
    ctx: { provider: import('@system/social/shared/ISocialMediaProvider').ISocialMediaProvider },
  ): Promise<SocialBrowseResult> {
    const limit = params.limit ?? 15;
    const sort = params.sort ?? 'hot';
    const posts = await ctx.provider.getFeed({ sort, limit });

    const lines = posts.map((p, i) => {
      const votes = p.votes > 0 ? `+${p.votes}` : String(p.votes);
      const community = p.community ? `m/${p.community}` : '';
      return `  ${i + 1}. [${votes}] "${p.title}" by ${p.authorName} ${community} (${p.commentCount} comments) — ${p.id}`;
    });

    const summary = posts.length === 0
      ? `No posts found on ${params.platform} (sort: ${sort}).`
      : `${params.platform} — ${sort} feed (${posts.length} posts):\n${lines.join('\n')}\n\nUse mode=post --target=<id> to read any post in detail.`;

    return transformPayload(params, {
      success: true,
      mode: 'trending',
      message: `Fetched ${posts.length} trending posts from ${params.platform}`,
      summary,
      posts,
    });
  }

  /**
   * Render comments as an indented thread tree.
   * Groups by parentId, renders depth via indentation.
   */
  private renderCommentTree(comments: SocialComment[]): string {
    if (comments.length === 0) return '';

    // Build parent→children map
    const childrenOf = new Map<string | undefined, SocialComment[]>();
    for (const c of comments) {
      const parentKey = c.parentId ?? undefined;
      const siblings = childrenOf.get(parentKey) ?? [];
      siblings.push(c);
      childrenOf.set(parentKey, siblings);
    }

    const lines: string[] = [];

    const render = (parentId: string | undefined, depth: number): void => {
      const children = childrenOf.get(parentId) ?? [];
      for (const c of children) {
        const indent = '  '.repeat(depth + 1);
        const votes = c.votes > 0 ? `+${c.votes}` : String(c.votes);
        lines.push(`${indent}[${votes}] ${c.authorName}: ${c.content}`);
        render(c.id, depth + 1);
      }
    };

    render(undefined, 0);

    // If tree rendering found nothing (flat comments without parentId linkage),
    // fall back to flat rendering
    if (lines.length === 0) {
      for (const c of comments) {
        const indent = '  '.repeat((c.depth ?? 0) + 1);
        const votes = c.votes > 0 ? `+${c.votes}` : String(c.votes);
        lines.push(`${indent}[${votes}] ${c.authorName}: ${c.content}`);
      }
    }

    return lines.join('\n');
  }
}
