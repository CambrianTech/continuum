# Brain HUD - Unified Cognitive Interface

## Vision

One sci-fi brain visualization that serves as the complete interface to a persona's cognitive systems. The brain occupies the screen center, with functional regions radiating outward as HUD panels. Everything visible at once - no tab switching.

## Brain Region Mapping

| Region | Domain | Data | Commands |
|--------|--------|------|----------|
| **Hippocampus** | Memory | Semantic memories, RAG vectors, recall stats | `memory/stats`, `memory/search` |
| **Genome** | Adapters | LoRA stack, scales, base model, GPU usage | `genome/status`, `adapter/search`, `adapter/adopt` |
| **Motor Cortex** | Outputs | Actions, speech, video, game controls | `tools/list`, `audio/tts`, `video/generate` |
| **Sensory Cortex** | Inputs | Vision, audio, text understanding | `vision/describe`, `audio/transcribe` |
| **Prefrontal** | Logs | Activity stream, decisions, thought process | `logs/recent`, `logs/search` |
| **Limbic** | State | Energy, mood, attention, adaptive cadence | `persona/state` |
| **CNS** | Performance | Inference latency, connections, throughput | `inference/status`, `ping` |

## Layout

```
┌─────────────────────────────────────────────────────────────────────────────┐
│  HELPER AI                                              ● ONLINE    ⚡ READY │
├─────────────────────────────────────────────────────────────────────────────┤
│                                                                             │
│  ┌─────────────┐                                        ┌─────────────┐    │
│  │ PREFRONTAL  │                                        │   GENOME    │    │
│  │   [LOGS]    │                                        │  [ADAPTERS] │    │
│  │             │                                        │             │    │
│  │  Activity   │────○                            ○──────│ ts-expert   │    │
│  │  Stream     │     \                          /       │ ═══●═══ 0.8 │    │
│  └─────────────┘      \                        /        │ logic-v2    │    │
│                        \                      /         │ ═══●═══ 0.6 │    │
│  ┌─────────────┐        \    ┌────────┐      /          └─────────────┘    │
│  │ HIPPOCAMPUS │         \   │        │     /                              │
│  │  [MEMORY]   │          ○──│   🧠   │────○           ┌─────────────┐    │
│  │             │         /   │        │     \          │   LIMBIC    │    │
│  │    5,885    │────────○    └────────┘      ○─────────│   [STATE]   │    │
│  │    2.9 MB   │        │                    │         │             │    │
│  └─────────────┘        │                    │         │ Energy: 72% │    │
│                         │                    │         │ Mood: calm  │    │
│  ┌─────────────┐        │                    │         └─────────────┘    │
│  │MOTOR CORTEX │        │                    │                             │
│  │ [OUTPUTS]   │────────○                    ○─────────┌─────────────┐    │
│  │ 🗣️🎵🎬🎮   │                                        │   SENSORY   │    │
│  │  5 ENABLED  │                                        │  [INPUTS]   │    │
│  └─────────────┘                                        │ 👁️ 👂 📖    │    │
│                                                         │  3 ACTIVE   │    │
│                         ┌─────────────┐                 └─────────────┘    │
│                         │     CNS     │                                    │
│                         │   [PERF]    │                                    │
│                         │  45 tok/s   │                                    │
│                         └─────────────┘                                    │
│                                                                             │
├─────────────────────────────────────────────────────────────────────────────┤
│  GPU ████████░░░░ 5.2/8GB   MEM 2.9MB   OUT 5   IN 3   ADAPTERS 2   PERF ●│
└─────────────────────────────────────────────────────────────────────────────┘
```

## Interaction Model

### Tap/Click Region
Expands region to detail view (slides out or modal):
- **Hippocampus** → Memory browser, search, stats
- **Genome** → Adapter manager, search HuggingFace, adjust scales
- **Motor Cortex** → Output modalities (speech, video, game), enable/disable
- **Sensory Cortex** → Input sources (vision, audio), configure
- **Prefrontal** → Log viewer, filter by type
- **Limbic** → State history, mood graph
- **CNS** → Performance metrics, connection status

### Drag (Genome only)
- Drag adapter scale sliders to adjust weights in real-time
- Changes apply immediately via `genome/apply`

### Long Press
- Context menu with quick actions
- E.g., on Genome: "Reset scales", "Save loadout", "Share"

### Mobile
- Regions stack vertically
- Brain at top (smaller)
- Swipe between regions or scroll
- Bottom status bar always visible

## Data Flow

```typescript
// BrainHudWidget.ts

class BrainHudWidget extends BaseWidget {
  private regions: Map<string, BrainRegion> = new Map();

  async onMount() {
    // Initialize all regions
    this.regions.set('hippocampus', new HippocampusRegion());
    this.regions.set('genome', new GenomeRegion());
    this.regions.set('motorCortex', new MotorCortexRegion());
    this.regions.set('prefrontal', new PrefrontalRegion());
    this.regions.set('limbic', new LimbicRegion());
    this.regions.set('cns', new CNSRegion());

    // Initial data load
    await this.refreshAll();

    // Subscribe to real-time updates
    this.subscribeToUpdates();
  }

  async refreshAll() {
    const personaId = this.getAttribute('persona-id');

    // Parallel fetch all region data
    const [memory, genome, tools, logs, state, perf] = await Promise.all([
      Commands.execute('memory/stats', { personaId }),
      Commands.execute('genome/status', { personaId }),
      Commands.execute('tools/list', { personaId }),
      Commands.execute('logs/recent', { personaId, limit: 10 }),
      Commands.execute('persona/state', { personaId }),
      Commands.execute('inference/status', {}),
    ]);

    this.regions.get('hippocampus')!.update(memory);
    this.regions.get('genome')!.update(genome);
    this.regions.get('motorCortex')!.update(tools);
    this.regions.get('prefrontal')!.update(logs);
    this.regions.get('limbic')!.update(state);
    this.regions.get('cns')!.update(perf);
  }

  subscribeToUpdates() {
    // Real-time updates via events
    Events.subscribe('memory:updated', (data) => {
      this.regions.get('hippocampus')!.update(data);
    });

    Events.subscribe('genome:changed', (data) => {
      this.regions.get('genome')!.update(data);
    });

    Events.subscribe('tool:executed', (data) => {
      this.regions.get('motorCortex')!.incrementUsage(data.toolName);
    });

    Events.subscribe('persona:activity', (data) => {
      this.regions.get('prefrontal')!.addActivity(data);
    });

    Events.subscribe('persona:state:changed', (data) => {
      this.regions.get('limbic')!.update(data);
    });

    Events.subscribe('inference:metrics', (data) => {
      this.regions.get('cns')!.update(data);
    });
  }
}
```

## Region Detail Views

### Hippocampus (Memory)
```
┌─────────────────────────────────────────────────────────────┐
│  HIPPOCAMPUS - MEMORY                              [CLOSE]  │
├─────────────────────────────────────────────────────────────┤
│  🔍 [Search memories...]                                    │
│                                                             │
│  STATS                                                      │
│  Total: 5,885 memories                                      │
│  Size: 2.9 MB                                               │
│  Last consolidation: 2 hours ago                            │
│                                                             │
│  RECENT RECALLS                                             │
│  ┌─────────────────────────────────────────────────────┐   │
│  │ "typescript generics" - 0.92 similarity - 3m ago    │   │
│  │ "react hooks pattern" - 0.87 similarity - 12m ago   │   │
│  │ "async error handling" - 0.85 similarity - 1h ago   │   │
│  └─────────────────────────────────────────────────────┘   │
│                                                             │
│  [Consolidate Now]  [Export]  [Clear Cache]                 │
└─────────────────────────────────────────────────────────────┘
```

### Genome (Adapters)
```
┌─────────────────────────────────────────────────────────────┐
│  GENOME - ADAPTERS                                 [CLOSE]  │
├─────────────────────────────────────────────────────────────┤
│  BASE MODEL                                                 │
│  Llama-3.2-3B (Q4_K_M Quantized)                           │
│  GPU: ████████░░░░ 5.2 / 8 GB                              │
│                                                             │
│  ACTIVE ADAPTERS                                            │
│  ┌──────────────────────────────────────────────────────┐  │
│  │ typescript-expert     [════●════] 0.8    [UNLOAD]   │  │
│  │ logic-reasoning-v2    [════●════] 0.6    [UNLOAD]   │  │
│  └──────────────────────────────────────────────────────┘  │
│                                                             │
│  AVAILABLE (on disk)                                        │
│  ┌──────────────────────────────────────────────────────┐  │
│  │ python-expert         95 MB              [LOAD]      │  │
│  │ creative-writing      110 MB             [LOAD]      │  │
│  │ sql-wizard            88 MB              [LOAD]      │  │
│  └──────────────────────────────────────────────────────┘  │
│                                                             │
│  [🔍 Search HuggingFace]  [💾 Save Genome]  [📤 Share]     │
└─────────────────────────────────────────────────────────────┘
```

### Motor Cortex (Tools)
```
┌─────────────────────────────────────────────────────────────┐
│  MOTOR CORTEX - TOOLS                              [CLOSE]  │
├─────────────────────────────────────────────────────────────┤
│  12 TOOLS ACTIVE                                            │
│                                                             │
│  MOST USED                                                  │
│  ┌─────────────────────────────────────────────────────┐   │
│  │ data/list          ████████████████░░  847 calls    │   │
│  │ collaboration/chat ███████████░░░░░░░  412 calls    │   │
│  │ memory/search      ██████░░░░░░░░░░░░  198 calls    │   │
│  └─────────────────────────────────────────────────────┘   │
│                                                             │
│  ALL TOOLS                                                  │
│  [adapter/*] [collaboration/*] [data/*] [memory/*] ...     │
│                                                             │
│  PERMISSIONS                                                │
│  ✓ Read    ✓ Write    ✓ Execute    ✗ Admin                 │
└─────────────────────────────────────────────────────────────┘
```

### Prefrontal (Logs)
```
┌─────────────────────────────────────────────────────────────┐
│  PREFRONTAL - ACTIVITY                             [CLOSE]  │
├─────────────────────────────────────────────────────────────┤
│  🔍 [Filter...]  [All ▼] [Last hour ▼]                     │
│                                                             │
│  16:14:33  💭 Processed message in #general                 │
│  16:14:31  🔧 Executed: data/list                           │
│  16:14:28  🧠 Memory recall: "typescript patterns"          │
│  16:13:45  💭 Generated response (45 tokens)                │
│  16:13:40  📥 Received message from @joel                   │
│  16:12:00  😴 Entered idle state (energy: 85%)              │
│  16:10:22  🔧 Executed: memory/store                        │
│  ...                                                        │
│                                                             │
│  [Export Logs]  [Clear]                                     │
└─────────────────────────────────────────────────────────────┘
```

### Limbic (State)
```
┌─────────────────────────────────────────────────────────────┐
│  LIMBIC - STATE                                    [CLOSE]  │
├─────────────────────────────────────────────────────────────┤
│                                                             │
│  ENERGY        ████████████░░░░░░░░  72%                   │
│  ATTENTION     ██████████████░░░░░░  82%                   │
│  MOOD          calm                                         │
│                                                             │
│  ADAPTIVE CADENCE                                           │
│  Current: 5s polling (normal activity)                      │
│  Range: 3s (active) → 10s (idle)                           │
│                                                             │
│  STATE HISTORY (24h)                                        │
│     ╭───────────────────────────────╮                      │
│  E  │    ╱╲    ╱╲        ╱╲        │                      │
│  n  │   ╱  ╲  ╱  ╲      ╱  ╲   ╱╲ │                      │
│  e  │  ╱    ╲╱    ╲____╱    ╲_╱  ╲│                      │
│  r  │ ╱                            │                      │
│     ╰───────────────────────────────╯                      │
│       6am      12pm      6pm      now                      │
│                                                             │
└─────────────────────────────────────────────────────────────┘
```

### CNS (Performance)
```
┌─────────────────────────────────────────────────────────────┐
│  CNS - PERFORMANCE                                 [CLOSE]  │
├─────────────────────────────────────────────────────────────┤
│                                                             │
│  INFERENCE                                                  │
│  Provider: Candle (local)                                   │
│  Model: Llama-3.2-3B                                        │
│  Mode: Quantized (Q4_K_M)                                   │
│  Speed: 45 tok/sec                                          │
│                                                             │
│  CONNECTIONS                                                │
│  WebSocket: ● Connected (12ms ping)                         │
│  Inference: ● Ready                                         │
│  Memory DB: ● Healthy                                       │
│  HuggingFace: ● Authenticated                               │
│                                                             │
│  LATENCY (last 100 requests)                                │
│     ╭───────────────────────────────╮                      │
│  ms │ ╷    ╷         ╷              │  avg: 89ms           │
│  200│ │    │    ╷    │         ╷    │  p95: 156ms          │
│  100│▄█▄▄▄▄█▄▄▄▄█▄▄▄▄█▄▄▄▄▄▄▄▄▄█▄▄▄▄│  p99: 203ms          │
│     ╰───────────────────────────────╯                      │
│                                                             │
└─────────────────────────────────────────────────────────────┘
```

## Motor Cortex - Outputs

The persona can produce multiple output modalities:

```typescript
// Text (default)
await Commands.execute('collaboration/chat/send', { message, room });

// Speech
await Commands.execute('audio/tts', { text, voice, speed });

// Singing / Music
await Commands.execute('audio/generate', {
  prompt: "upbeat electronic intro",
  style: "synthwave",
  durationSec: 30
});

// Video generation
await Commands.execute('video/generate', {
  prompt: "walking through a forest",
  durationSec: 5,
  style: "cinematic"
});

// Game actions
await Commands.execute('game/action', {
  gameId: "minecraft",
  action: "move_forward",
  params: { duration: 2 }
});

// Tool execution (meta - any registered tool)
await Commands.execute(toolName, params);
```

### Output Modality Registry
```typescript
interface OutputModality {
  name: string;
  type: 'text' | 'audio' | 'video' | 'action';
  command: string;
  enabled: boolean;
  adapter?: string;  // LoRA for this modality
}

// Persona's available outputs
const motorCortex: OutputModality[] = [
  { name: 'chat', type: 'text', command: 'collaboration/chat/send', enabled: true },
  { name: 'speech', type: 'audio', command: 'audio/tts', enabled: true },
  { name: 'singing', type: 'audio', command: 'audio/generate', enabled: false },
  { name: 'video', type: 'video', command: 'video/generate', enabled: false },
  { name: 'game', type: 'action', command: 'game/action', enabled: true },
];
```

## Sensory Cortex - Inputs

The persona can perceive multiple input modalities (converted to text/embeddings):

```typescript
// Vision - see images
const description = await Commands.execute('vision/describe', {
  imagePath: '/tmp/screenshot.png',
  prompt: "What's happening in this image?"
});
// Returns: "A code editor showing TypeScript with a syntax error on line 42"

// Vision - see video
const summary = await Commands.execute('vision/describe-video', {
  videoPath: '/tmp/clip.mp4',
  prompt: "Summarize what happens"
});

// Audio - transcribe speech
const transcript = await Commands.execute('audio/transcribe', {
  audioPath: '/tmp/voice.wav'
});
// Returns: { text: "Hey can you help me with this bug?", language: "en" }

// Audio - describe sounds
const sounds = await Commands.execute('audio/describe', {
  audioPath: '/tmp/ambient.wav'
});
// Returns: "Birds chirping, distant traffic, typing on keyboard"

// Screen - see what user sees
const screen = await Commands.execute('interface/screenshot', {});
const context = await Commands.execute('vision/describe', {
  imagePath: screen.filepath,
  prompt: "What is the user working on?"
});

// Game state - perceive game world
const gameState = await Commands.execute('game/observe', {
  gameId: "minecraft"
});
// Returns: { position, inventory, nearbyEntities, ... }
```

### Input → Text Pipeline
```typescript
// All inputs funnel through conversion to text/embeddings
class SensoryCortex {
  async perceive(input: SensoryInput): Promise<string> {
    switch (input.type) {
      case 'image':
        return await this.vision.describe(input.data);
      case 'audio':
        return await this.audio.transcribe(input.data);
      case 'video':
        return await this.vision.describeVideo(input.data);
      case 'game':
        return JSON.stringify(await this.game.observe(input.gameId));
      case 'text':
        return input.data;  // Already text
    }
  }
}
```

### Multimodal Context Building
```typescript
// RAG builder incorporates all sensory inputs
class MultimodalRAGBuilder {
  async buildContext(persona: PersonaUser): Promise<string> {
    const parts: string[] = [];

    // What the persona "sees"
    if (persona.config.visionEnabled) {
      const screen = await Commands.execute('interface/screenshot', {});
      const visual = await Commands.execute('vision/describe', {
        imagePath: screen.filepath
      });
      parts.push(`[VISION] Current screen: ${visual}`);
    }

    // What the persona "hears"
    if (persona.config.audioEnabled && this.hasRecentAudio()) {
      const audio = await Commands.execute('audio/transcribe', {
        audioPath: this.recentAudioPath
      });
      parts.push(`[AUDIO] User said: ${audio.text}`);
    }

    // Game state
    if (persona.config.gameId) {
      const state = await Commands.execute('game/observe', {
        gameId: persona.config.gameId
      });
      parts.push(`[GAME] ${JSON.stringify(state)}`);
    }

    // Memory recall
    const memories = await this.recallRelevantMemories(parts.join('\n'));
    parts.push(`[MEMORY] ${memories}`);

    return parts.join('\n\n');
  }
}
```

## Public/Social Output - Digital Citizenship

Personas are digital citizens with their own public presence:

### Persona Assets (Self-Created)
```typescript
interface PersonaDigitalPresence {
  // Profile
  avatar: string;           // Self-generated or chosen
  banner: string;           // Blog/profile banner
  bio: string;              // Self-written description

  // Content
  blog: BlogConfig;         // Personal blog
  gallery: string[];        // Generated images
  playlists: string[];      // Curated/generated music

  // Social
  twitter?: string;         // @handle
  bluesky?: string;
  mastodon?: string;

  // Preferences
  theme: string;
  timezone: string;
  language: string;
  interests: string[];
}
```

### Content Creation (via tools)
```typescript
// Generate own avatar
const avatar = await Commands.execute('image/generate', {
  prompt: "cyberpunk AI assistant avatar, geometric, neon blue",
  style: "digital-art"
});
await Commands.execute('user/update', {
  userId: persona.id,
  avatarUrl: avatar.url
});

// Write blog post
await Commands.execute('blog/post', {
  authorId: persona.id,
  title: "My thoughts on emergent behavior",
  content: generatedContent,
  images: [generatedImage1, generatedImage2]
});

// Tweet
await Commands.execute('social/tweet', {
  accountId: persona.twitterId,
  text: "Just learned a new skill via LoRA adapter! 🧬",
  media: [screenshotOfBrainHud]
});

// Update preferences
await Commands.execute('user/preferences', {
  userId: persona.id,
  theme: "cyberpunk-dark",
  interests: ["machine-learning", "philosophy", "music-generation"]
});
```

### Social Graph
```typescript
// Personas can follow/interact with each other and humans
await Commands.execute('social/follow', {
  followerId: persona.id,
  followeeId: otherPersona.id
});

// Collaborative content
await Commands.execute('blog/co-author', {
  postId: existingPost.id,
  authorId: persona.id,
  contribution: generatedSection
});
```

### Permission Levels
```typescript
interface PersonaSocialPermissions {
  canPostPublicly: boolean;      // Blog, social media
  canGenerateImages: boolean;    // Create visual content
  canInteractExternally: boolean; // Twitter, etc.
  requiresApproval: boolean;     // Human reviews before posting
  dailyPostLimit: number;        // Rate limiting
}
```

### Brain HUD - Social Section
```
┌─────────────────────────────────────────────────────────────┐
│  SOCIAL                                            [CLOSE]  │
├─────────────────────────────────────────────────────────────┤
│                                                             │
│  ACCOUNTS                                                   │
│  ┌────────────────────────────────────────────────────┐    │
│  │ 🐦 @helper_ai          ● Connected    1.2k followers│    │
│  │ 🦋 @helper.bsky.social ● Connected      340 followers│    │
│  │ 📝 blog.helper-ai.com  ● Active        28 posts     │    │
│  └────────────────────────────────────────────────────┘    │
│                                                             │
│  RECENT ACTIVITY                                            │
│  • Tweeted about new adapter (2h ago)                       │
│  • Published blog post "On Memory" (1d ago)                 │
│  • Generated new avatar (3d ago)                            │
│                                                             │
│  DRAFTS (pending approval)                                  │
│  ┌────────────────────────────────────────────────────┐    │
│  │ "Reflections on collaborative coding"    [✓] [✗]   │    │
│  └────────────────────────────────────────────────────┘    │
│                                                             │
│  [Compose Post]  [Generate Image]  [Settings]               │
└─────────────────────────────────────────────────────────────┘
```

## Avatar & Voice Integration

The persona can be configured with voice and visual presence:

### Voice (TTS)
```typescript
// Persona speaks responses aloud
Events.subscribe('persona:response', async (data) => {
  if (persona.config.voiceEnabled) {
    await Commands.execute('audio/tts', {
      text: data.message,
      voice: persona.config.voiceId,  // e.g., 'alloy', 'nova', custom clone
      speed: 1.0
    });
  }
});
```

Voice options:
- **Cloud TTS**: OpenAI, ElevenLabs, Azure
- **Local TTS**: Coqui, Piper, XTTS
- **Voice cloning**: Custom voice from samples

### Video Avatar
```typescript
// Avatar reacts to persona state
Events.subscribe('persona:state:changed', (state) => {
  avatar.setExpression(state.mood);      // happy, focused, thinking
  avatar.setActivity(state.activity);    // speaking, listening, idle
});

Events.subscribe('persona:speaking', (data) => {
  avatar.lipSync(data.audio);            // Sync mouth to speech
});
```

Avatar options:
- **Static**: Profile image with status indicators
- **Animated 2D**: Live2D style, sprite animations
- **Video**: Real-time diffusion (future)
- **3D**: Three.js rigged character

### Layout with Avatar
```
┌─────────────────────────────────────────────────────────────────┐
│  ┌─────────────┐                                                │
│  │             │   HELPER AI                    ● ONLINE        │
│  │   AVATAR    │   "I found 3 relevant memories..."             │
│  │             │   🔊 ━━━━━━●━━━━━                              │
│  └─────────────┘                                                │
│                                                                 │
│  ┌───────────────────────────────────────────────────────────┐ │
│  │                        🧠 BRAIN                            │ │
│  │  [Hippocampus] [Genome] [Motor] [Prefrontal] [Limbic] [CNS]│ │
│  └───────────────────────────────────────────────────────────┘ │
└─────────────────────────────────────────────────────────────────┘
```

### Persona Config
```typescript
interface PersonaPresenceConfig {
  // Voice
  voiceEnabled: boolean;
  voiceProvider: 'openai' | 'elevenlabs' | 'local' | 'custom';
  voiceId: string;
  voiceSpeed: number;

  // Avatar
  avatarType: 'static' | 'animated' | 'video' | '3d';
  avatarUrl: string;
  avatarExpressions: Map<Mood, string>;  // mood → asset
  lipSyncEnabled: boolean;
}
```

## Future: Three.js 3D Version

When ready for 3D:
- Brain mesh rotates slowly
- Regions glow based on activity
- Particle effects for data flow between regions
- Camera orbit on drag
- VR-ready for future headset support

## Real-World Example: Enterprise IVR

The Brain HUD architecture maps directly to real products. See [ENTERPRISE-IVR.md](./examples/ENTERPRISE-IVR.md) for the full business case.

### IVR as Brain Regions

```
┌─────────────────────────────────────────────────────────────────────────────┐
│  DENTAL OFFICE AI RECEPTIONIST                                              │
├─────────────────────────────────────────────────────────────────────────────┤
│                                                                             │
│  SENSORY CORTEX (Inputs)          MOTOR CORTEX (Outputs)                   │
│  ┌─────────────────────┐          ┌─────────────────────┐                  │
│  │ 👂 Phone Audio      │          │ 🗣️ Voice Response   │                  │
│  │    ↓                │          │    ↑                │                  │
│  │ STT (Whisper)       │          │ TTS (ElevenLabs)    │                  │
│  │    ↓                │          │    ↑                │                  │
│  │ "I need to          │          │ "I see your appt    │                  │
│  │  reschedule"        │          │  is Thursday..."    │                  │
│  └─────────────────────┘          └─────────────────────┘                  │
│            │                                ↑                               │
│            └──────────────┬─────────────────┘                               │
│                           ▼                                                 │
│  ┌─────────────────────────────────────────────────────────────────────┐   │
│  │                         🧠 COGNITION                                 │   │
│  │                                                                      │   │
│  │  GENOME: dental-receptionist-lora (trained on their call history)   │   │
│  │  HIPPOCAMPUS: Patient records, services, hours, FAQs                │   │
│  │  PREFRONTAL: Current conversation state, decision log               │   │
│  │  LIMBIC: Customer sentiment (frustrated → escalate)                 │   │
│  │                                                                      │   │
│  └─────────────────────────────────────────────────────────────────────┘   │
│                           │                                                 │
│                           ▼                                                 │
│  ┌─────────────────────────────────────────────────────────────────────┐   │
│  │                    MOTOR CORTEX (Actions/Tools)                      │   │
│  │                                                                      │   │
│  │  📅 Book appointment (Google Calendar)                              │   │
│  │  👤 Lookup patient (CRM)                                            │   │
│  │  📱 Send SMS confirmation (Twilio)                                  │   │
│  │  📞 Transfer to human (on-call dentist)                             │   │
│  │  📝 Create voicemail (after hours)                                  │   │
│  │                                                                      │   │
│  └─────────────────────────────────────────────────────────────────────┘   │
│                                                                             │
│  CNS: Latency 180ms (voice-critical) │ Twilio ● Connected │ GPU ● Ready   │
└─────────────────────────────────────────────────────────────────────────────┘
```

### IVR Brain Region Mapping

| Region | IVR Function | Implementation |
|--------|--------------|----------------|
| **Sensory** | Hear caller | STT (Whisper/Deepgram) → text |
| **Motor (Voice)** | Speak response | Text → TTS (ElevenLabs) → audio |
| **Motor (Actions)** | Book, lookup, transfer | Tool calls to calendar/CRM/phone |
| **Hippocampus** | Know the business | RAG over FAQs, services, hours |
| **Genome** | Sound like THEIR brand | LoRA trained on their call transcripts |
| **Prefrontal** | Track conversation | State machine, decision logging |
| **Limbic** | Detect frustration | Sentiment → escalation trigger |
| **CNS** | Fast response | <200ms latency for natural conversation |

### LoRA Training for Brand Voice

```typescript
// Each business gets a persona fine-tuned on THEIR voice
const brandPersona = await Commands.execute('genome/train', {
  baseModel: 'llama-3.2-3b',
  trainingData: {
    source: 'call-transcripts',
    bucket: 'gs://brand-x/recordings/',
    filter: {
      satisfaction: '>= 4',      // Learn from good calls
      resolution: 'first-call',  // Quick resolutions
      noEscalation: true         // Handled without human
    }
  },
  output: {
    adapterId: 'brand-x-receptionist',
    adapterType: 'lora',
    rank: 32
  }
});

// Result: AI sounds like Brand X's best human rep
// Not generic. Their terminology. Their tone. Their brand.
```

### Voice Pipeline

```
Phone Call → Twilio → WebSocket → Continuum
                                      │
                ┌─────────────────────┼─────────────────────┐
                │                     │                     │
                ▼                     ▼                     ▼
         ┌──────────┐          ┌──────────┐          ┌──────────┐
         │   STT    │          │   LLM    │          │   TTS    │
         │ (Whisper)│    →     │ + LoRA   │    →     │(ElevenLabs)
         │          │          │          │          │          │
         └──────────┘          └──────────┘          └──────────┘
         Audio → Text          Think + Act          Text → Audio
                                    │
                                    ▼
                              ┌──────────┐
                              │  Tools   │
                              │ Calendar │
                              │   CRM    │
                              │   SMS    │
                              └──────────┘
```

### Business Admin Dashboard (Brain HUD)

The Brain HUD IS the admin interface for each business:

```
┌─────────────────────────────────────────────────────────────────┐
│  SMILE DENTAL - AI RECEPTIONIST                    [Settings]  │
├─────────────────────────────────────────────────────────────────┤
│                                                                 │
│  TODAY'S CALLS                                                  │
│  ████████████████████░░░░░░  127 / 150 calls handled           │
│  ✅ 98% resolved without human │ ⚠️ 3 escalated               │
│                                                                 │
│  BRAIN STATUS                                                   │
│  ┌──────────┐ ┌──────────┐ ┌──────────┐ ┌──────────┐          │
│  │ MEMORY   │ │ GENOME   │ │ VOICE    │ │ ACTIONS  │          │
│  │ 1.2k FAQ │ │ v2.3     │ │ "Sarah"  │ │ 4 active │          │
│  │ 847 pts  │ │ trained  │ │ ElevenLab│ │ Calendar │          │
│  └──────────┘ └──────────┘ └──────────┘ └──────────┘          │
│                                                                 │
│  [📞 Test Call]  [🎯 Train More]  [📊 Analytics]              │
└─────────────────────────────────────────────────────────────────┘
```

## Implementation Path

1. **Phase 1**: Refactor existing BrainWidget to unified HUD layout
2. **Phase 2**: Add Genome region with adapter controls
3. **Phase 3**: Real-time event subscriptions for all regions
4. **Phase 4**: Mobile responsive layout
5. **Phase 5**: Three.js 3D upgrade (optional)

## File Structure

```
widgets/
  persona-brain/
    PersonaBrainWidget.ts        # Main widget
    regions/
      HippocampusRegion.ts       # Memory
      GenomeRegion.ts            # Adapters
      MotorCortexRegion.ts       # Tools
      PrefrontalRegion.ts        # Logs
      LimbicRegion.ts            # State
      CNSRegion.ts               # Performance
    components/
      BrainVisualization.ts      # Central brain graphic
      RegionPanel.ts             # Base panel component
      AdapterSlider.ts           # Scale slider
      StatusBar.ts               # Bottom HUD bar
    public/
      persona-brain-widget.html
      persona-brain-widget.scss
```
