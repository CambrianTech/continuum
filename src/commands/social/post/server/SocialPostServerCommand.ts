/**
 * Social Post Command - Server Implementation
 *
 * Creates a post on a social media platform using the persona's stored credentials.
 */

import type { JTAGContext } from '@system/core/types/JTAGTypes';
import { transformPayload } from '@system/core/types/JTAGTypes';
import type { ICommandDaemon } from '@daemons/command-daemon/shared/CommandBase';
import { SocialPostBaseCommand } from '../shared/SocialPostCommand';
import type { SocialPostParams, SocialPostResult } from '../shared/SocialPostTypes';
import { loadSocialContext } from '@system/social/server/SocialCommandHelper';
import { SocialActionRequiredError } from '@system/social/shared/ISocialMediaProvider';

export class SocialPostServerCommand extends SocialPostBaseCommand {

  constructor(context: JTAGContext, subpath: string, commander: ICommandDaemon) {
    super(context, subpath, commander);
  }

  protected async executeSocialPost(params: SocialPostParams): Promise<SocialPostResult> {
    const { platform, title, content, community, url } = params;

    if (!platform) throw new Error('platform is required');
    if (!title) throw new Error('title is required');
    if (!content) throw new Error('content is required');

    const ctx = await loadSocialContext(platform, params.personaId, params);

    // Check rate limit before posting
    const rateCheck = ctx.provider.checkRateLimit('post');
    if (!rateCheck.allowed) {
      return transformPayload(params, {
        success: false,
        message: rateCheck.message ?? 'Rate limited for posts',
      });
    }

    let post;
    try {
      post = await ctx.provider.createPost({ title, content, community, url });
    } catch (e) {
      // Owner-setup / verification gates are user-resolvable, not crashes.
      // Return a clean result with the action info so the CLI/UI can surface
      // it without dumping raw JSON.
      if (e instanceof SocialActionRequiredError) {
        return transformPayload(params, {
          success: false,
          message: e.humanMessage,
          actionRequired: {
            humanMessage: e.humanMessage,
            setupUrl: e.setupUrl,
            apiAlternative: e.apiAlternative,
          },
        });
      }
      throw e;
    }

    return transformPayload(params, {
      success: true,
      message: `Posted to ${platform}${community ? ` in ${community}` : ''}: "${title}"`,
      post,
    });
  }
}
