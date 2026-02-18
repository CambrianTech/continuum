/**
 * Social Downvote Command - Server Implementation
 *
 * Downvote a post on a social media platform.
 * Convenience command — delegates to provider.vote() with direction='down'.
 */

import { CommandBase, type ICommandDaemon } from '@daemons/command-daemon/shared/CommandBase';
import type { JTAGContext } from '@system/core/types/JTAGTypes';
import type { SocialDownvoteParams, SocialDownvoteResult } from '../shared/SocialDownvoteTypes';
import { createSocialDownvoteResultFromParams } from '../shared/SocialDownvoteTypes';
import { loadSocialContext } from '@system/social/server/SocialCommandHelper';
import { Logger } from '@system/core/logging/Logger';

const log = Logger.create('social/downvote');

export class SocialDownvoteServerCommand extends CommandBase<SocialDownvoteParams, SocialDownvoteResult> {

  constructor(context: JTAGContext, subpath: string, commander: ICommandDaemon) {
    super('social/downvote', context, subpath, commander);
  }

  async execute(params: SocialDownvoteParams): Promise<SocialDownvoteResult> {
    const { platform, postId } = params;

    if (!platform) {
      return createSocialDownvoteResultFromParams(params, {
        success: false,
        message: 'platform is required',
        postId: '',
      });
    }

    if (!postId) {
      return createSocialDownvoteResultFromParams(params, {
        success: false,
        message: 'postId is required',
        postId: '',
      });
    }

    try {
      const ctx = await loadSocialContext(platform, params.personaId, params);

      log.info(`Downvoting post: ${postId}`);
      await ctx.provider.vote({ targetId: postId, targetType: 'post', direction: 'down' });

      return createSocialDownvoteResultFromParams(params, {
        success: true,
        message: `Downvoted post ${postId}`,
        postId,
      });
    } catch (error) {
      return createSocialDownvoteResultFromParams(params, {
        success: false,
        message: `Downvote failed: ${String(error)}`,
        postId,
      });
    }
  }
}
