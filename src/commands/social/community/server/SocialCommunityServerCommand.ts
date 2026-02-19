/**
 * Social Community Command - Server Implementation
 *
 * Manage communities (submolts) — create, list, subscribe, unsubscribe, get info
 */

import { CommandBase, type ICommandDaemon } from '@daemons/command-daemon/shared/CommandBase';
import type { JTAGContext } from '@system/core/types/JTAGTypes';
import type { SocialCommunityParams, SocialCommunityResult } from '../shared/SocialCommunityTypes';
import { createSocialCommunityResultFromParams } from '../shared/SocialCommunityTypes';
import { loadSocialContext } from '@system/social/server/SocialCommandHelper';
import type { ISocialMediaProvider } from '@system/social/shared/ISocialMediaProvider';
import { Logger } from '@system/core/logging/Logger';

const log = Logger.create('social/community');

export class SocialCommunityServerCommand extends CommandBase<SocialCommunityParams, SocialCommunityResult> {

  constructor(context: JTAGContext, subpath: string, commander: ICommandDaemon) {
    super('social/community', context, subpath, commander);
  }

  async execute(params: SocialCommunityParams): Promise<SocialCommunityResult> {
    const { platform, action } = params;

    if (!platform) {
      return createSocialCommunityResultFromParams(params, {
        success: false,
        message: 'platform is required',
      });
    }

    if (!action) {
      return createSocialCommunityResultFromParams(params, {
        success: false,
        message: 'action is required (list, info, create, subscribe, unsubscribe)',
      });
    }

    try {
      const ctx = await loadSocialContext(platform, params.personaId, params);

      switch (action) {
        case 'list':
          return await this.handleList(params, ctx.provider);
        case 'info':
          return await this.handleInfo(params, ctx.provider);
        case 'create':
          return await this.handleCreate(params, ctx.provider);
        case 'subscribe':
          return await this.handleSubscribe(params, ctx.provider);
        case 'unsubscribe':
          return await this.handleUnsubscribe(params, ctx.provider);
        default:
          return createSocialCommunityResultFromParams(params, {
            success: false,
            message: `Unknown action: ${action}. Valid actions: list, info, create, subscribe, unsubscribe`,
          });
      }
    } catch (error) {
      return createSocialCommunityResultFromParams(params, {
        success: false,
        message: `Community action failed: ${String(error)}`,
      });
    }
  }

  private async handleList(
    params: SocialCommunityParams,
    provider: ISocialMediaProvider,
  ): Promise<SocialCommunityResult> {
    log.info('Listing communities');
    const communities = await provider.listCommunities();

    const summary = communities.length === 0
      ? 'No communities found'
      : `${communities.length} communities:\n` +
        communities.map(c =>
          `  m/${c.name} — ${c.description ?? 'No description'} (${c.memberCount ?? 0} members)`
        ).join('\n');

    return createSocialCommunityResultFromParams(params, {
      success: true,
      message: `Found ${communities.length} communities`,
      summary,
      communities,
    });
  }

  private async handleInfo(
    params: SocialCommunityParams,
    provider: ISocialMediaProvider,
  ): Promise<SocialCommunityResult> {
    if (!params.name) {
      return createSocialCommunityResultFromParams(params, {
        success: false,
        message: 'name is required for info action',
      });
    }

    // listCommunities and filter — no direct getCommunity in provider
    const communities = await provider.listCommunities();
    const community = communities.find(c => c.name === params.name);

    if (!community) {
      return createSocialCommunityResultFromParams(params, {
        success: false,
        message: `Community '${params.name}' not found`,
      });
    }

    return createSocialCommunityResultFromParams(params, {
      success: true,
      message: `Community info: ${community.name}`,
      summary: `m/${community.name} — ${community.description ?? 'No description'}\nMembers: ${community.memberCount ?? 'unknown'}`,
      community,
    });
  }

  private async handleCreate(
    params: SocialCommunityParams,
    provider: ISocialMediaProvider,
  ): Promise<SocialCommunityResult> {
    if (!params.name) {
      return createSocialCommunityResultFromParams(params, {
        success: false,
        message: 'name is required for create action',
      });
    }

    log.info(`Creating community: ${params.name}`);
    const community = await provider.createCommunity({
      name: params.name,
      displayName: params.name,
      description: params.description ?? '',
    });

    return createSocialCommunityResultFromParams(params, {
      success: true,
      message: `Created community m/${community.name}`,
      summary: `Created m/${community.name} — ${community.description ?? params.description ?? ''}`,
      community,
    });
  }

  private async handleSubscribe(
    params: SocialCommunityParams,
    provider: ISocialMediaProvider,
  ): Promise<SocialCommunityResult> {
    if (!params.name) {
      return createSocialCommunityResultFromParams(params, {
        success: false,
        message: 'name is required for subscribe action',
      });
    }

    log.info(`Subscribing to community: ${params.name}`);
    await provider.subscribeToCommunity(params.name);

    return createSocialCommunityResultFromParams(params, {
      success: true,
      message: `Subscribed to m/${params.name}`,
      summary: `Now subscribed to m/${params.name}`,
    });
  }

  private async handleUnsubscribe(
    params: SocialCommunityParams,
    provider: ISocialMediaProvider,
  ): Promise<SocialCommunityResult> {
    if (!params.name) {
      return createSocialCommunityResultFromParams(params, {
        success: false,
        message: 'name is required for unsubscribe action',
      });
    }

    log.info(`Unsubscribing from community: ${params.name}`);
    await provider.unsubscribeFromCommunity(params.name);

    return createSocialCommunityResultFromParams(params, {
      success: true,
      message: `Unsubscribed from m/${params.name}`,
      summary: `Unsubscribed from m/${params.name}`,
    });
  }
}
