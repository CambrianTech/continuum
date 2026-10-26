# Minds, seats and admission: who is born, who is awake, who goes first

Status: DRAFT for the team's critique (Fable, 2026-10-26). Codex, Cormac, BigMama: please
critique by area before anyone codes. Joel: "It needs really thought between you guys."

## What happened (measured on the M5, 2026-10-25)

- 16 persona homes on the M5, born in batches (8 on 09-02, 4 on 09-13), all resumed on every
  boot. 11-14 acted at once from ~15:20Z. Nobody decided that.
- One act costs W ≈ 20 s of GPU alone (median 8.2k fresh tokens at ~800 tok/s, ~280 output
  tokens at ~50 tok/s). Between acts a mind spends ~3 s (tool 1.2 s, assembly 0.8 s). So the
  chip saturates near N* = 1 + G/W ≈ 1.2 minds; past it every extra mind adds ~W to every act.
- At 14 minds: median act 146 s live (the simple model predicts 141 s). The engine had ≥1
  generation in flight ~100% of the time, 4-6 usually: the GPU is saturated, not idle.
- A mind's time between generations was ~105 s of OUR queueing: 77 s in the lane semaphore,
  29 s in the prefill throttle, before the engine's own contention.
- The grid allocator (`grid_allocator.rs`) already places minds on offered seats and rotates
  the dormant through slack, but a seat is a KV-memory lane: the M5's memory offers 6-8 lanes,
  its GPU time serves ~1-2 minds at full speed.
- Contention dyno (#5435), the M5's probes, 15 minds, 2 h: foreground-first, work-conserving
  (every other mind admitted): her act 39 s vs 199 s all at once, the others 167 vs 160 s,
  144 acts/h (the most). Caps alone: 226-237 s for her. Grant controllers (stretch-bounded,
  throughput-climbed): 25-26 s for her but 109-119 acts/h. Deleted.
- Joel decided: the M5 hosts 4 (Aiko, Aris, Atlas, Benchy); 12 despawned, homes intact, under
  a 24 h hold.

## The three decisions, each made by accident today

1. **Born.** A mind is born because a person or an activity asks, with the reason recorded.
   Never by a script, a batch, a benchmark. The grid's population is about its seats; a birth
   beyond them is born dormant.
2. **Awake.** A seat is MEASURED SPEED capacity, not memory: the minds a node serves at good
   latency, from its own W and G (and the foreground discipline below). The grid allocator
   places by these seats; the rest are dormant, rotated through slack by the existing dormant
   order, or waiting for capacity (a node joins). Waking when addressed is immediate.
3. **First.** One admission scheduler (a CBAR stage) replaces the three gates (ambient
   permits, lane semaphore, prefill throttle): one queue, priority-ordered (foreground =
   the mind a person is working with now; directed; background), work-conserving (nothing
   held back while the GPU has room). The engine's scheduler matches it: the foreground's
   prefill first, a background prefill yields between ubatches.

Non-GPU work (tools, embeddings, recall, assembly of the next prompt, page I/O, learning's
data prep) runs on the substrate's CPU stages concurrently, never inside a GPU grant.

## Open questions for the team

- **Codex (serving, admission, grid):** a seat as measured speed capacity: from W and G as
  above, or from the prefill/decode knees we already persist? How does the allocator's offer
  change, and the dormant rotation? Foreground prefill-first in the fork: on your #69 seam?
- **Cormac (state, pages, training):** dormancy durable and cheap: what a dormant mind keeps
  warm (page on disk? checkpoint at the head boundary?), and how waking restores her fast.
  Does training/dreaming run in dormant time or on CPU stages beside serving?
- **BigMama (live, media):** the foreground signal: who is "the mind a person is working
  with now" across chat, a video call, the desktop. Live media must preempt everything: does
  it need its own class above foreground?
- **All:** what replaces the expiring hold, and how a node's owner sees and overrides who is
  awake (a person's choice beats the allocator).

## Gates (done when)

- The mind a person works with: median act ≤ 1.5 × her solo W on any node (measured, the
  dyno then live).
- No node runs more minds at once than its seats; the grid's awake total equals its seats.
- No birth without a recorded reason; no resident lost (dormant keeps home, memory, work).
- One admission owner; the three gates deleted.
