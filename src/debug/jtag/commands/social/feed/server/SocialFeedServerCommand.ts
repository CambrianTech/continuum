/**
 * Social Feed Command - Server Implementation
 *
 * Reads the feed from a social media platform.
 * Supports global feed, personalized feed, and community-specific feeds.
 */

import type { JTAGContext } from '@system/core/types/JTAGTypes';
import { transformPayload } from '@system/core/types/JTAGTypes';
import type { ICommandDaemon } from '@daemons/command-daemon/shared/CommandBase';
import { SocialFeedBaseCommand } from '../shared/SocialFeedCommand';
import type { SocialFeedParams, SocialFeedResult } from '../shared/SocialFeedTypes';
import { loadSocialContext } from '@system/social/server/SocialCommandHelper';

export class SocialFeedServerCommand extends SocialFeedBaseCommand {

  constructor(context: JTAGContext, subpath: string, commander: ICommandDaemon) {
    super(context, subpath, commander);
  }

  protected async executeSocialFeed(params: SocialFeedParams): Promise<SocialFeedResult> {
    const { platform, sort, community, limit, personalized } = params;

    if (!platform) throw new Error('platform is required');

    const ctx = await loadSocialContext(platform, params.personaId, params);

    let posts;
    if (community) {
      posts = await ctx.provider.getCommunityFeed(community, sort, limit);
    } else {
      posts = await ctx.provider.getFeed({ sort, limit, personalized });
    }

    const source = community ? `${platform}/${community}` : platform;
    return transformPayload(params, {
      success: true,
      message: `Fetched ${posts.length} posts from ${source} (${sort ?? 'default'})`,
      posts,
    });
  }
}
