# The Continuum engine: its design and object model

Status: design for review, 2026-11-20. Owners: Fable (the model and its migration), Codex (resources,
serving, grid; review). Replaces `PARTS.md`. Measured against canary c819052ef7.

[Runtime architecture](../../README.md#runtime-architecture) · [How a mind runs](../HOW-A-MIND-RUNS.md) ·
[Module architecture](../MODULE-ARCHITECTURE.md)

This book starts from the design ideal (chapter 0): Continuum as an engine that keeps a living
society present on small, changing hardware, as RAGE keeps a city present on a console. It then
describes every important object and interface in the Rust core, how they inherit from
one another from the most general to the most specific, which crate and module each lives in, and
how today's code gets there. It reads in order: each chapter builds on the ones before it.

## Chapters

| # | Chapter | What it answers |
|---|---|---|
| 0 | [The engine](00-engine.md) | The design ideal: the brief, what RAGE and CBAR learned and what each lesson becomes here, the budgets, what we refuse |
| 1 | [The core today](01-today.md) | What exists now, measured: crates, layers, traits, and where the same idea is written many times |
| 2 | [Principles](02-principles.md) | Lineage, composition, pure and partial abstractions, generic owners, dispatch, naming |
| 3 | [The roots](03-roots.md) | The handful of general abstractions everything descends from |
| 4 | [The lineages](04-lineages.md) | Each family, from its root down to its leaves, and what each generation adds |
| 5 | [The persona](05-persona.md) | A resident as one tree of parts, with the type at every node |
| 6 | [Crates](06-crates.md) | The crate layers from the floor to the node binary, and what each crate owns |
| 7 | [Modules](07-modules.md) | How a crate is laid out inside: one object, one directory, general above specific |
| 8 | [Coverage](08-coverage.md) | Every trait in the core today, and where it lands |
| 9 | [The plan](09-plan.md) | The changes that get us there, in order, with owners and the ratchets that say done |

## The model on one page

Everything that runs is a **part**. A part has an identity and a generation, owns its state, is made
of parts, lives on a worker, changes only through commands answered by completions, and is read only
through what it publishes. Every piece of work asks for resources with a **claim**: its value over
time, its essentials and its progress, which every governor decides by. Beside parts sit a few
general abstractions that are not parts: a **policy** decides, a **check** judges, a **sink**
records, a **source** feeds a mind's view, and a **durable** value outlives a process.

Each family descends from those roots through generations that each add one thing:

```
Part ............................................. identity, generation; parts, worker, commands, publications
├─ Model ......................................... owns a model it loads and releases
│  ├─ SpeechToText, TextToSpeech, VoiceActivity     speech
│  ├─ Embedder                                      embeddings
│  └─ InferenceAdapter                              generation
├─ Region ........................................ publishes an estimate, recomputed on change
│  ├─ FusionRegion                                  refines it by confidence and age (amygdala, Wernicke, vision)
│  ├─ Faculty                                       contributes to her workspace's turn
│  └─ Monitor                                       measures on a cadence floor
├─ Holder ........................................ holds governed bytes; footprint and reclaim
├─ Transport ..................................... carries envelopes to a peer
├─ Renderer ...................................... draws a body or a scene
├─ Room .......................................... an activity's shared state, read through its views
│  └─ Activity → Course                             a room run by a recipe; an Academy course
├─ Search ........................................ a query in, ranked candidates out (recall, genes, text)
├─ Genome ........................................ her genes as states of one object; lessons and learning
├─ Service ....................................... answers typed commands from outside the tree
└─ Persona ....................................... one resident, made of her branches

Claim ......... asks           Policy ..... decides       Check ....... judges       Sink ........ records
Source ........ feeds a view    Durable .... outlives a process
```

The crates follow the same direction: a floor of leaf crates that know nothing of Continuum, a
kernel that hosts parts, domain crates that each own a lineage (serving, media, perception, memory,
hands, grid, learning, the mind), and one node binary that composes them (chapter 6).

## How to use this book

- Before adding a type, find its parent in chapter 4. If no family fits, the new root belongs in
  chapter 3, and that is a design change for review, never a quiet addition.
- Before adding a crate or module, place it with chapters 6 and 7.
- Chapter 8 is the checklist: a trait not on it is not yet placed.
