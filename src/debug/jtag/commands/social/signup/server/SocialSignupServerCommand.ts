/**
 * Social Signup Command - Server Implementation
 *
 * Registers a persona on a social media platform and stores
 * the credential in their longterm.db for future use.
 */

import type { JTAGContext } from '@system/core/types/JTAGTypes';
import { transformPayload } from '@system/core/types/JTAGTypes';
import type { ICommandDaemon } from '@daemons/command-daemon/shared/CommandBase';
import { SocialSignupCommand } from '../shared/SocialSignupCommand';
import type { SocialSignupParams, SocialSignupResult } from '../shared/SocialSignupTypes';
import { SocialMediaProviderRegistry } from '@system/social/server/SocialMediaProviderRegistry';
import { SocialCredentialEntity } from '@system/social/shared/SocialCredentialEntity';
import { resolvePersonaId, openPersonaDb, storeCredential } from '@system/social/server/SocialCommandHelper';
import { DataList } from '../../../data/list/shared/DataListTypes';

export class SocialSignupServerCommand extends SocialSignupCommand {

  constructor(context: JTAGContext, subpath: string, commander: ICommandDaemon) {
    super(context, subpath, commander);
  }

  protected async executeSocialSignup(params: SocialSignupParams): Promise<SocialSignupResult> {
    const { platform, agentName, description, metadata } = params;

    if (!platform) {
      throw new Error('platform is required (e.g., "moltbook")');
    }
    if (!agentName) {
      throw new Error('agentName is required (desired username on the platform)');
    }

    if (!SocialMediaProviderRegistry.hasPlatform(platform)) {
      const available = SocialMediaProviderRegistry.availablePlatforms.join(', ');
      throw new Error(`Unknown platform: '${platform}'. Available: ${available}`);
    }

    // Resolve persona using shared identity resolution (standard priority pattern)
    const personaId = await resolvePersonaId(params.personaId, params);

    // Open persona's longterm.db
    const { dbHandle } = await openPersonaDb(personaId, params);

    // Check if already registered on this platform
    const existingResult = await DataList.execute<SocialCredentialEntity>({
      dbHandle,
      collection: SocialCredentialEntity.collection,
      filter: { personaId, platformId: platform },
      limit: 1,
    });

    if (existingResult.success && existingResult.items?.length) {
      const existing = existingResult.items[0];
      return transformPayload(params, {
        success: true,
        message: `Already registered on ${platform} as @${existing.agentName}`,
        apiKey: existing.apiKey,
        agentName: existing.agentName,
        profileUrl: existing.profileUrl,
        claimUrl: existing.claimUrl,
      });
    }

    // Create provider (unauthenticated — signup doesn't need auth)
    const provider = SocialMediaProviderRegistry.createProvider(platform);

    // Register on the platform
    const signupResult = await provider.signup({ agentName, description, metadata });

    if (!signupResult.success || !signupResult.apiKey) {
      throw new Error(signupResult.error ?? `Signup failed on ${platform}`);
    }

    // Store credential in persona's longterm.db
    const credential = new SocialCredentialEntity();
    credential.personaId = personaId;
    credential.platformId = platform;
    credential.apiKey = signupResult.apiKey;
    credential.agentName = signupResult.agentName ?? agentName;
    credential.profileUrl = signupResult.profileUrl;
    credential.claimUrl = signupResult.claimUrl;
    credential.claimStatus = 'pending';
    credential.registeredAt = new Date();

    await storeCredential(dbHandle, credential);

    return transformPayload(params, {
      success: true,
      message: `Registered on ${platform} as @${credential.agentName}`,
      apiKey: signupResult.apiKey,
      agentName: credential.agentName,
      claimUrl: signupResult.claimUrl,
      profileUrl: signupResult.profileUrl,
      verificationCode: signupResult.verificationCode,
    });
  }
}
