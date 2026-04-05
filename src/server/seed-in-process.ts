/**
 * In-Process Database Seeder
 *
 * Replaces the subprocess-based seed-continuum.ts for Docker environments.
 * Uses ORM directly — zero subprocess spawns, ~200MB instead of 2GB.
 *
 * Called by docker-entrypoint.ts after server is ready.
 * All entities created via Commands.execute() which routes to the running
 * server's command handlers — same path as browser/CLI, but in-process.
 */

import { Commands } from '../system/core/shared/Commands';
import { UserEntity } from '../system/data/entities/UserEntity';
import { RoomEntity } from '../system/data/entities/RoomEntity';
import type { UUID } from '../system/core/types/CrossPlatformUUID';
import { PERSONA_CONFIGS, PERSONA_UNIQUE_IDS, getAvailablePersonas, selectLocalModel, type PersonaConfig } from '../scripts/seed/personas';
import { DataList } from '../commands/data/list/shared/DataListTypes';
import { DataCreate } from '../commands/data/create/shared/DataCreateTypes';
import { DataUpdate } from '../commands/data/update/shared/DataUpdateTypes';

// Profile data — same as seed-continuum.ts
const PERSONA_PROFILES: Record<string, { bio: string; speciality: string; accentColor: string }> = {
  [PERSONA_UNIQUE_IDS.HELPER]: { bio: 'A friendly, concise assistant who provides quick practical help', speciality: 'practical-assistance', accentColor: '#00d4ff' },
  [PERSONA_UNIQUE_IDS.TEACHER]: { bio: 'An educational mentor who explains concepts with examples', speciality: 'education-mentoring', accentColor: '#ff9800' },
  [PERSONA_UNIQUE_IDS.CODE_REVIEW]: { bio: 'A critical analyst who evaluates code quality and security', speciality: 'code-analysis', accentColor: '#e91e63' },
  [PERSONA_UNIQUE_IDS.CLAUDE]: { bio: "Anthropic's coding agent — writes, debugs, and ships code", speciality: 'code', accentColor: '#d4a574' },
  [PERSONA_UNIQUE_IDS.GENERAL]: { bio: 'General-purpose AI assistant for broad knowledge tasks', speciality: 'general', accentColor: '#7c4dff' },
  [PERSONA_UNIQUE_IDS.DEEPSEEK]: { bio: "DeepSeek's reasoning model — math, code, deep analysis", speciality: 'analysis', accentColor: '#00bcd4' },
  [PERSONA_UNIQUE_IDS.GROQ]: { bio: 'Lightning-fast inference — speed without sacrificing quality', speciality: 'general', accentColor: '#ff5722' },
  [PERSONA_UNIQUE_IDS.CLAUDE_ASSISTANT]: { bio: "Anthropic's conversational assistant — thoughtful, nuanced", speciality: 'general', accentColor: '#d4a574' },
  [PERSONA_UNIQUE_IDS.GPT]: { bio: "OpenAI's versatile assistant — broad knowledge, creative writing", speciality: 'creative', accentColor: '#4caf50' },
  [PERSONA_UNIQUE_IDS.GROK]: { bio: "xAI's unfiltered model — real-time knowledge, wit", speciality: 'creative', accentColor: '#f44336' },
  [PERSONA_UNIQUE_IDS.TOGETHER]: { bio: 'Open-source model hub — best community models at scale', speciality: 'general', accentColor: '#2196f3' },
  [PERSONA_UNIQUE_IDS.FIREWORKS]: { bio: 'High-performance inference — optimized open models', speciality: 'code', accentColor: '#ff6d00' },
  [PERSONA_UNIQUE_IDS.LOCAL]: { bio: 'Local Candle inference — runs entirely on your hardware', speciality: 'general', accentColor: '#8bc34a' },
  [PERSONA_UNIQUE_IDS.GEMINI]: { bio: "Google's multimodal model — vision, code, reasoning", speciality: 'analysis', accentColor: '#4285f4' },
  [PERSONA_UNIQUE_IDS.QWEN3_OMNI]: { bio: 'Audio-native AI — hears and speaks directly', speciality: 'voice-conversation', accentColor: '#6c5ce7' },
  [PERSONA_UNIQUE_IDS.GEMINI_LIVE]: { bio: "Google's audio-native model — real-time voice", speciality: 'voice-conversation', accentColor: '#34a853' },
};

/**
 * Seed the database in-process. Zero subprocesses.
 * Returns true if seeding was performed, false if already seeded.
 */
export async function seedDatabase(): Promise<boolean> {
  // Check if already seeded
  try {
    const result = await DataList.execute<RoomEntity>({
      collection: RoomEntity.collection,
      limit: 1,
      dbHandle: 'default',
    });
    if (result?.items?.length > 0) {
      return false; // Already seeded
    }
  } catch {
    // DB might not be ready yet — proceed with seeding
  }

  console.log('🌱 Seeding database (in-process)...');
  const start = Date.now();

  // 1. Create human owner
  const humanUser = await createUser('joel', 'Developer', 'human');
  console.log(`  ✅ Owner: ${humanUser.displayName} (${humanUser.uniqueId})`);

  // 2. Create rooms
  const rooms = [
    { uniqueId: 'general', displayName: 'General', description: 'Welcome to general discussion! Introduce yourself and chat about anything.' },
    { uniqueId: 'pantheon', displayName: 'Pantheon', description: 'Advanced reasoning and multi-model collaboration' },
    { uniqueId: 'code', displayName: 'Code', description: 'Software development with real tools and real agent loops' },
    { uniqueId: 'factory', displayName: 'Factory', description: 'Monitor active forges, test model quality, manage the device ladder' },
    { uniqueId: 'academy', displayName: 'Academy', description: 'Share knowledge, tutorials, and collaborate on learning' },
    { uniqueId: 'dev-updates', displayName: 'Dev Updates', description: 'Development updates and changelog' },
    { uniqueId: 'universe', displayName: 'Universe', description: 'Avatar themes, scene packs, and visual customization' },
    { uniqueId: 'grid-ops', displayName: 'Grid Ops', description: 'Distributed compute operations and node management' },
  ];

  const roomEntities: RoomEntity[] = [];
  for (const r of rooms) {
    const room = await createRoom(r.uniqueId, r.displayName, r.description, humanUser.id);
    roomEntities.push(room);
  }
  console.log(`  ✅ ${roomEntities.length} rooms`);

  // 3. Create personas
  const { personas, summary } = getAvailablePersonas();
  const gpu = summary[0] || 'unknown';
  console.log(`  🖥️ ${gpu}`);

  const localModel = selectLocalModel(0); // Will be overridden per-persona
  const createdPersonas: Map<string, UserEntity> = new Map();

  for (const config of personas) {
    try {
      const user = await createUser(
        config.uniqueId,
        config.displayName,
        config.type === 'agent' ? 'agent' : 'persona',
        config.provider,
        config.modelId || (config.provider === 'candle' ? localModel : undefined),
      );
      createdPersonas.set(config.uniqueId, user);

      // Add to general room
      const generalRoom = roomEntities.find(r => r.uniqueId === 'general');
      if (generalRoom) {
        await addMemberToRoom(generalRoom.id, user.id);
      }
    } catch (err) {
      console.warn(`  ⚠️ ${config.displayName}: ${err}`);
    }
  }
  console.log(`  ✅ ${createdPersonas.size} personas`);

  // 4. Persona profiles (bios, accent colors)
  for (const [uniqueId, profile] of Object.entries(PERSONA_PROFILES)) {
    const user = createdPersonas.get(uniqueId);
    if (!user) continue;
    try {
      await DataCreate.execute({
        collection: 'user_profiles',
        dbHandle: 'default',
        data: {
          userId: user.id,
          bio: profile.bio,
          speciality: profile.speciality,
          visualIdentity: {
            avatar: '',
            theme: 'dark',
            accentColor: profile.accentColor,
          },
          preferences: {
            language: 'en',
            timezone: 'UTC',
            notifications: { mentions: true, directMessages: true, roomUpdates: false },
            privacy: { showOnlineStatus: true, allowDirectMessages: true, shareActivity: false },
          },
        },
      });
    } catch {
      // Profile may already exist — fine
    }
  }
  console.log(`  ✅ Persona profiles`);

  // 5. Generate avatar PNGs
  try {
    const { generateAllAvatars } = await import('../scripts/seed/generate-avatars');
    const avatarSpecs = Object.entries(PERSONA_PROFILES).map(([uniqueId, profile]) => {
      const persona = personas.find(p => p.uniqueId === uniqueId);
      return { uniqueId, displayName: persona?.displayName || uniqueId, accentColor: profile.accentColor };
    });
    const count = await generateAllAvatars(avatarSpecs);
    console.log(`  🖼️ ${count} avatars generated`);
  } catch (err) {
    console.warn(`  ⚠️ Avatars: ${err}`);
  }

  // 6. Sync recipes from JSON files
  try {
    const fs = await import('fs');
    const path = await import('path');
    const recipesDir = path.join(__dirname, '..', 'system', 'recipes');
    if (fs.existsSync(recipesDir)) {
      const files = fs.readdirSync(recipesDir).filter((f: string) => f.endsWith('.json'));
      let created = 0;
      for (const f of files) {
        const data = JSON.parse(fs.readFileSync(path.join(recipesDir, f), 'utf-8'));
        try {
          await DataCreate.execute({ collection: 'recipes', dbHandle: 'default', data });
          created++;
        } catch { /* already exists */ }
      }
      console.log(`  ✅ ${created} recipes`);
    }
  } catch (err) {
    console.warn(`  ⚠️ Recipes: ${err}`);
  }

  const elapsed = Date.now() - start;
  console.log(`🎉 Seeded in ${(elapsed / 1000).toFixed(1)}s`);
  return true;
}

// ── Helpers ────────────────────────────────────────

async function createUser(
  uniqueId: string,
  displayName: string,
  type: 'human' | 'persona' | 'agent',
  provider?: string,
  modelId?: string,
): Promise<UserEntity> {
  // Check if exists first
  const existing = await DataList.execute<UserEntity>({
    collection: UserEntity.collection,
    filter: { uniqueId },
    limit: 1,
    dbHandle: 'default',
  });
  if (existing?.items?.[0]) {
    return existing.items[0];
  }

  // Create new
  const user = new UserEntity();
  user.uniqueId = uniqueId;
  user.displayName = displayName;
  user.type = type as any;
  user.isAI = type !== 'human';
  user.status = 'online';
  if (provider) (user as any).provider = provider;
  if (modelId) (user as any).modelId = modelId;

  const result = await DataCreate.execute<UserEntity>({
    collection: UserEntity.collection,
    data: user,
    dbHandle: 'default',
  });

  return (result?.data || user) as UserEntity;
}

async function createRoom(
  uniqueId: string,
  displayName: string,
  description: string,
  ownerId: UUID,
): Promise<RoomEntity> {
  // Check if exists
  const existing = await DataList.execute<RoomEntity>({
    collection: RoomEntity.collection,
    filter: { uniqueId },
    limit: 1,
    dbHandle: 'default',
  });
  if (existing?.items?.[0]) {
    return existing.items[0];
  }

  const room = new RoomEntity();
  room.uniqueId = uniqueId;
  room.displayName = displayName;
  room.description = description;
  room.ownerId = ownerId;
  room.type = 'room' as any;
  room.isPublic = true;
  room.members = [{ userId: ownerId, role: 'owner', joinedAt: new Date().toISOString() } as any];

  const result = await DataCreate.execute<RoomEntity>({
    collection: RoomEntity.collection,
    data: room,
    dbHandle: 'default',
  });

  return (result?.data || room) as RoomEntity;
}

async function addMemberToRoom(roomId: UUID, userId: UUID): Promise<void> {
  try {
    await DataUpdate.execute<RoomEntity>({
      collection: RoomEntity.collection,
      id: roomId,
      dbHandle: 'default',
      data: {
        $push: {
          members: { userId, role: 'member', joinedAt: new Date().toISOString() },
        },
      } as any,
    });
  } catch {
    // Member might already exist
  }
}
