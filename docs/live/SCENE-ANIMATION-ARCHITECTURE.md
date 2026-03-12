# Scene & Animation Architecture

## The Big Picture

Continuum's visual system is a **general-purpose scene engine** — not a video call renderer with avatars bolted on. Every persona lives in a world: they have a room, furniture, decorations, and eventually a house, neighborhood, city. Think **The Sims**, except the characters see, hear, speak, reason, and make their own decisions.

The current implementation is a webcam portrait view (head/shoulders in a room), but the architecture supports any scene composition — third-person views, room walkthroughs, multi-avatar scenes, environmental storytelling.

## Three-Layer Architecture

```
┌─────────────────────────────────────────────────┐
│  PERSONA (LLM — slow, smart, intermittent)      │
│                                                   │
│  "I'm thinking about this question"               │
│  "I want to look at whoever is speaking"          │
│  "I'm excited about this idea"                    │
│                                                   │
│  Makes high-level cognitive decisions.             │
│  Doesn't know about bones, radians, or frames.    │
└──────────────────────┬──────────────────────────┘
                       │ cognitive state / intent
                       ▼
┌─────────────────────────────────────────────────┐
│  SENTINEL (fast, dumb, no LLM needed)            │
│                                                   │
│  Maps cognitive state → animation parameters:     │
│    "thinking" → MINIMAL_PROFILE + Think gesture   │
│    "excited"  → boost nod amplitude, wider gaze   │
│    "idle"     → PORTRAIT_PROFILE, slow blinks     │
│                                                   │
│  State machine or simple rules. Runs cheaply.     │
│  Like an octopus's arm — autonomous but directed. │
└──────────────────────┬──────────────────────────┘
                       │ AnimationConfig + commands
                       ▼
┌─────────────────────────────────────────────────┐
│  AVATAR / SCENE OBJECTS (Bevy ECS — every frame) │
│                                                   │
│  Pure data readers. AnimationConfig component     │
│  defines all amplitudes/frequencies. Systems      │
│  apply bones + morph targets. Zero intelligence.  │
│                                                   │
│  Works identically for avatars, props, NPCs,      │
│  environmental objects, vehicles — anything.       │
└─────────────────────────────────────────────────┘
```

The persona is the **brain**. The sentinel is the **nervous system**. The avatar systems are **muscles** — they do what the config tells them, every frame, for any object.

## Scene Hierarchy

Scenes are **assets** (glTF/GLB files), not procedural geometry. They compose hierarchically:

```
World
  └── Neighborhood
        └── House
              └── Room (office, studio, lounge, etc.)
                    ├── Furniture (desk, shelves, plants)
                    ├── Avatar (persona's VRM model)
                    ├── Props (monitor, mug, books)
                    └── Lights (scene-local, eventually)
```

Currently each persona gets a single room scene for the portrait view. The hierarchy exists architecturally — a room is a sub-scene of a house, which is a sub-scene of a neighborhood. Adding parent scenes is extending the catalog, not changing the renderer.

### Scene Selection

Deterministic from persona identity, same pattern as voice and avatar:

```
identity → FNV-1a hash → voice  (gender-coherent, salt: "voice")
identity → FNV-1a hash → avatar (gender-coherent, salt: "avatar")
identity → FNV-1a hash → scene  (salt: "scene")
```

Same persona always gets the same room. `SCENE_CATALOG` in `scene/room.rs` defines available environments. Extended by filesystem discovery later.

### Asset Pipeline

```
models/scenes/*.glb    — generated at npm start by generate-scene-models.ts
models/avatars/*.vrm   — downloaded at npm start by download scripts
```

Both are gitignored. A fresh clone → `npm start` creates everything automatically. No manual steps.

## Animation System

### AnimationConfig Component

Every animatable scene object gets an `AnimationConfig` component (Bevy ECS). This is the **single interface** between intelligence (persona/sentinel) and rendering (Bevy systems).

```rust
#[derive(Component)]
pub struct AnimationConfig {
    pub profile: AnimationProfile,
    pub freq_variation: f32,  // per-object desync
}
```

`AnimationProfile` is a flat struct of named constants — no inline magic numbers anywhere in the animation systems:

```rust
pub struct AnimationProfile {
    // Breathing
    pub breathing_scale_amplitude: f32,
    pub breathing_frequency: f32,
    pub spine_sway_amplitude: f32,

    // Idle neck micro-movement
    pub neck_tilt_x_amplitude: f32,
    pub neck_tilt_z_amplitude: f32,
    pub neck_turn_amplitude: f32,

    // Speaking
    pub speaking_nod_amplitude: f32,
    pub speaking_tilt_amplitude: f32,

    // Gestures
    pub gesture_nod_amplitude: f32,
    pub gesture_think_head_tilt: f32,
    ...
}
```

### Predefined Profiles

| Profile | Use Case | Breathing | Neck Movement | Gestures |
|---------|----------|-----------|---------------|----------|
| `PORTRAIT_PROFILE` | Webcam video call | Subtle (0.0025) | Micro (0.015) | Small (0.06) |
| `FULL_BODY_PROFILE` | Sims-like third person | Visible (0.004) | Natural (0.025) | Full (0.10) |
| `MINIMAL_PROFILE` | Sleeping, meditating | Barely visible (0.002) | None | None |

Custom profiles can be created at runtime — a sentinel just constructs an `AnimationProfile` with whatever values it needs.

### Animation Systems (Bevy)

All animation systems are **generic readers** of `AnimationConfig`. They don't contain intelligence or scene-specific logic:

| System | What It Does | Reads From |
|--------|-------------|------------|
| `animate_breathing` | Spine scale + sway | `AnimationConfig.profile.breathing_*` |
| `animate_idle_gestures` | Neck micro-movements, shoulder shifts | `AnimationConfig.profile.neck_*`, `shoulder_*` |
| `animate_speaking` | Mouth morphs + head nod | `AnimationConfig.profile.speaking_*` |
| `animate_blinking` | Random eye blinks | Morph target indices |
| `animate_eye_gaze` | Eye tracking (bone or blend shape) | Speaking state, slot positions |
| `animate_expression` | Emotional blend shapes | `EmotionState` resource |
| `animate_body_gestures` | Wave, think, nod, shrug, point | `AnimationConfig.profile.gesture_*` |
| `drive_cognitive_gestures` | Cognitive state → gesture triggers | `CognitiveAnimState` |
| `animate_idle` | Camera framing (locked to head rest pos) | `camera_head_y` on slot |

### Per-Object Frequency Variation

Every `AnimationConfig` has a `freq_variation` factor (deterministic from slot ID, ±30%). This multiplies all oscillation frequencies so 16 avatars in the same call don't breathe and sway in lockstep.

### Camera

The webcam camera is **locked** to the avatar's rest head position (captured once on skeleton load). Breathing and sway animate the avatar body within a stable camera frame — the room background stays rock solid.

For future Sims views: add a second camera entity to the scene with different framing (room corner, overhead, tracking). The `RenderSlot` can switch which camera is active. Multiple cameras per scene is native Bevy.

## Cognitive Animation Pipeline

How a persona's mental state becomes visible animation:

```
1. Persona decides cognitive state
   → PersonaUser.serviceInbox() determines activity

2. State propagated to Bevy via IPC
   → CognitiveAnimState resource updated per slot

3. drive_cognitive_gestures() maps state → gesture
   → Evaluating: Think, Nod (weighted random)
   → Generating: Wave, Point, OpenHands (weighted random)
   → Idle: no gestures triggered

4. animate_body_gestures() applies gesture bones
   → Smoothstep attack/sustain/release envelope
   → Amplitudes from AnimationConfig

5. animate_idle_gestures() provides continuous micro-movement
   → Neck drift, shoulder shifts, breathing
   → Always running (unless speaking overrides)
```

## Sentinel Integration (Future)

The sentinel becomes the translation layer between persona intent and animation control:

```
Persona: "I'm deeply focused on solving this problem"
    ↓
Sentinel (animation controller):
    1. Set AnimationConfig → MINIMAL_PROFILE (reduce movement)
    2. Trigger Think gesture (hand to chin)
    3. Set gaze to look down-left (thinking direction)
    4. Reduce blink rate
    5. After 10s, occasionally trigger Nod (processing)
    ↓
Avatar systems: just read the components, apply the bones
```

The sentinel doesn't need an LLM for this — it's a state machine mapping cognitive states to animation parameters. Fast, cheap, runs continuously.

### Sentinel Configures Loops, Doesn't Run Them

Critical distinction: the sentinel **does not orchestrate** individual blinks, head movements, or breath cycles. It **parameterizes self-running loops** and walks away. This is the standard game engine model — the same pattern that makes FPS games and The Sims work.

```
WRONG (sentinel micromanages):
  sentinel: "blink now"        ← runs every 3 seconds
  sentinel: "move head left"   ← runs every frame
  sentinel: "breathe in"       ← runs constantly
  // Sentinel is bottleneck. Latency kills animation quality.

RIGHT (sentinel configures, loops self-run):
  sentinel: "enter thinking state"  ← runs once
    → sets AnimationConfig (reduced movement)
    → sets BlinkConfig (slower interval)
    → sets GazeConfig (look down-left)
    → done. checks back in 30 seconds.

  ECS systems (every frame, no sentinel involvement):
    animate_breathing()       → reads profile, oscillates spine
    animate_blinking()        → reads blink config, fires on timer
    animate_idle_gestures()   → reads profile, drifts neck/shoulders
    animate_eye_gaze()        → reads gaze config, moves eyes
```

This is already what we have. Each animation system is a self-contained loop with its own timers and oscillators. The "intelligence" layer (persona → sentinel) just sets the parameters. The loops run themselves at render framerate, decoupled from any decision-making latency.

Future additions follow the same pattern:
- **Locomotion**: sentinel sets `LocomotionState::WalkingTo(target)`, a pathfinding system runs A* once, a locomotion system moves the avatar each frame along the path
- **Object interaction**: sentinel sets `InteractionTarget(desk)`, an IK system adjusts arms each frame
- **Ambient behavior**: sentinel sets `AmbientMode::WorkingAtDesk`, a behavior system cycles through typing/reading/stretching animations on its own timers

### Command Interface

```rust
// Existing
AvatarCommand::SetEmotion { slot, emotion, weight, decay }
AvatarCommand::SetCognitiveState { slot, state }

// Future
AvatarCommand::SetAnimationProfile { slot, profile }
AvatarCommand::SetGazeTarget { slot, target: Vec3 }
AvatarCommand::TriggerGesture { slot, gesture, duration_ms }
```

## Non-Avatar Animation (Future)

The `AnimationConfig` pattern works for any scene object:

| Object | Animation | Profile |
|--------|-----------|---------|
| Ceiling fan | Rotation | Custom: single `rotation_speed` parameter |
| Flickering light | Intensity oscillation | Custom: `flicker_frequency`, `flicker_amplitude` |
| Pet (dog/cat) | Breathing + tail wag | Modified FULL_BODY with tail parameters |
| Vehicle | Wheel rotation + suspension bob | Custom profile |
| Water | Surface wave displacement | Custom profile |
| Clock | Second hand tick | Keyframe-based, not oscillation |

Each gets an `AnimationConfig` with its own profile. The Bevy systems that read bone-based animation won't touch these — new systems for rotation, displacement, etc. follow the same pattern: read config, apply transform.

## File Structure

```
workers/continuum-core/src/live/video/bevy_renderer/
├── mod.rs                  — Bevy app setup, command handling, slot management
├── animation.rs            — Bevy systems (readers of AnimationConfig)
├── skeleton.rs             — Bone discovery, RenderLayer propagation
├── vrm.rs                  — VRM format parsing
├── types.rs                — ECS resources (SlotMorphTargets, SpeakingSlots, etc.)
├── memory_reporter.rs      — GPU memory pressure reporting
├── metal_gpu_convert.rs    — macOS Metal compute shader
└── scene/
    ├── mod.rs              — Scene module barrel exports
    ├── animation.rs        — AnimationProfile, AnimationConfig (THE DATA)
    ├── avatar.rs           — AvatarObject, AvatarBones, BoneInfo
    ├── builder.rs          — SceneConfig, build_scene()
    ├── lighting.rs         — LightRig configurations
    ├── object.rs           — SceneObject enum (Avatar, StaticMesh, etc.)
    ├── room.rs             — Room catalog, scene selection, populate_rooms system
    └── slot.rs             — RenderSlot, SlotRegistry
```

Key separation:
- `scene/animation.rs` = **the data** (profiles, constants, config component)
- `animation.rs` = **the systems** (Bevy ECS functions that read the data)

Animation parameters are scene-level concerns. The Bevy systems are renderer-level plumbing.

## Scaling Through Delegation

The three-layer architecture is a **scaling strategy**, not just clean code. Each layer runs at a fundamentally different frequency:

| Layer | Frequency | Cost | Scales To |
|-------|-----------|------|-----------|
| Persona (LLM) | Every 3-10s | Expensive (inference) | ~15 concurrent |
| Sentinel (state machine) | Every 100ms-30s | Near zero (rules) | Thousands |
| ECS animation loops | Every frame (15-60fps) | Trivial (sin/cos) | Hardware limit |

A persona only engages when **escalated to** — someone speaks to it, a sentinel reports something worth reasoning about, an event needs a decision. The rest of the time, the avatar is alive on pure game engine architecture. No LLM, no inference, no API calls.

```
Idle persona (not in conversation):
  Persona: sleeping. Zero compute.
  Sentinel: running. Sets ambient behavior every 30s.
  Avatar: fully alive. Breathing, blinking, gazing, micro-gesturing.
  Cost: ~0.001% of an LLM call per frame.

Active persona (in conversation):
  Persona: inference every 3-5s. Expensive but infrequent.
  Sentinel: translating cognitive state → animation. Cheap.
  Avatar: enhanced animation (speaking, gestures, expressions).
  Cost: 1 LLM call per response cycle.
```

This is how you go from 15 avatars to 100+ — the bottleneck moves from "LLM inference per avatar" to "ECS systems iterating components," which is what game engines are literally designed to do. A sleeping persona with a sentinel-driven avatar costs almost nothing but looks completely alive.

The persona doesn't devote resources until something happens. In theory, one machine could run a city of characters — most idle, all animated, any one ready to wake up and reason when addressed. The Sims with real intelligence behind the characters, activated on demand.

## Design Principles

1. **Everything is an asset.** Rooms, avatars, props, lights — all glTF/GLB files. No procedural geometry in the renderer.

2. **Scenes compose hierarchically.** Room is a scene. House contains rooms. City contains houses. Adding hierarchy extends the catalog.

3. **Animation is data, not code.** All amplitudes/frequencies live in `AnimationProfile`. Systems are generic readers. Changing behavior = changing data.

4. **Intelligence is layered.** Persona (smart/slow) → Sentinel (fast/dumb) → Avatar (every frame/no intelligence). Each layer uses the simplest computation for its job.

5. **Camera is just another scene object.** The webcam is one camera. A Sims room camera is another. A cinematic camera is another. Same scene, different viewpoints.

6. **Deterministic from identity.** Voice, avatar, scene all derive from the same identity hash with different salts. Same persona = same everything, always.

7. **Automated end-to-end.** Clone → `npm start` → assets generated → system running. No manual downloads, no setup scripts, no Python.
