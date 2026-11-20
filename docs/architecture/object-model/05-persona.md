# 5. The persona

[The object model](README.md) · previous: [The lineages](04-lineages.md) · next: [Crates](06-crates.md)

A resident is one part made of parts. This chapter gives her tree with the type at every node, says
which branches are durable and which are cache, and maps today's pieces of her onto it. How she runs
(frames, turns, learning, swaps) is in [HOW-A-MIND-RUNS.md](../HOW-A-MIND-RUNS.md).

## 5.1 Her tree

```
Persona: Part                               a fixed set of branches (parts!)
├─ identity: Identity                       PeerId, keypair, home, name, role                 durable (airc)
├─ mind: Mind: Part
│  ├─ workspace: Workspace: Part            one turn at a time; reads its faculties           cache
│  ├─ attention: Attention: Region          her dial and focus                                durable (MindState)
│  ├─ amygdala: Amygdala: FusionRegion      how much depth now                                cache; its model is a gene
│  ├─ wernicke: Wernicke: FusionRegion      words heard and planned                           cache
│  ├─ faculties: Parts<_, dyn Faculty>      recall, deliberation, act-then-speak, deferred    cache
│  └─ memory: Memory: Part                  her hippocampus: Recall searches over her engrams engrams durable
├─ genome: Genome: Part
│  ├─ genes: GeneSet: Part                  stable genes, composed per turn                   durable
│  ├─ shadow: Shadow: Part                  the layer she learns into (Loaded)                durable at checkpoints
│  ├─ lessons: Lessons: Part                queue, journal, credits                           durable
│  └─ regions                               dream consolidation, credit acceptance            cache
├─ body: Body: Part
│  ├─ voice: Voice: Part                    her quick voice and her voice gene; speech out    gene durable
│  ├─ avatar: Avatar: Part                  her rendered body, through an AvatarRenderer      cache
│  ├─ senses: Senses: Part                  her subscriptions to rooms and perception         durable (cursors)
│  └─ hands: Hands: Part                    her tools, as working sets per activity           cache
├─ residency: Residency: Holder
│  ├─ blocks: Blocks                        her KV pages, held by the Core                    cache, saved on stop
│  └─ tokens: Tokens                        her token history, appended once                  durable
└─ presence: Presence: Part                 her membership in rooms (airc)                   durable
```

Every branch answers the lifecycle commands and the same queries through the tree. Suspending her
is one broadcast: each branch saves its durable state, demotes its pages, and answers `Done` or
`Pending` on what it is still writing.

## 5.2 Her level of presence

She is always at one of four levels of presence (chapter 0, "Bubbles of fidelity"), and attention
moves her between them:
- **acting:** on the Core, at full detail;
- **attentive:** warm, about to act;
- **present:** following her rooms at her dial's level of detail;
- **at rest:** her durable self and a wake condition.

A level is a statement about her caches only. Promoting her is `Prepare` and `Resume` reaching the
branches that level needs, and demoting her is `Suspend` reaching them, through the same tree.
Her durable branches are the same at every level.

## 5.3 Durable and cache

A branch's durable state is who she is; its cache is how fast she is. Persisting her persists every
durable branch at its seam, each as a `Durable` value:

- her identity in airc;
- `MindState` (her activity cursors, continuation and attention dial), saved at each turn's seam
  ([EVENT-MIND.md](../EVENT-MIND.md));
- her engrams, through the ORM medium;
- her genome's genes, snapshots and learning checkpoints, with the journal cursor they consumed
  through;
- her token history.

Her KV pages are cache: saved on a stop so that a restart restores them, and rebuilt from her durable
state when they cannot be restored.

## 5.4 What she shares and what is hers

The node's perception streams, the Core, the governor and the rooms are not hers. She holds handles
to them, and her branches read their publications:
- her senses subscribe to a room's views and to perception's fused estimates;
- her residency's blocks are held by the Core;
- her voice and avatar draw on the node's shared speech and render parts, through select dispatch.

What is computed once for everyone (an embedding, a detection, a transcription) stays in the shared
part; what it means to her is computed in her mind.

## 5.5 Today's pieces of her, placed

| Today | Where it is | Where it lands |
|---|---|---|
| `PersonaUnit` and `PersonaParts` (blocks, tokens, genome, learning, decode) | `continuum-persona-engine` | `Persona`'s residency, genome and lessons branches; `decode` is the Core's handle to her |
| `PersonaCognition`'s engine and adapter registry | `continuum-mind::persona::unified` | her deliberation faculty, and the node's `InferenceAdapter` parts it selects from |
| its inbox, rate limiter and sleep state | same | her senses and attention |
| its domain classifier, message cache and content dedup | same | shared perception publications she reads; dedup is a property of the shared cache |
| its admission and recall metadata | same | her memory branch |
| its airc and roster sources | same | `Source`s of her view over the rooms she is in |
| its capture sink | same | a `Sink` |
| `PersonaAircRuntime` (inbound handle, command pump, heartbeat, owned tasks) | `continuum-mind::persona::airc_runtime` | her presence branch; its tasks become `Task`s owned by that branch |
| `HippocampusModule`, node-wide | `continuum-core` | her memory branch, one per persona; the store holds the bytes |
| `WorkspaceCycle` and its faculties | `continuum-cognition::workspace` | her workspace and faculties |
