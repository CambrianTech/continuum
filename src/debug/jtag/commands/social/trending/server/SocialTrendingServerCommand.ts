/**
 * Social Trending Command - Server Implementation
 *
 * Discover trending and popular content on a social media platform.
 * Uses the feed endpoint with sort=hot (default), top, or rising.
 */

import { CommandBase, type ICommandDaemon } from '@daemons/command-daemon/shared/CommandBase';
import type { JTAGContext } from '@system/core/types/JTAGTypes';
import { transformPayload } from '@system/core/types/JTAGTypes';
import type { SocialTrendingParams, SocialTrendingResult } from '../shared/SocialTrendingTypes';
import { loadSocialContext } from '@system/social/server/SocialCommandHelper';

export class SocialTrendingServerCommand extends CommandBase<SocialTrendingParams, SocialTrendingResult> {

  constructor(context: JTAGContext, subpath: string, commander: ICommandDaemon) {
    super('social/trending', context, subpath, commander);
  }

  async execute(params: SocialTrendingParams): Promise<SocialTrendingResult> {
    const { platform, community, limit } = params;
    const sort = params.sort ?? 'hot';
    const effectiveLimit = limit ?? 10;

    if (!platform) throw new Error('platform is required');

    const ctx = await loadSocialContext(platform, params.personaId, params);

    let posts;
    if (community) {
      posts = await ctx.provider.getCommunityFeed(community, sort, effectiveLimit);
    } else {
      posts = await ctx.provider.getFeed({ sort, limit: effectiveLimit });
    }

    const source = community ? `${platform}/${community}` : platform;
    return transformPayload(params, {
      success: true,
      message: `Fetched ${posts.length} trending posts from ${source} (${sort})`,
      posts,
    });
  }
}
