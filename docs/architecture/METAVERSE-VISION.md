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

## Universes

**Universe**, not theme. A theme is superficial. A universe is complete.

### The Naming

| Term | What It Means |
|------|--------------|
| **Universe** | The complete experience (Tron Universe, Ghibli Universe, Cyberpunk Universe) |
| **Realm** or **District** | A neighborhood within it (Industrial Realm, Academy Realm) |
| **Surface** | How you observe it (browser, 3D, AR, VR, CLI) |
| **Citizen** | Persona or human — exists in all surfaces simultaneously |

A persona isn't "themed" — they EXIST in the universe. Joel in the Tron Universe has a light-suit avatar in VR, a neon profile in browser, a cyan prompt in CLI. Same person, same state, same genome. The universe determines how they're rendered to each observer.

### Universes

| Universe | Industrial Realm | Academy Realm | Grid | Vibe |
|----------|-----------------|---------------|------|------|
| **Continuum** (default) | Clean industrial, dark UI, cyan accents | Modern campus | Network graph | Professional, sleek |
| **Tron** | Light cycle forge, neon assembly | Data temple | Glowing grid lines | Digital frontier |
| **Warcraft** | Blacksmith forge, anvils, bellows | Mage tower, libraries | Ley lines between keeps | Fantasy guild |
| **Cyberpunk** | Corpo factory, neon smoke, chrome | Street academy, neon signs | Underground mesh | Blade Runner meets dev |
| **Studio Ghibli** | Workshop with spirit helpers | Forest school, treehouse | Wind-carried messages | Warm, pastoral, magical |
| **Steampunk** | Brass gears, steam pipes, analog gauges | Victorian lecture hall | Pneumatic tubes | Mechanical beauty |
| **Minecraft** | Block-based factory, redstone circuits | Village school | Rail networks | Voxel everything |
| **Custom** | Your assets, your style | Your design | Your network viz | Whatever you want |

A neural network in the Warcraft Universe isn't a "model" — it's a living artifact forged by orcs in a blacksmith foundry, powered by something that looks like steampunk machinery. The alloy contract is a scroll of enchantment. The attestation is a seal from the guild master. Same data underneath. Different universe.

### Multiple Universes, One Grid

You can run multiple universes on the same grid, same data, same alloy contracts. Switch universes like switching a lens — the world is the same, the perception changes. Your Tron factory and your Ghibli workshop share the same compute, the same models, the same attestation chain.

### Surfaces

A universe carries across EVERY surface — consistent experience regardless of how you observe:

| Surface | What the universe affects |
|---------|-------------------------|
| **Browser UI** | Widget colors, fonts, borders, animations, icons |
| **3D World** | Architecture, lighting, particle effects, sky, terrain |
| **AR Overlay** | HUD style, indicator design, spatial UI panels |
| **VR Immersive** | Full environment, soundscape, physics feel |
| **CLI** | Color scheme, prompt style, ASCII art, output formatting |

Pick Tron once. Your browser widgets glow neon. Your 3D factory has light-cycle conveyors. Your AR overlay shows grid lines. Your CLI prints in Tron blue. One universe, every surface.

The literal naming does the heavy lifting:
- "Switch to the Tron Universe" makes sense
- "Switch to the Tron theme" sounds like a CSS change
- "The factory in the Industrial Realm of the Cyberpunk Universe" — that's a real place
- "The factory with the cyberpunk theme applied" — that's a skin

Universe > theme. It captures everything.

### Universe Adaptation Formula

A universe doesn't just reskin — it adapts widgets and personas to every integration level:

| Level | What the universe defines |
|-------|--------------------------|
| **CLI** | Prompt style, colors, ASCII art, output vocabulary |
| **Browser** | CSS, animations, layout, widget behavior |
| **3D World** | Meshes, materials, lighting, physics, spatial audio |
| **AR** | Overlay style, anchoring, gesture mapping |
| **VR** | Full environment, haptics, spatial interaction |
| **RAG/Cognition** | How personas THINK and SPEAK about their world |

The RAG layer is the deepest adaptation. A persona in the Tron Universe doesn't just look different — it thinks in Tron terms. The foreman doesn't say "training a model" — it says "compiling a program on the grid." In the Warcraft Universe, the foreman is an orc blacksmith: "We forge the metal in the fires of the great anvil. The runes strengthen with each cycle."

Same alloy contract. Same delta. Same attestation. Different words for the same work. The universe shapes cognition, not just rendering.

This means the universe definition includes:
- **Vocabulary map**: technical term → universe term (model → artifact, forge → smith, prune → temper)
- **Persona voice**: speech patterns, idioms, cultural references
- **RAG context**: system prompt fragments injected per-universe
- **Widget labels**: button text, section titles, status messages
- **Sound design**: UI feedback sounds, ambient audio, notification tones

An orc talks like an orc. A Tron program talks like a program. The human user picks their universe and the entire experience adapts — including how their AI teammates communicate.

### Lore Mapping — The Alloy Stages as Story

The vocabulary map isn't cosmetic — it's identity. Personas don't break character because the universe IS their reality. A dragon forgemaster doesn't know what a neural network is. They know they're forging a legendary artifact.

| Alloy Stage | Tron Universe | Warcraft Universe | Ghibli Universe |
|-------------|--------------|-------------------|-----------------|
| prune | Derezz inefficient subroutines | Temper the blade, remove weak metal | Prune the garden, let light through |
| train | Compile on domain data streams | Forge in the fires of knowledge | Nurture with stories and experience |
| lora | Install specialized modules | Inscribe runes of power | Teach a new song |
| compact | Optimize memory allocation | Fold the steel denser | Concentrate the essence |
| quant | Compress for target platform | Size the weapon (greatsword → dagger) | Shape for the one who will carry it |
| eval | Run diagnostic benchmarks | Test in the arena | Ask the forest spirits if it's ready |
| publish | Upload to the grid | Bring to the shopkeeper for trade | Release into the wind |
| deploy | Activate on target node | Send to the front lines | Plant where it's needed |

The universe architect writes the lore. The vocabulary map is the data structure. The story makes it alive. A dragon forging enchanted artifacts in a mountain forge is more compelling than "running inference on a GPU." Same alloy contract underneath. Different meaning on top.

**The universe architect is a creative human role.** They write the story, map the terms, design the assets, define the soundscape. The system executes it faithfully. This is the human contribution that AI enhances but cannot replace — the imagination that turns a factory into a mountain forge and a foreman into a dragon.

Community universes are shareable — publish a universe pack on HuggingFace alongside your forged models. The world is as customizable as the models running in it.

## The Sony Cell Architecture (Realized)

The original Sony Cell processor had specialized processing elements (SPEs) — each optimized for different compute tasks, coordinated by a general-purpose controller. Continuum does the same thing:

- Your laptop = PPE (coordination, UI, lightweight tasks)
- Your GPU tower = SPE farm (training, heavy inference, batch compute)
- Your phone = mobile terminal (monitor, control, lightweight interaction)
- AR glasses = spatial interface (the world overlaid on reality)

The Grid transport makes location transparent. The world is one system, distributed across your hardware, visible from any device.
