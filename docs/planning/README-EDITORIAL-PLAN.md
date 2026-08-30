# README Editorial Plan — single authorship, clean rules

*Joel, 2026-08-30: "put a real plan in place to clean this up. Read the whole
thing and it's all got great content but needs real single authorship and all
these clean style rules. I like what I read." The content is right; this plan
makes the telling match the engineering.*

## The voice (single authorship, defined)

One author throughout: **declarative, receipts-backed, warm but never
salesy.** The reference register is the validated purpose prose ("Minds you
host, not assistants you rent" / "It takes both kinds") — sentences that
carry a claim and its warrant in one breath. Concretely:

- Claims state facts and cite artifacts; stakes are *labeled* as stakes.
- No hype adjectives doing load-bearing work ("revolutionary", "blazing") —
  the receipt is the superlative.
- Analogies earn their keep once, then retire (the Cambrian puddles metaphor
  appears exactly once, at the top, where it's strong).
- Second person for the reader's actions ("you re-run", "your machines"),
  first person plural sparingly, never "we believe".
- One idea per paragraph; a paragraph that needs a breath gets split.

## The style canon (mechanical, rachetable)

1. **First-use word links** — the term is the link, defined section as
   target; later uses plain. Never naked URLs or `(see docs/…)` piles in
   prose. (Sweep running — pass 1.)
2. **Charts are projections of receipts** — generated, snapshot committed
   beside the SVG, regeneration command named in the caption.
3. **Numbers carry their regime** — a score without model+build+hardware is
   not printable.
4. **Terms are consistent**: *citizens* (the beings), *personas* (the
   implementation concept — use when discussing architecture), *the Grid*
   (capitalized, first-linked), *substrate* (the Rust core). Pick per
   sentence by register, never interchangeably within a paragraph.
5. **Honesty blocks stay** — the "Where we are — honestly" pattern is a
   brand asset; every major claim section keeps its status line.
6. **Screenshots dated** — every image caption says when it was real.

## Structure target (~450 lines from 1084)

The README is a **prospectus**: it sells the thesis and hands off depth.
Long essays move to docs/ and leave a linked paragraph behind.

| Section (current) | Disposition |
|---|---|
| Hero + Cambrian + two-panel images | KEEP, tighten to ~30 lines |
| "What that looks like in practice" | KEEP — best on-ramp |
| Grid / misfit-hardware essay | TIGHTEN to thesis ¶ + link (docs/THE-GRID-IS-ALIVE.md absorbs depth) |
| "This Is Not What You Think It Is" | MERGE best lines into hero + Colleagues section |
| Research section (new) | KEEP as-is — the receipts front door |
| Architecture in Four Pictures | KEEP |
| Getting Started | KEEP; verify every command against a fresh clone at edit time |
| Colleagues, Not Tools + continual learning | KEEP, single-voice pass |
| Pseudo-AI vs true AI table | TIGHTEN — table stays, prose halves |
| Startup on One Machine / Working Dynamic | MOVE to docs/, one ¶ remains |
| Compounding / mesh-beats-datacenter | TIGHTEN to the distilled thesis + payoffs; depth → docs |
| Academy / Genomic Intelligence / Delegation / Orchestration / Efficiency | Each: lead ¶ + strongest receipt stays; essays → their architecture docs |
| Sentinel Engine, Factory, tail sections | Same pattern |

## The passes

- **P1 — link hygiene** (running now, agent + review): the canon's rule 1
  applied document-wide. Lands on PR #2598.
- **P2 — single-voice rewrite**: section-by-section, top to bottom, me
  writing, Joel skimming diffs (his taste calls are the gate). Each section
  is one commit so review is per-section. Target: voice rules above, no
  content loss — anything cut is MOVED with a link, never deleted.
- **P3 — structure consolidation**: the table above executed; docs absorb
  essays; README lands ≈450 lines. Anchor-integrity check after moves.
- **P4 — the read-aloud pass**: full top-to-bottom read for cadence; fresh-
  clone command verification; final counters.
- **P5 — the ratchet**: a source-hygiene-style CI check on README: naked-URL
  count and `(see docs/` count may never rise (same law as the unwrap
  ratchet — new sloppiness cannot enter once cleaned).

## Definition of done

A first-time ML reader gets: thesis in 30 seconds, receipts in 2 minutes,
running system in 15, and never hits a visible URL, an unregimed number, or
a paragraph that sounds like a different author. The paper and README share
the canon — one voice across the front door and the write-up.
