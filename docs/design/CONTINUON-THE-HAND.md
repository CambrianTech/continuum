# THE CONTINUON — status orb, and the HAND of any AI

**Source: Joel, 2026-08-31 (verbatim fragments, typed on a phone — read for intent):**

> "the continuon (see reference in legacy and docs) was to indicate status ALONG
> with the favicon, dynamically, and instead we are posting this atrocious
> message: positron: state feed reconnecting — … [object Event] — retry #19 in 8s"
>
> "the old continuon actually was centered, looked like hal 9000 and was
> actually good"
>
> "it was the 'hand' of any ai"
>
> "when they screenshot we will see animations or same with click — like how you
> see tutorial vids — circle dynamically, loop around (its a rectangle of
> course) the element you are screenshotting or your mouse pointer — so that
> positronic control shows what the ai is doing, since we can playwright it —
> but more than that" *(thought continues; this doc grows when it does)*

## What the continuon IS

Not a rail badge. The continuon is the **AI's embodiment in the interface** —
two duties, one identity:

### 1. Status channel (SHIPPED 2026-08-31, first slice)

The orb + the favicon carry the state-feed status dynamically — green breathing
= live, amber pulse = connecting/reconnecting/cached, red still = closed.
**Never a text banner.** The retry bar with `[object Event]` in it was the
anti-pattern (deleted; engineer detail went to the console, harness contract
stayed on `<html data-feed-status>`). The favicon means the TAB shows health
before the page is even focused.

Heritage bar: the legacy continuon was **centered and HAL-9000-grade** — a
presence, not an icon. The current 14px rail orb is the compressed form; the
centered form should return where a surface has the room (boot screen, empty
states, the live-call face).

### 2. The HAND (next slices — the positronic-control visualizer)

When an AI acts on an interface — screenshot, click, hot-edit, scroll — the
surface SHOWS the act, tutorial-video style:

- **Observe/screenshot** → an animated rectangle sweep looping the observed
  element (`querySelector` bounds; full viewport = frame the viewport).
- **Click/pointer** → a ring pulse at the pointer location, travel path drawn
  compositor-only (offset-path, the PositronUniverse pulse discipline).
- **Hot-edit** → the edited element flashes its outline as it changes.
- Every act is attributed: the sweep carries the actor's name chip — you SEE
  *which* citizen's hand it is.

The signal path is positronic, not cosmetic: the acts already flow through the
substrate (`perception/observe`, `perception/hot-edit` land at the eye-node;
clicks will ride the same Provided-command seam). The core emits a typed act
event; every rendering surface subscribes and draws the hand. The animations
are TRUTH-DRIVEN (a sweep renders because a real act happened — same law as
universes, [[no fake data]]).

## Build plan (slices)

1. ✅ Orb + favicon status channel; banner deleted; `[object Event]` named
   honestly (sdk `StateConnection`).
2. **Act events**: core emits `positron:act` {actor, verb, target selector,
   room} when a perception/act verb executes against a surface. Rides the
   existing state-feed pipe (a small ViewState or event kind — reuse, don't
   invent a channel).
3. **The hand overlay**: one `<continuon-hand>` overlay element per surface —
   rect-sweep + pointer-ring + edit-flash, capped concurrency, reduced-motion
   stills everything, paint-budget laws inherited from Universe.ts.
4. **Centered HAL form**: the big orb returns on surfaces with room (boot,
   empty room states, live-call face) — one component, two sizes, same truth.
5. *(awaiting the "more than that" continuation)*

## Laws

- The continuon never fabricates: no idle theatrics, every animation traces to
  a real act or a real status change.
- One status source (`StateFeedStatus`) — orb, favicon, and `data-feed-status`
  all project it; no second opinion.
- Paint budget: compositor-only animations, capped live pulses, visibility
  pause, `prefers-reduced-motion` stills everything.
