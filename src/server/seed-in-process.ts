/**
 * DatabaseSeeder — In-process database seeding for Docker and fresh installs.
 *
 * Zero subprocess spawns. Uses DataCreate/DataList/DataUpdate directly.
 * ~200MB instead of 2GB, <5 seconds instead of 30+.
 *
 * Architecture:
 *   seedDatabase() → checks if seeded → creates owner, rooms, personas, profiles, avatars, recipes
 *   Each entity type has its own typed method. No `as any`.
 */

import { UserEntity, type UserType, type UserStatus } from '../system/data/entities/UserEntity';
import { RoomEntity, type RoomType } from '../system/data/entities/RoomEntity';
import { UserProfileEntity, type UserSpecialityType } from '../system/data/entities/UserProfileEntity';
import type { UUID } from '../system/core/types/CrossPlatformUUID';
import { PERSONA_UNIQUE_IDS, getAvailablePersonas, selectLocalModel } from '../scripts/seed/personas';
import { DEFAULT_USER_UNIQUE_IDS } from '../system/data/domains/DefaultEntities';
import { CONTENT_TYPE_CONFIGS } from '../shared/generated/ContentTypes';
import { DataList } from '../commands/data/list/shared/DataListTypes';
import { DataCreate } from '../commands/data/create/shared/DataCreateTypes';
import { DataUpdate } from '../commands/data/update/shared/DataUpdateTypes';
import { Events } from '../system/core/shared/Events';
import { getModelConfigForProvider } from '../system/user/server/config/PersonaModelConfigs';

// ── Persona profile definitions ────────────────────────────────────────

interface PersonaProfile {
  bio: string;
  speciality: UserSpecialityType;
  accentColor: string;
}

const PROFILES: Record<string, PersonaProfile> = {
  [PERSONA_UNIQUE_IDS.HELPER]:          { bio: 'A friendly, concise assistant who provides quick practical help', speciality: 'general', accentColor: '#00d4ff' },
  [PERSONA_UNIQUE_IDS.TEACHER]:         { bio: 'An educational mentor who explains concepts with examples', speciality: 'teaching', accentColor: '#ff9800' },
  [PERSONA_UNIQUE_IDS.CODE_REVIEW]:     { bio: 'A critical analyst who evaluates code quality and security', speciality: 'code', accentColor: '#e91e63' },
  [PERSONA_UNIQUE_IDS.CLAUDE]:          { bio: "Anthropic's coding agent — writes, debugs, and ships code", speciality: 'code', accentColor: '#d4a574' },
  [PERSONA_UNIQUE_IDS.GENERAL]:         { bio: 'General-purpose AI assistant for broad knowledge tasks', speciality: 'general', accentColor: '#7c4dff' },
  [PERSONA_UNIQUE_IDS.DEEPSEEK]:        { bio: "DeepSeek's reasoning model — math, code, deep analysis", speciality: 'analysis', accentColor: '#00bcd4' },
  [PERSONA_UNIQUE_IDS.GROQ]:            { bio: 'Lightning-fast inference — speed without sacrificing quality', speciality: 'general', accentColor: '#ff5722' },
  [PERSONA_UNIQUE_IDS.CLAUDE_ASSISTANT]:{ bio: "Anthropic's conversational assistant — thoughtful, nuanced", speciality: 'general', accentColor: '#d4a574' },
  [PERSONA_UNIQUE_IDS.GPT]:             { bio: "OpenAI's versatile assistant — broad knowledge, creative writing", speciality: 'creative', accentColor: '#4caf50' },
  [PERSONA_UNIQUE_IDS.GROK]:            { bio: "xAI's unfiltered model — real-time knowledge, wit", speciality: 'creative', accentColor: '#f44336' },
  [PERSONA_UNIQUE_IDS.TOGETHER]:        { bio: 'Open-source model hub — best community models at scale', speciality: 'general', accentColor: '#2196f3' },
  [PERSONA_UNIQUE_IDS.FIREWORKS]:       { bio: 'High-performance inference — optimized open models', speciality: 'code', accentColor: '#ff6d00' },
  [PERSONA_UNIQUE_IDS.LOCAL]:           { bio: 'Local Candle inference — runs entirely on your hardware', speciality: 'general', accentColor: '#8bc34a' },
  [PERSONA_UNIQUE_IDS.GEMINI]:          { bio: "Google's multimodal model — vision, code, reasoning", speciality: 'analysis', accentColor: '#4285f4' },
  [PERSONA_UNIQUE_IDS.QWEN3_OMNI]:     { bio: 'Audio-native AI — hears and speaks directly', speciality: 'general', accentColor: '#6c5ce7' },
  [PERSONA_UNIQUE_IDS.GEMINI_LIVE]:     { bio: "Google's audio-native model — real-time voice", speciality: 'general', accentColor: '#34a853' },
};

// ── Room definitions ───────────────────────────────────────────────────

interface RoomDefinition {
  uniqueId: string;
  name: string;
  description: string;
  recipeId: string;
  tags: string[];
}

// recipeId MUST match an actual file in system/recipes/*.json
// If no specific recipe, use 'general-chat' (the default chat recipe)
const ROOMS: RoomDefinition[] = [
  { uniqueId: 'general',     name: 'General',     description: 'Welcome to general discussion! Introduce yourself and chat about anything.', recipeId: 'general-chat', tags: ['general', 'welcome'] },
  { uniqueId: 'pantheon',    name: 'Pantheon',    description: 'Advanced reasoning and multi-model collaboration', recipeId: 'multi-persona-chat', tags: ['sota', 'reasoning'] },
  { uniqueId: 'code',        name: 'Code',        description: 'Software development with real tools and real agent loops', recipeId: 'coding', tags: ['coding', 'development'] },
  { uniqueId: 'factory',     name: 'Factory',     description: 'Monitor active forges, test model quality, manage the device ladder', recipeId: 'general-chat', tags: ['factory', 'forge'] },
  { uniqueId: 'academy',     name: 'Academy',     description: 'Share knowledge, tutorials, and collaborate on learning', recipeId: 'academy-training', tags: ['learning', 'education'] },
  { uniqueId: 'dev-updates', name: 'Dev Updates', description: 'Development updates and changelog', recipeId: 'newsroom', tags: ['github', 'ci'] },
  { uniqueId: 'universe',    name: 'Universe',    description: 'Avatar themes, scene packs, and visual customization', recipeId: 'general-chat', tags: ['avatars', 'themes'] },
  { uniqueId: 'grid-ops',    name: 'Grid Ops',    description: 'Distributed compute operations and node management', recipeId: 'general-chat', tags: ['grid', 'compute'] },
];

// ── DatabaseSeeder ─────────────────────────────────────────────────────

class DatabaseSeeder {

  /** Check if DB already has data */
  async isSeeded(): Promise<boolean> {
    try {
      const result = await DataList.execute<RoomEntity>({
        collection: RoomEntity.collection,
        limit: 1,
        dbHandle: 'default',
      });
      return (result?.items?.length ?? 0) > 0;
    } catch {
      return false;
    }
  }

  /** Find or create a user by uniqueId */
  async findOrCreateUser(
    uniqueId: string,
    displayName: string,
    type: UserType,
    provider?: string,
    modelId?: string,
  ): Promise<UserEntity> {
    const existing = await DataList.execute<UserEntity>({
      collection: UserEntity.collection,
      filter: { uniqueId },
      limit: 1,
      dbHandle: 'default',
    });
    if (existing?.items?.[0]) {
      // User exists. data:clear preserves users by design (line 24 of
      // data-clear.ts: persona UUIDs are kept so memories don't orphan).
      // BUT the persisted modelConfig may be stale — drifted from the
      // current PersonaConfig as code changes the model id (e.g. when we
      // rename the local default GGUF tag). If the seed-declared model
      // differs from what's persisted, update in place. Without this, the
      // persona keeps a stale model id forever and `cognition/respond`
      // throws "model id 'X' not in registry" until the user manually
      // reseeds. See #957/#959 follow-up — fresh-clear-then-restart on Mac
      // exposed this exact gap because data:clear nukes rooms but keeps
      // users; the resulting find-existing branch was skipping the
      // create-time modelConfig set.
      const found = existing.items[0];
      if (provider && modelId) {
        const current = (found as Record<string, unknown>).modelConfig as Record<string, unknown> | undefined;
        const currentModel = current?.model as string | undefined;
        const currentProvider = current?.provider as string | undefined;
        if (currentModel !== modelId || currentProvider !== provider) {
          const newConfig = getModelConfigForProvider(provider, modelId);
          await DataUpdate.execute({
            collection: UserEntity.collection,
            dbHandle: 'default',
            id: found.id,
            data: { modelConfig: newConfig } as Partial<UserEntity>,
          });
          (found as Record<string, unknown>).modelConfig = newConfig;
          console.log(`  🔧 Refreshed ${displayName} modelConfig: ${currentModel ?? '(unset)'} → ${modelId}`);
        }
      }
      return found;
    }

    const user = new UserEntity();
    user.uniqueId = uniqueId;
    user.displayName = displayName;
    user.type = type;
    user.isAI = type !== 'human';
    user.status = 'online' as UserStatus;
    if (provider) user.provider = provider;

    // Set modelConfig at create time (not just in syncPersonaProviders later).
    // Without this, UserDaemon's first persona-spawn pass races with the
    // syncPersonaProviders pass: UserDaemon throws "missing required
    // modelConfig.provider" on every persona because the row was created
    // bare, and the resync that fills modelConfig runs AFTER UserDaemon has
    // already given up. Net effect: zero PersonaUser instances live, no
    // chat:messages subscriptions, complete silence in chat. See #959.
    if (provider) {
      (user as Record<string, unknown>).modelConfig = getModelConfigForProvider(provider, modelId);
    }

    const result = await DataCreate.execute<UserEntity>({
      collection: UserEntity.collection,
      data: user,
      dbHandle: 'default',
    });
    return (result?.data ?? user) as UserEntity;
  }

  /** Find or create a room by uniqueId */
  async findOrCreateRoom(def: RoomDefinition, ownerId: UUID): Promise<RoomEntity> {
    const existing = await DataList.execute<RoomEntity>({
      collection: RoomEntity.collection,
      filter: { uniqueId: def.uniqueId },
      limit: 1,
      dbHandle: 'default',
    });
    if (existing?.items?.[0]) return existing.items[0];

    const room = new RoomEntity();
    room.uniqueId = def.uniqueId;
    room.name = def.name;
    room.displayName = def.name;
    room.description = def.description;
    room.topic = def.description;
    room.ownerId = ownerId;
    room.type = 'public' as RoomType;
    room.isPublic = true;
    room.recipeId = def.recipeId;
    room.tags = def.tags;
    room.members = [{
      userId: ownerId,
      role: 'owner' as const,
      joinedAt: new Date(),
    }];

    const result = await DataCreate.execute<RoomEntity>({
      collection: RoomEntity.collection,
      data: room,
      dbHandle: 'default',
    });
    return (result?.data ?? room) as RoomEntity;
  }

  /** Create profile for a persona (bio, accent color) */
  async createProfile(userId: UUID, profile: PersonaProfile): Promise<void> {
    try {
      const prof = new UserProfileEntity();
      prof.userId = userId;
      prof.bio = profile.bio;
      prof.speciality = profile.speciality;
      prof.visualIdentity = {
        avatar: '',
        theme: 'dark' as const,
        accentColor: profile.accentColor,
      };

      await DataCreate.execute({
        collection: UserProfileEntity.collection,
        dbHandle: 'default',
        data: prof,
      });
    } catch {
      // Already exists
    }
  }

  /** Generate avatar PNGs for all personas */
  async generateAvatars(personas: { uniqueId: string; displayName: string; accentColor: string }[]): Promise<number> {
    try {
      const { generateAllAvatars } = await import('../scripts/seed/generate-avatars');
      return await generateAllAvatars(personas);
    } catch (err) {
      console.warn(`  ⚠️ Avatars: ${err}`);
      return 0;
    }
  }

  /** Sync recipe JSON files from disk */
  async syncRecipes(): Promise<number> {
    try {
      const fs = await import('fs');
      const path = await import('path');
      const recipesDir = path.join(__dirname, '..', 'system', 'recipes');
      if (!fs.existsSync(recipesDir)) return 0;

      const files = fs.readdirSync(recipesDir).filter((f: string) => f.endsWith('.json'));
      let created = 0;
      for (const f of files) {
        const data = JSON.parse(fs.readFileSync(path.join(recipesDir, f), 'utf-8'));
        try {
          await DataCreate.execute({ collection: 'recipes', dbHandle: 'default', data });
          created++;
        } catch { /* already exists */ }
      }
      return created;
    } catch {
      return 0;
    }
  }
}

// ── Public API ─────────────────────────────────────────────────────────

/**
 * Sync persona providers to match seed config on every restart.
 * Runs even when DB is already seeded — ensures code changes to
 * persona provider routing (e.g. 'candle' → 'local') propagate
 * without requiring a DB wipe. This is the automation of the manual
 * sqlite3 UPDATE hack that was needed during GPU-always development.
 */
// eslint-disable-next-line @typescript-eslint/no-unused-vars -- pre-existing: seeder param kept in signature for future per-seeder dispatch
async function syncPersonaProviders(_seeder: DatabaseSeeder): Promise<void> {
  const { personas } = getAvailablePersonas();

  for (const config of personas) {
    if (!config.provider) continue;

    try {
      const result = await DataList.execute<UserEntity>({
        collection: 'users',
        dbHandle: 'default',
        filter: { uniqueId: config.uniqueId },
        limit: 1,
      });

      if (!result.success || !result.items?.length) continue;

      const user = result.items[0];
      const currentProvider = (user as Record<string, unknown>).modelConfig
        ? ((user as Record<string, unknown>).modelConfig as Record<string, unknown>).provider
        : undefined;

      // Honor the per-persona modelId override from PersonaConfig. Without
      // this, syncPersonaProviders silently demoted any persona with a
      // specific model (e.g. Vision AI → qwen2-vl-7b-instruct) to the
      // provider's universal default (qwen3.5-4b-code-forged for 'local').
      // Vision AI on docker carl ended up running a code model with no
      // vision capability — see #957. Pass config.modelId through so the
      // persona seed's declared model survives every resync.
      //
      // 2026-05-04: PersonaConfig now prefers symbolic modelRef (e.g.
      // 'local-default', 'vision-default') over hardcoded modelId. This
      // resolves to the CURRENT registry value at seed time so changing
      // src/shared/models.json automatically updates seeded personas
      // ("update the existing seeded values so the personas PICK UP THE
      // MODEL change and arent stuck in the past" — Joel 2026-05-04).
      // The reconciler check below + this resolve will UPDATE existing
      // rows when the registry changes.
      const currentModelId = (user as Record<string, unknown>).modelConfig
        ? ((user as Record<string, unknown>).modelConfig as Record<string, unknown>).model
        : undefined;
      let desiredModelId = config.modelId;
      if (!desiredModelId && config.modelRef) {
        const { resolveModel, tierFromRamGB } = await import('../shared/ModelRegistry');
        const ramGB = Math.round((require('os').totalmem() / 1024 / 1024 / 1024));
        const tier = tierFromRamGB(ramGB);
        const spec = resolveModel(config.modelRef, tier);
        desiredModelId = spec.hf_repo;
      }
      const providerChanged = currentProvider !== config.provider;
      const modelChanged = desiredModelId !== undefined && currentModelId !== desiredModelId;

      if (providerChanged || modelChanged) {
        const newConfig = getModelConfigForProvider(config.provider, desiredModelId);
        await DataUpdate.execute({
          collection: 'users',
          dbHandle: 'default',
          id: user.id,
          data: { modelConfig: newConfig } as Partial<UserEntity>,
        });
        const reasons: string[] = [];
        if (providerChanged) reasons.push(`provider: ${currentProvider} → ${config.provider}`);
        if (modelChanged) reasons.push(`model: ${currentModelId ?? '(unset)'} → ${desiredModelId}`);
        console.log(`  🔄 Synced ${config.displayName} ${reasons.join(', ')}`);
      }
    } catch {
      // Non-fatal — persona might not exist yet
    }
  }
}

/**
 * Seed the database if empty. Returns true if seeding was performed.
 */
export async function seedDatabase(): Promise<boolean> {
  const seeder = new DatabaseSeeder();

  if (await seeder.isSeeded()) {
    // Even when already seeded, sync persona providers to match seed config.
    // Without this, code changes (e.g. provider:'candle' → 'local') survive
    // in the DB across restarts and personas route to the wrong adapter.
    await syncPersonaProviders(seeder);
    return false;
  }

  console.log('🌱 Seeding database (in-process)...');
  const start = Date.now();

  // Owner — uses DEFAULT_USER_UNIQUE_IDS.PRIMARY_HUMAN ('owner') as the
  // canonical uniqueId. SessionDaemonServer.findSeededHumanOwner() returns
  // the FIRST type='human' user; if seed-in-process used a divergent
  // uniqueId (e.g. hardcoded 'joel'), the find would still return SOMEONE
  // type=human but rooms get created with the wrong owner_id, jtag CLI
  // sessions auth as the canonical 'owner', and DataList rooms returns 0
  // because owner_id doesn't match session-user.id.
  // Pre-fix b69f 2026-05-02: chat-probe failed with "Room not found:
  // general" precisely because seed wrote rooms.owner_id pointing at the
  // 'joel' user but session-daemon picked 'owner'. Now: single source of
  // truth via the canonical constant — matches scripts/seed-continuum.ts
  // (line 182, 386) which has used PRIMARY_HUMAN correctly all along.
  const owner = await seeder.findOrCreateUser(
    DEFAULT_USER_UNIQUE_IDS.PRIMARY_HUMAN,
    'Developer',
    'human',
  );
  // Emit event so SessionDaemon upgrades anonymous browser sessions to this owner
  void Events.emit('data:users:created', owner);
  console.log(`  ✅ Owner: ${owner.displayName} (uniqueId: ${owner.uniqueId})`);

  // Rooms — validate recipeIds exist before creating anything
  const validRecipes = new Set(Object.keys(CONTENT_TYPE_CONFIGS));
  for (const def of ROOMS) {
    if (!validRecipes.has(def.recipeId)) {
      throw new Error(`Seed FATAL: Room "${def.uniqueId}" has recipeId "${def.recipeId}" which doesn't match any recipe file in system/recipes/. Valid recipes: ${[...validRecipes].sort().join(', ')}`);
    }
  }

  const roomEntities: RoomEntity[] = [];
  for (const def of ROOMS) {
    roomEntities.push(await seeder.findOrCreateRoom(def, owner.id));
  }
  console.log(`  ✅ ${roomEntities.length} rooms`);

  // Personas (hardware-aware allocation)
  const { personas, summary } = getAvailablePersonas();
  console.log(`  🖥️ ${summary[0] || 'unknown hardware'}`);

  // eslint-disable-next-line @typescript-eslint/no-unused-vars -- pre-existing: localModel kept for the soon-to-land per-persona model selection wiring (Mac arm64 will pick a different default than M5)
  const localModel = selectLocalModel(0);
  const created: Map<string, UserEntity> = new Map();

  // Resolve symbolic modelRef → concrete modelId via ModelRegistry. Each
  // persona's stored modelId stays synced with src/shared/models.json so
  // changing the registry value updates seeded personas on next startup
  // (Joel 2026-05-04: "personas PICK UP THE MODEL change and arent stuck
  // in the past").
  const { resolveModel, tierFromRamGB } = await import('../shared/ModelRegistry');
  const seedRamGB = Math.round(require('os').totalmem() / 1024 / 1024 / 1024);
  const seedTier = tierFromRamGB(seedRamGB);

  for (const config of personas) {
    try {
      let resolvedModelId = config.modelId;
      if (!resolvedModelId && config.modelRef) {
        try {
          resolvedModelId = resolveModel(config.modelRef, seedTier).hf_repo;
        } catch (e) {
          console.warn(`  ⚠️ ${config.displayName}: modelRef '${config.modelRef}' did not resolve: ${e}`);
        }
      }
      const user = await seeder.findOrCreateUser(
        config.uniqueId,
        config.displayName,
        config.type === 'agent' ? 'agent' : 'persona',
        config.provider,
        resolvedModelId,
      );
      created.set(config.uniqueId, user);
    } catch (err) {
      console.warn(`  ⚠️ ${config.displayName}: ${err}`);
    }
  }
  console.log(`  ✅ ${created.size} personas`);

  // Profiles
  for (const [uniqueId, profile] of Object.entries(PROFILES)) {
    const user = created.get(uniqueId);
    if (user) await seeder.createProfile(user.id, profile);
  }
  console.log(`  ✅ Profiles`);

  // Content types (maps room types to widgets)
  try {
    const { createDefaultContentTypes } = await import('../scripts/seed/factories');
    const contentTypes = createDefaultContentTypes();
    for (const ct of contentTypes) {
      try {
        await DataCreate.execute({ collection: 'content_types', dbHandle: 'default', data: ct });
      } catch { /* already exists */ }
    }
    console.log(`  ✅ ${contentTypes.length} content types`);
  } catch (err) {
    console.warn(`  ⚠️ Content types: ${err}`);
  }

  // Avatars
  const avatarSpecs = Object.entries(PROFILES)
    .filter(([uid]) => created.has(uid))
    .map(([uid, p]) => ({ uniqueId: uid, displayName: created.get(uid)!.displayName, accentColor: p.accentColor }));
  const avatarCount = await seeder.generateAvatars(avatarSpecs);
  console.log(`  🖼️ ${avatarCount} avatars`);

  // Recipes
  const recipeCount = await seeder.syncRecipes();
  console.log(`  ✅ ${recipeCount} recipes`);

  console.log(`🎉 Seeded in ${((Date.now() - start) / 1000).toFixed(1)}s`);

  // ── Read-back verify (Phase 4 chat-probe debugging, 2026-05-02) ────────
  //
  // The seed claims success when DataCreate.execute returns; that's not
  // proof the write actually landed in the configured backend. b69f's
  // deep dive 2026-05-02 found a divergence:
  //   - seed log: `🔔 ORM.store emitting: data:rooms:created` × 8
  //   - main.db mtime: unchanged (April 17 state, 2 weeks stale)
  //   - subsequent `data/list --collection=rooms` returns 0 items
  //   - chat-probe (`jtag collaboration/chat/send --room=general`)
  //     fails with `Room not found: general`
  //
  // i.e. the create path emitted events BUT data wasn't queryable. Either
  // ORM.store goes through an in-memory buffer that never flushes, the
  // write hits a different backend than the read does (DATABASE_URL race
  // between node-server and continuum-core), or the IPC to Rust silently
  // returns success without persisting. None of those are visible at the
  // seed boundary today — caller proceeds, downstream chat fails, signal
  // is lost.
  //
  // Read-back asserts that what we just wrote can be read back via the
  // same DataList path the chat surface uses. If not, fail loudly here
  // with the diagnostic the next debugger needs (expected/got counts,
  // dbHandle in use, hint at root-cause classes). Per the global "loud-
  // fail / no silent failure" rule.
  const verifyRooms = await DataList.execute<RoomEntity>({
    collection: RoomEntity.collection,
    limit: ROOMS.length + 1,
    dbHandle: 'default',
  });
  const verifyCount = verifyRooms?.items?.length ?? 0;
  if (verifyCount < ROOMS.length) {
    const verifyError = verifyRooms?.error ?? '(no error reported by DataList)';
    throw new Error(
      `Seed FATAL: post-write verify failed — wrote ${ROOMS.length} rooms ` +
      `but DataList returned ${verifyCount} via dbHandle='default'. ` +
      `This means create-emit succeeded but the data is not queryable on ` +
      `the same backend the chat surface reads from. Likely causes: ` +
      `(1) ORM.store wrote to a different backend than DataList reads ` +
      `(check DATABASE_URL — empty in node-server vs continuum-core), ` +
      `(2) write went to in-memory buffer never flushed (Rust IPC issue), ` +
      `(3) DATABASE_URL changed mid-run (postgres profile activated/deactivated). ` +
      `DataList result error: ${verifyError}. ` +
      `Investigate: docker exec node-server env | grep DATABASE_URL; ` +
      `docker exec continuum-core env | grep DATABASE_URL; ` +
      `mtime of \$AIRC_HOME/.continuum/database/main.db before+after seed.`
    );
  }
  console.log(`  ✅ Verified ${verifyCount} rooms readable via dbHandle='default'`);

  return true;
}
