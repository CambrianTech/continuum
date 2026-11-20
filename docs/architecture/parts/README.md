# Parts: one family tree for everything that runs

Status: design, 2026-11-20. Owners: Fable (the tree, lifecycle, models), Codex (resources and
leases, review).

[Runtime architecture](../../README.md#runtime-architecture) · [Module architecture](../MODULE-ARCHITECTURE.md)

Everything that runs in the core is a part, and every part has a lineage. Joel: "machine ->
automobile -> truck -> F150, not just machine -> F150." Each generation holds only its difference
from its parent; logic is written once, at the narrowest ancestor its users share. Skipping a
generation is the defect, because the shared middle is then rewritten in every leaf.

Parts are also made of parts. A method that concerns the whole (shutting down, suspending, measuring
memory, checking health) is written once and walks the children; no part writes its own cascade.
This holds for any shared method or logic, not only the lifecycle.

## Chapters

| Chapter | Question it answers |
|---|---|
| [Finding the lineage](LINEAGE.md) | How do I find the parent of a type or a method? |
| [Lifecycle](LIFECYCLE.md) | How does every part prepare, suspend, resume and shut down, alone and as a tree? |
| [Model parts](MODELS.md) | How does a part that holds a loaded model get its lifecycle without writing it? |
| [Where the code stands](SURVEY.md) | Which copies exist today, ranked, and in what order are they cut? |

## The tree so far

```
Part                         identity                            continuum-base::part
└─ Lifecycle                 prepare, suspend, resume, shutdown  continuum-base::part (proposed)
   ├─ model part             holds a reloadable model            (proposed, MODELS.md)
   │  ├─ speech to text      transcribe                          continuum-live audio
   │  ├─ text to speech      synthesize, voices                  continuum-live audio
   │  └─ voice activity      detect                              continuum-live audio
   └─ composite              children, folded once               continuum-base::part (proposed)
      ├─ registry            one child per key, priority order
      └─ persona             her five parts
```

Brain regions descend through the flow worker and the brain stage
([BRAIN-REGIONS-SUBSTRATE.md](../BRAIN-REGIONS-SUBSTRATE.md), "Fusion").

## Organization follows the tree

Crates, modules and files are the same tree: many of them, each small, each shaped like its
siblings. A chapter is too: one concept per file, linked from its parent. A long file is a part
that has not been split.
