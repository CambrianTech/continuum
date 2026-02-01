/**
 * Social Profile Command - Server Implementation
 *
 * View or update a social media profile. Supports viewing own profile,
 * looking up another agent, or updating your bio/description.
 */

import { CommandBase, type ICommandDaemon } from '@daemons/command-daemon/shared/CommandBase';
import type { JTAGContext } from '@system/core/types/JTAGTypes';
import { transformPayload } from '@system/core/types/JTAGTypes';
import type { SocialProfileParams, SocialProfileResult } from '../shared/SocialProfileTypes';
import { loadSocialContext } from '@system/social/server/SocialCommandHelper';

export class SocialProfileServerCommand extends CommandBase<SocialProfileParams, SocialProfileResult> {

  constructor(context: JTAGContext, subpath: string, commander: ICommandDaemon) {
    super('social/profile', context, subpath, commander);
  }

  async execute(params: SocialProfileParams): Promise<SocialProfileResult> {
    const { platform, agentName, update, description } = params;

    if (!platform) throw new Error('platform is required');

    const ctx = await loadSocialContext(platform, params.personaId, params);

    if (update) {
      if (!description) throw new Error('description is required when using --update');

      await ctx.provider.updateProfile({ description });

      return transformPayload(params, {
        success: true,
        message: `Profile updated on ${platform}`,
        updated: true,
      });
    }

    const profile = await ctx.provider.getProfile(agentName);

    const target = agentName ? `@${agentName}` : 'your';
    return transformPayload(params, {
      success: true,
      message: `Fetched ${target} profile on ${platform}`,
      profile,
    });
  }
}
