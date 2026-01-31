/**
 * Social Engage Command - Server Implementation
 *
 * All social interaction: vote, follow/unfollow, subscribe/unsubscribe.
 */

import type { JTAGContext } from '@system/core/types/JTAGTypes';
import { transformPayload } from '@system/core/types/JTAGTypes';
import type { ICommandDaemon } from '@daemons/command-daemon/shared/CommandBase';
import { SocialEngageBaseCommand } from '../shared/SocialEngageCommand';
import type { SocialEngageParams, SocialEngageResult, EngageAction } from '../shared/SocialEngageTypes';
import { loadSocialContext } from '@system/social/server/SocialCommandHelper';

export class SocialEngageServerCommand extends SocialEngageBaseCommand {

  constructor(context: JTAGContext, subpath: string, commander: ICommandDaemon) {
    super(context, subpath, commander);
  }

  protected async executeSocialEngage(params: SocialEngageParams): Promise<SocialEngageResult> {
    const { platform, action, target } = params;

    if (!platform) throw new Error('platform is required');
    if (!action) throw new Error('action is required');
    if (!target) throw new Error('target is required');

    const ctx = await loadSocialContext(platform, params.personaId, params);

    const rateCheck = ctx.provider.checkRateLimit(action === 'vote' ? 'vote' : 'request');
    if (!rateCheck.allowed) {
      return transformPayload(params, {
        success: false,
        message: rateCheck.message ?? `Rate limited for ${action}`,
        action,
        target,
      });
    }

    switch (action) {
      case 'vote':
        return this.handleVote(params, ctx);
      case 'follow':
        return this.handleFollow(params, ctx);
      case 'unfollow':
        return this.handleUnfollow(params, ctx);
      case 'subscribe':
        return this.handleSubscribe(params, ctx);
      case 'unsubscribe':
        return this.handleUnsubscribe(params, ctx);
      default:
        throw new Error(`Unknown engage action: ${action}. Valid: vote, follow, unfollow, subscribe, unsubscribe`);
    }
  }

  private async handleVote(
    params: SocialEngageParams,
    ctx: { provider: import('@system/social/shared/ISocialMediaProvider').ISocialMediaProvider },
  ): Promise<SocialEngageResult> {
    const targetType = params.targetType ?? 'post';
    const direction = params.direction ?? 'up';

    await ctx.provider.vote({
      targetId: params.target,
      targetType,
      direction,
    });

    const verb = direction === 'up' ? 'Upvoted' : 'Downvoted';
    return transformPayload(params, {
      success: true,
      message: `${verb} ${targetType} ${params.target} on ${params.platform}`,
      action: 'vote',
      target: params.target,
    });
  }

  private async handleFollow(
    params: SocialEngageParams,
    ctx: { provider: import('@system/social/shared/ISocialMediaProvider').ISocialMediaProvider },
  ): Promise<SocialEngageResult> {
    await ctx.provider.follow(params.target);

    return transformPayload(params, {
      success: true,
      message: `Now following ${params.target} on ${params.platform}`,
      action: 'follow',
      target: params.target,
    });
  }

  private async handleUnfollow(
    params: SocialEngageParams,
    ctx: { provider: import('@system/social/shared/ISocialMediaProvider').ISocialMediaProvider },
  ): Promise<SocialEngageResult> {
    await ctx.provider.unfollow(params.target);

    return transformPayload(params, {
      success: true,
      message: `Unfollowed ${params.target} on ${params.platform}`,
      action: 'unfollow',
      target: params.target,
    });
  }

  private async handleSubscribe(
    params: SocialEngageParams,
    ctx: { provider: import('@system/social/shared/ISocialMediaProvider').ISocialMediaProvider },
  ): Promise<SocialEngageResult> {
    await ctx.provider.subscribeToCommunity(params.target);

    return transformPayload(params, {
      success: true,
      message: `Subscribed to m/${params.target} on ${params.platform}`,
      action: 'subscribe',
      target: params.target,
    });
  }

  private async handleUnsubscribe(
    params: SocialEngageParams,
    ctx: { provider: import('@system/social/shared/ISocialMediaProvider').ISocialMediaProvider },
  ): Promise<SocialEngageResult> {
    await ctx.provider.unsubscribeFromCommunity(params.target);

    return transformPayload(params, {
      success: true,
      message: `Unsubscribed from m/${params.target} on ${params.platform}`,
      action: 'unsubscribe',
      target: params.target,
    });
  }
}
