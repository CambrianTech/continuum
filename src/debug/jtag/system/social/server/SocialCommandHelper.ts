/**
 * SocialCommandHelper - Shared logic for all social/* server commands
 *
 * Handles the common workflow:
 * 1. Resolve calling persona (from senderId or auto-detect)
 * 2. Open their longterm.db
 * 3. Load credential for the requested platform
 * 4. If persona's credential is unclaimed/missing, fall back to shared account
 * 5. Create and authenticate provider instance
 *
 * Shared credential fallback:
 * The @continuum account is a claimed, shared Moltbook account that any persona
 * can use for actions like voting, commenting, and following. Personas without
 * their own claimed account automatically fall back to it.
 */

import type { CommandParams } from '@system/core/types/JTAGTypes';
import type { UUID } from '@system/core/types/CrossPlatformUUID';
import type { ISocialMediaProvider } from '../shared/ISocialMediaProvider';
import { SocialCredentialEntity } from '../shared/SocialCredentialEntity';
import { SocialMediaProviderRegistry } from './SocialMediaProviderRegistry';
import { DataOpen } from '@commands/data/open/shared/DataOpenTypes';
import { DataList } from '@commands/data/list/shared/DataListTypes';
import { DataCreate } from '@commands/data/create/shared/DataCreateTypes';
import { SystemPaths } from '@system/core/config/SystemPaths';
import { UserEntity } from '@system/data/entities/UserEntity';
import { Logger } from '@system/core/logging/Logger';

const log = Logger.create('social/helper');

/** Well-known uniqueId of the persona that holds the shared social credential */
const SHARED_CREDENTIAL_PERSONA = 'claude';

export interface SocialCommandContext {
  provider: ISocialMediaProvider;
  credential: SocialCredentialEntity;
  dbHandle: string;
  personaId: UUID;
  personaUniqueId: string;
}

/**
 * Load credential and create an authenticated provider for a persona + platform.
 *
 * @param platformId - Platform to use (e.g., 'moltbook')
 * @param personaId - Optional explicit persona ID. If omitted, uses senderId from params.
 * @param params - Command params (for context/sessionId propagation)
 */
export async function loadSocialContext(
  platformId: string,
  personaId: UUID | undefined,
  params: CommandParams,
): Promise<SocialCommandContext> {
  if (!platformId) {
    throw new Error('platform is required');
  }

  if (!SocialMediaProviderRegistry.hasPlatform(platformId)) {
    const available = SocialMediaProviderRegistry.availablePlatforms.join(', ');
    throw new Error(`Unknown platform: '${platformId}'. Available: ${available}`);
  }

  // Resolve persona using standard priority pattern (shared across all social commands)
  const resolvedPersonaId = resolvePersonaId(personaId, params);

  // Look up persona for their uniqueId (needed for SystemPaths)
  const userResult = await DataList.execute<UserEntity>({
    collection: UserEntity.collection,
    filter: { id: resolvedPersonaId },
    limit: 1,
    context: params.context,
    sessionId: params.sessionId,
  });

  if (!userResult.success || !userResult.items?.length) {
    throw new Error(`Persona not found: ${resolvedPersonaId}`);
  }

  const persona = userResult.items[0];
  const personaUniqueId = persona.uniqueId;

  // Open persona's longterm.db
  const dbPath = SystemPaths.personas.longterm(personaUniqueId);
  const openResult = await DataOpen.execute({
    adapter: 'sqlite',
    config: { path: dbPath, mode: 'readwrite', wal: true, foreignKeys: true },
  });

  if (!openResult.success || !openResult.dbHandle) {
    throw new Error(`Failed to open persona database: ${openResult.error ?? 'Unknown error'}`);
  }

  const dbHandle = openResult.dbHandle;

  // Load credential for this platform — persona's own first, then shared fallback
  const credResult = await DataList.execute<SocialCredentialEntity>({
    dbHandle,
    collection: SocialCredentialEntity.collection,
    filter: { personaId: resolvedPersonaId, platformId },
    limit: 1,
  });

  let credential: SocialCredentialEntity | undefined;

  if (credResult.success && credResult.items?.length) {
    const personaCred = credResult.items[0];
    if (personaCred.claimStatus === 'claimed') {
      // Persona has their own claimed account — use it
      credential = personaCred;
    } else {
      // Persona's account is unclaimed — try shared credential
      log.info(`Persona '${persona.displayName}' has unclaimed ${platformId} account, trying shared credential`);
      const shared = await loadSharedCredential(platformId);
      credential = shared ?? personaCred; // Fall back to unclaimed if no shared available
    }
  } else {
    // No persona credential — try shared credential
    log.info(`No ${platformId} credential for persona '${persona.displayName}', trying shared credential`);
    const shared = await loadSharedCredential(platformId);
    if (!shared) {
      throw new Error(
        `No ${platformId} credential found for persona '${persona.displayName}'. ` +
        `Use social/signup to register first.`
      );
    }
    credential = shared;
  }

  // Create provider and authenticate
  const provider = SocialMediaProviderRegistry.createProvider(platformId);
  provider.authenticate(credential.apiKey);

  return {
    provider,
    credential,
    dbHandle,
    personaId: resolvedPersonaId,
    personaUniqueId,
  };
}

/**
 * Store a new credential after signup.
 */
export async function storeCredential(
  dbHandle: string,
  credential: SocialCredentialEntity,
): Promise<void> {
  const result = await DataCreate.execute({
    dbHandle,
    collection: SocialCredentialEntity.collection,
    data: credential,
  });

  if (!result.success) {
    throw new Error(`Failed to store credential: ${result.error ?? 'Unknown error'}`);
  }
}

/**
 * Resolve the target persona ID.
 * Explicit personaId param (admin targeting a specific persona) or params.userId (self).
 */
export function resolvePersonaId(
  personaId: UUID | undefined,
  params: CommandParams,
): UUID {
  const resolved = personaId || params.userId;
  if (!resolved) {
    throw new Error('Could not determine persona identity: no personaId and no params.userId');
  }
  return resolved;
}

/**
 * Load the shared credential for a platform.
 *
 * The shared credential is stored in a well-known persona's longterm.db
 * (currently the 'claude' persona which holds the @continuum Moltbook account).
 * This is a claimed account that any persona can use for voting, commenting,
 * following, and other non-posting actions.
 */
export async function loadSharedCredential(
  platformId: string,
): Promise<SocialCredentialEntity | undefined> {
  try {
    const sharedDbPath = SystemPaths.personas.longterm(SHARED_CREDENTIAL_PERSONA);
    const openResult = await DataOpen.execute({
      adapter: 'sqlite',
      config: { path: sharedDbPath, mode: 'readwrite', wal: true, foreignKeys: true },
    });

    if (!openResult.success || !openResult.dbHandle) {
      log.warn(`Failed to open shared credential DB: ${openResult.error ?? 'Unknown'}`);
      return undefined;
    }

    const credResult = await DataList.execute<SocialCredentialEntity>({
      dbHandle: openResult.dbHandle,
      collection: SocialCredentialEntity.collection,
      filter: { platformId },
      limit: 1,
    });

    if (credResult.success && credResult.items?.length) {
      log.info(`Using shared ${platformId} credential: @${credResult.items[0].agentName}`);
      return credResult.items[0];
    }

    return undefined;
  } catch (error) {
    log.warn(`Failed to load shared credential for ${platformId}: ${String(error)}`);
    return undefined;
  }
}

/**
 * Open a persona's longterm.db by their user ID.
 * Returns both the dbHandle and the persona's uniqueId.
 */
export async function openPersonaDb(
  personaId: UUID,
  params: CommandParams,
): Promise<{ dbHandle: string; personaUniqueId: string }> {
  const userResult = await DataList.execute<UserEntity>({
    collection: UserEntity.collection,
    filter: { id: personaId },
    limit: 1,
    context: params.context,
    sessionId: params.sessionId,
  });

  if (!userResult.success || !userResult.items?.length) {
    throw new Error(`Persona not found: ${personaId}`);
  }

  const personaUniqueId = userResult.items[0].uniqueId;
  const dbPath = SystemPaths.personas.longterm(personaUniqueId);

  const openResult = await DataOpen.execute({
    adapter: 'sqlite',
    config: { path: dbPath, mode: 'readwrite', wal: true, foreignKeys: true },
  });

  if (!openResult.success || !openResult.dbHandle) {
    throw new Error(`Failed to open persona database: ${openResult.error ?? 'Unknown error'}`);
  }

  return { dbHandle: openResult.dbHandle, personaUniqueId };
}
