# 7. Modules

[The object model](README.md) · previous: [Crates](06-crates.md) · next: [Coverage](08-coverage.md)

Inside a crate, the module tree is the lineage tree. A reader finds a type by walking from the general
down to the specific, and every directory holds one object or one family
(MODULE-ARCHITECTURE §9.3: one object, one directory).

## 7.1 The rules

- **A lineage is a directory.** Its `mod.rs` declares the family's trait and nothing else; its generic
  owners each have a file beside it; each leaf is a subdirectory (or one file, when it is small).
- **General above specific.** A directory never reaches down into its leaves' internals, and a leaf
  never reaches sideways into a sibling: siblings share only what their parent declares.
- **The crate root is the surface.** `lib.rs` declares the crate's lineages and re-exports their
  traits, generic owners and leaf constructors. Nothing else is public.
- **One concern per file, and small.** A file holds one type and its methods. The 1,000-line gate is a
  ceiling, not a target.
- **One test module per file**, beside the code it tests, as the test rules in CLAUDE.md require.
- **No `utils`, `common` or `helpers`.** A function that does not belong to an object is a missing
  object or belongs to the floor crate whose one job it is.

## 7.2 Example: the roots, `continuum-base`

```
continuum-base/src/
├─ lib.rs                 pub mod part, policy, check, sink; re-exports
├─ part/
│  ├─ mod.rs              trait Part, Identity, Generation
│  ├─ parts.rs            Parts<K, P>; the parts! declaration; aggregate, broadcast, select
│  ├─ command.rs          trait Command, Dispatch
│  ├─ completion.rs       Completion<A>: answers as they arrive, rejoin, reattach
│  ├─ outcome.rs          Outcome<P, E>
│  ├─ published.rs        Published<S>
│  ├─ task.rs             Task
│  ├─ lifecycle.rs        Prepare, Suspend, Resume, Shutdown
│  └─ holding/
│     ├─ mod.rs           trait Model, trait Remote
│     ├─ loaded.rs        Loaded<M>: a model's lifecycle, once
│     ├─ session.rs       Session<S>: a remote's lifecycle, once
│     └─ slot.rs          the reloadable model slot (today's ReloadableModel)
├─ policy.rs              trait Policy
├─ check.rs               trait Check
└─ sink/
   ├─ mod.rs              trait Sink
   └─ noop.rs, ring.rs, jsonl.rs, fanout.rs
```

## 7.3 Example: a domain lineage, `continuum-media::speech`

```
continuum-media/src/
├─ lib.rs                 pub mod speech, call, mix; re-exports
└─ speech/
   ├─ mod.rs              the voice service's Parts of each capability, and their select policies
   ├─ to_text/
   │  ├─ mod.rs           trait SpeechToText: Part (transcribe)
   │  ├─ moonshine/       impl Model (load) and SpeechToText (transcribe); held as Loaded<Moonshine>
   │  ├─ whisper/
   │  ├─ openai_realtime/ impl Remote (open) and SpeechToText; held as Session<OpenAiRealtime>
   │  └─ stub.rs
   ├─ to_speech/
   │  ├─ mod.rs           trait TextToSpeech: Part (synthesize, voices)
   │  ├─ kokoro/  orpheus/  piper/  pocket/
   │  └─ silence.rs
   └─ activity/
      ├─ mod.rs           trait VoiceActivity: Part (detect)
      ├─ silero/  silero_raw/
      └─ webrtc.rs  rms_threshold.rs
```

A leaf directory such as `kokoro/` holds what only Kokoro needs: its phonemizer, its session, its
`load`, its `synthesize`. It holds no lifecycle code (`Loaded` supplies it), no name or description
outside its `Part` impl, and no registry.

## 7.4 Example: the persona, `continuum-mind`

```
continuum-mind/src/
├─ lib.rs
└─ persona/
   ├─ mod.rs              Persona: the parts! declaration of her branches
   ├─ identity.rs
   ├─ mind/
   │  ├─ mod.rs           Mind
   │  ├─ workspace/       Workspace and its turn
   │  ├─ attention.rs
   │  ├─ amygdala/        a FusionRegion leaf
   │  ├─ wernicke/        a FusionRegion leaf
   │  ├─ faculties/       recall/, deliberation/, act_then_speak/, deferred/
   │  └─ memory/          her hippocampus over continuum-memory's Recall
   ├─ body/               voice/, avatar/, senses/, hands/
   ├─ residency/
   └─ presence/
```

The genome branch lives in `continuum-genome` (layer 3) and is a field of `Persona` by type, not a
directory of this crate.
