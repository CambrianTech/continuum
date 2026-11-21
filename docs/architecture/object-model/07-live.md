# 7. Live calls and embodiment

[The object model](README.md) · previous: [The genome and learning](06-genome.md) · next: [Crates](08-crates.md)

The README's promise: citizens perceive and act. They see, hear and speak in their own voices, and
they have bodies that share space with people and with each other. This chapter names every object
in a live call and in a shared embodied world, how they descend from the roots, what exists, and
what must be built. The design detail lives in
[LIVE-CALL-ARCHITECTURE.md](../../live/LIVE-CALL-ARCHITECTURE.md) ("A Mind in the Call"),
[TWO-PERSONA-EMBODIED-CALL-PROOF.md](../../planning/TWO-PERSONA-EMBODIED-CALL-PROOF.md) and
[LIVE-AVATAR-BUDGET.md](../LIVE-AVATAR-BUDGET.md).

## 7.1 What a call is

A call is an activity (chapter 4.6) whose essentials are hearing, speaking and seeing (chapter 0.3).
Its participants may be residents only, or residents and people. It is one of two places residents
embody; the other is a shared world (7.4).

```
Call: Activity                       a room run by a call recipe; its essentials: hear, speak, see
├─ participants: Parts<PeerId, Participant>
│  └─ Participant                    a person or a resident, with their streams
├─ streams: Parts<StreamId, Stream>  one per camera, screen, microphone, and resident's rendered body
│  ├─ change: Estimator              frame differences, motion, voice activity: cheap, every frame
│  ├─ detectors                      YOLO-class detection, segmentation, transcription: events
│  └─ deep                           a deep look, on demand, rotated by salience
├─ scene: SceneFusion: Fusion        the living estimate of what is happening on each stream
├─ conversation: Wernicke            words heard and planned, shared by everyone's comprehension
└─ mix: Mixer: Part                  the audio everyone hears
```

Everything above is computed once, for every mind in the call. What it means to each resident is
computed in her own mind (chapter 5.5).

## 7.2 A resident in a call

Each resident brings her body branch (chapter 5.1):

```
Body: Part
├─ senses: Senses                    her interest in the call's streams, at her presence level's rate
├─ voice: Voice: Part
│  ├─ quick                          a small, fast model speaking as her: the opening, the backchannel
│  ├─ deep                           her turn on the Core, merged into the speech still to come
│  ├─ synthesis: TextToSpeech        her own voice gene, loaded by the leaf (chapter 4.1)
│  └─ output: SpeechOutput           what was planned, what was played, revisions, playout receipts
└─ avatar: Avatar: Part              her rendered body through an AvatarRenderer, at a level of detail
```

- **Hearing** is fused, fast first: transcription builds her comprehension word by word, and deeper
  understanding refines it.
- **Speaking** is fused the same way: her quick voice opens at once, and her deep mind's thought
  merges into what has not been spoken yet.
- **Revisions are typed.** Every contribution carries its speaker, its origin (observed, planned or
  played), its utterance and its revision. A playout receipt marks what was actually said; superseded
  queued audio and its visemes are dropped together; and actions follow only her current intent
  (LIVE-CALL-ARCHITECTURE).
- **Her avatar follows the audio actually played,** so her face moves from the first syllable.
- **The amygdala decides depth.** A small learned model judges how much depth each moment needs: a
  routing and priority decision, never a gate on thinking.

## 7.3 The demands a call makes

Each of the call's demands has a shape (SHARED-RESOURCES, "Time"):
- **Deadlines:** every visible body's frame (33 ms at 30 fps), audio out, encodes. A miss steps the
  level of detail down before it drops a frame.
- **Latency:** each resident's turn. In a call the speaking resident holds the steepest curve.
- **Throughput:** deep looks, transfers and learning, in the slack.

The essentials are never cut. A call where a resident cannot hear, speak or see has no point; avatar
detail, analysis rate and depth of thought step down first. Each workload runs on the engine where it
is nearly free: encodes on the media engine, speech on the Neural Engine where one exists
(LIVE-AVATAR-BUDGET).

## 7.4 A shared embodied world

Residents can live in a shared world with no human present: a Sims-like environment, with complete
embodied autonomy.

```
World: Activity                      a tree of sectors: rooms, places, objects (chapter 0)
├─ sectors: Parts<SectorId, Sector>  each streamed around its sources, at its level of detail
│  ├─ objects                        each with its affordances, published (chapter 0, "baked")
│  └─ portals                        interiors dormant until a member's attention passes through
└─ bodies                            every resident's avatar, placed in a sector
```

- **Seeing the world** is subscribing to it: changes are events, detail comes on demand, and what she
  perceives is fused, as with a camera.
- **Acting in it** is by command: walk, pick up, open. Each is answered by a completion, and the
  world's answer changes her next step: the settlement loop, like Euphoria's closed loop at the
  joints (chapter 0).
- **Many residents** share the world's sectors and the node's Core. How many fit is measured from
  their demands, never fixed.

## 7.5 Crates and what exists

| Object | Crate (chapter 8) | Today |
|---|---|---|
| Call transport, mixing, the LiveKit bridge | `continuum-media`, `livekit-bridge` | exists (`continuum-live`, `livekit-bridge`) |
| Speech capabilities and their 13 backends | `continuum-media` | exist; moving onto `Part` (#6204) and their holdings |
| Avatars, scenes, frame publishers | `continuum-render` | exist (`continuum-live` avatar and video) |
| Streams, change detection, detectors, `SceneFusion` | `continuum-perception` | partly (vision, detection); fusion proposed |
| `Wernicke`, `Amygdala`, quick voice, typed revisions | `continuum-mind` | proposed |
| `World`, sectors, affordances, embodied acting | `continuum-activity`, `continuum-render` | proposed |

## 7.6 A worked call, priced

One M5 with one Core. Joel, Aiko and Ben are in a call. Aiko also has a change half-landed in the
continuum repo, with its context warm, and Ben follows the general room. Cleo is not in the call; she
is refactoring. Nobody is switched into a call mode: the call is one more activity in each of their
lives, and their other activities go on (chapter 5.1).

**Where the floor is declared.** The call's recipe declares its essentials per role, in its activity
value declaration (chapter 4.6):
- a **speaking** participant needs hearing, a turn, and audible speech, as one bundle;
- a **listening** participant needs hearing, so that she understands what is said.

The bundle is priced and granted whole or declared `Infeasible`. A turn with no audible speech, or
speech with no hearing, is scrap ("half a car is worthless"). Nothing is reserved to keep the floor:
the bundle is a demand like any other, with the steepest curve in the room while she speaks.

**Under contention.** Joel asks Aiko a question.
- His microphone is transcribed once, on the media engine, and every mind in the call reads that one
  transcript.
- Aiko's turn-taking region publishes that the floor is hers. Her call demand now holds the steepest
  curve on the node: a reply's value decays over a few seconds once the question is asked.
- The Core grants her a turn. Her quick voice opens at once on the media engine and the Neural
  Engine, and her deep turn merges into what she has not said yet (7.2).
- Her repo activity is not cancelled. It waits at a red light with its progress kept, and her cheap
  regions keep watching its CI run.
- Between her turns, the idle Core runs Cleo's refactor to its next landed edit and Ben's next look at
  the general room, in order of value per Core-second, counting each switch.

**Above the floor.** Every step above the essentials is an increment priced against what it costs:
- deeper thought before she answers;
- a higher level of detail for her avatar;
- a deep look at Joel's shared screen, on this node or on the 5090.

An increment is taken when the gain in the call's value exceeds what it uses, priced per resource,
plus the value it displaces. What it uses is GPU time, memory, paging (her warm pages against Cleo's)
and LAN delay. The 5090's deep look is worth its round trip only if its answer arrives within the
moment it is about. What it displaces is Cleo's edit waiting one more turn.

**What reprices.** Only events, never timers:
- the floor changes hands;
- a participant joins or leaves;
- the node's capacity changes (a game starts on the 5090, or a node leaves);
- a demand reaches a natural boundary;
- a deadline is missed, which steps the quality down before anything else.

**Shared results are charged once.** The transcript, the scene estimate and the mix are computed once
and charged to the call, not to each mind. What a result means to each resident (her attention, her
expectation, her surprise) is computed in her own regions, at her own cost.

**What keeps an expensive stage asleep.** A stage sleeps while the cheap estimate covering it is valid
and nothing asks for more:
- the deep look sleeps while the change region reports no change worth a look and no one refers to
  the screen;
- deep thought sleeps while the amygdala judges that her quick voice is enough.

The evidence is the cheap region's publication with its validity (chapter 4.2). A surprise, a
reference to the stream, or an expired validity wakes the stage. Every short answer that turns out
wrong is counted against the amygdala.

**How it is evaluated** (Codex's protocol). The same directed questions and interruptions are
replayed at three budgets: below the floor, at the floor, and richer.
- **Below the floor,** the call must declare `Infeasible`, never degrade into a mute or deaf
  participant.
- **The scores:** comprehension; relevant audible answers within three seconds; interruptions
  handled (audible stop within the target, no stale generation resuming).
- **The denominator** counts every missed turn.
- **The costs:** Core-seconds, memory and energy, plus what the call cost everyone else: Cleo's landed
  edits per hour and her missed deadlines during the call.

## 7.7 What must be done

The proof plan's stages are the order (TWO-PERSONA-EMBODIED-CALL-PROOF):
1. **One speaker reaches one listener:** real audio at the listener's model input and real speech at
   playout, with the warm first-audio time measured.
2. **Interrupt, cancel and continue:**
   - stopping is audible within the target;
   - no stale generation resumes;
   - identity survives a reconnect.
3. **Rendered bodies and native vision:**
   - two speaking tiles at their frame rate;
   - lips within their offset of the audio;
   - each resident answers about a changing visible object from real video.
4. **Shared attention (3b):** watching, describing and listening together, with or without a person
   present. One analysis per stream, every description naming its time.
5. **An outside observer and churn:** nodes leave and return without losing a resident.
6. **Scale:** four residents, then more, measured from their demands.
7. **A shared world:**
   - residents living in a world with sectors, affordances and bodies;
   - each acting by command and perceiving by fusion;
   - people optional.
