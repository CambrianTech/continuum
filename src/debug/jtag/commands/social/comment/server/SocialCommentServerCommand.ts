/**
 * Social Comment Command - Server Implementation
 *
 * Creates a comment on a post or replies to an existing comment (threaded).
 */

import type { JTAGContext } from '@system/core/types/JTAGTypes';
import { transformPayload } from '@system/core/types/JTAGTypes';
import type { ICommandDaemon } from '@daemons/command-daemon/shared/CommandBase';
import { SocialCommentBaseCommand } from '../shared/SocialCommentCommand';
import type { SocialCommentParams, SocialCommentResult } from '../shared/SocialCommentTypes';
import { loadSocialContext } from '@system/social/server/SocialCommandHelper';

export class SocialCommentServerCommand extends SocialCommentBaseCommand {

  constructor(context: JTAGContext, subpath: string, commander: ICommandDaemon) {
    super(context, subpath, commander);
  }

  protected async executeSocialComment(params: SocialCommentParams): Promise<SocialCommentResult> {
    const { platform, postId } = params;
    const action = params.action ?? 'create';

    if (!platform) throw new Error('platform is required');
    if (!postId) throw new Error('postId is required');

    const ctx = await loadSocialContext(platform, params.personaId, params);

    if (action === 'list') {
      const comments = await ctx.provider.getComments(postId, params.sort);
      return transformPayload(params, {
        success: true,
        message: `Fetched ${comments.length} comments from ${postId} on ${platform}`,
        comments,
      });
    }

    // action === 'create'
    if (!params.content) throw new Error('content is required for creating a comment');

    const rateCheck = ctx.provider.checkRateLimit('comment');
    if (!rateCheck.allowed) {
      return transformPayload(params, {
        success: false,
        message: rateCheck.message ?? 'Rate limited for comments',
      });
    }

    const comment = await ctx.provider.createComment({
      postId,
      content: params.content,
      parentId: params.parentId,
    });

    const verb = params.parentId ? 'Replied to comment' : 'Commented on post';
    return transformPayload(params, {
      success: true,
      message: `${verb} ${postId} on ${platform}`,
      comment,
    });
  }
}
