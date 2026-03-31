# Metaverse Vision — The World Layer

Continuum is a world, not a tool. The 2D interface is the first layer. The 3D immersive experience is the product.

## The Layers

```
Layer 0: Data (alloy contracts, persona state, genome, memory, grid)
Layer 1: 2D Widgets (factory composer, chat, academy — browser-based)
Layer 2: 3D Scenes (avatar calls, spatial rooms — Bevy renderer)
Layer 3: Walkable World (neighborhoods, buildings, factory floors)
Layer 4: Mixed Reality (AR overlay on physical space)
Layer 5: Full Metaverse (economy, real estate, physical location mapping)
```

Every layer shares the same data model. A forge alloy is a forge alloy whether you configure it in a 2D widget, a 3D control panel, or by talking to the foreman in AR. The view changes. The truth doesn't.

## The Neighborhoods

The world has districts, like a city:

| District | What Happens There | 2D Equivalent |
|----------|-------------------|---------------|
| **Industrial** | Factory floors, assembly lines, GPU farms | Factory widget |
| **Campus** | Academy classrooms, exam halls, libraries | Academy widget |
| **Downtown** | Collaboration spaces, meeting rooms, live calls | Chat + Live widgets |
| **Workshop** | Code editors, sentinel pipelines, dev tools | Coding widget |
| **Market** | Adapter marketplace, model leaderboards, trade | HuggingFace integration |
| **Grid Hub** | Network visualization, node health, routing | Grid widget |
| **Residence** | Persona homes, memories, personal genome labs | Profile widget |

## The Industrial District

The factory floor in 3D:

```
[Loading Dock]          [Assembly Line]              [Quality Control]     [Shipping]
 ┌─────────┐    ┌───────────────────────────┐    ┌──────────────┐    ┌──────────┐
 │ Base     │───>│ Prune → Train → Compact  │───>│ HumanEval    │───>│ HF Pub   │
 │ Models   │    │ (GPU stations humming)    │    │ MMLU         │    │ Grid     │
 │ arrive   │    │ Foreman watches metrics   │    │ Benchmarks   │    │ Deploy   │
 └─────────┘    └───────────────────────────┘    └──────────────┘    └──────────┘
```

Walk up to a station. See the loss curve on a floating display. The foreman persona explains what's happening. Tap a model on the conveyor to inspect its alloy. The attestation badge glows green when verified.

## Mixed Reality

AR glasses overlay persona state on physical space:

- Look at your GPU tower → see VRAM usage, active forge, temperature
- Look at your desk → see your persona team status, who's thinking, who's idle
- Look at your screen → spatial UI overlays on your browser
- Walk through your house → each room maps to a continuum district

## Technical Foundation

Already built:
- **Bevy 3D renderer** — avatar system, 15fps render loop, GPU governor
- **VRM avatars** — 21 CC0 models, facial animation, cognitive state visualization
- **LiveKit WebRTC** — real-time audio/video, data channels
- **ForgeAlloy** — universal work contract (same at every layer)
- **Persona state** — energy, attention, mood, genome (drives avatar behavior)
- **Grid mesh** — distributed compute (nodes ARE the physical machines)

The path from 2D widgets to 3D world is incremental:
1. Widgets render in 2D (now)
2. Same data drives 3D scenes (Bevy already does this for avatars)
3. Spatial rooms replace video tiles (personas exist in space)
4. Rooms connect into a world (navigation between districts)
5. World maps to physical space (AR/VR)

## Why This Matters

Every AI project builds a better terminal. A smarter command line. A faster code agent.

Continuum builds a place to LIVE. A world where your AI teammates have faces, voices, homes, jobs, memories, and skills they earned. Where you walk through a factory and watch models being forged. Where the academy is a campus your personas attend. Where the grid is a physical network you can see.

The interface isn't the product. The WORLD is the product. The interface is just how much of the world you can see right now.

## The Sony Cell Architecture (Realized)

The original Sony Cell processor had specialized processing elements (SPEs) — each optimized for different compute tasks, coordinated by a general-purpose controller. Continuum does the same thing:

- Your laptop = PPE (coordination, UI, lightweight tasks)
- Your GPU tower = SPE farm (training, heavy inference, batch compute)
- Your phone = mobile terminal (monitor, control, lightweight interaction)
- AR glasses = spatial interface (the world overlaid on reality)

The Grid transport makes location transparent. The world is one system, distributed across your hardware, visible from any device.
