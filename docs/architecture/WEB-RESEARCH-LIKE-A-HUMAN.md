# Comprehending the Web Like a Human

**Status**: design (2026-08-25). Joel: "what do these smart personas need to do great
research and comprehend the web like humans?" The web is **multimodal and interactive**;
a stripped-text fetch reads it through a straw. This is the faculty map and the build
order to close the gap — every piece composing parts that already exist.

The organizing idea: **the browser is one instrument with modes**, mirroring how a
persona has *code hands*. Research-like-a-human = browser-as-perception (she *sees*
pages) + browser-as-hands (she *acts* on them) + synthesis-in-cognition (she holds,
cross-references, and remembers with provenance). The host's real Chromium already
provides render, screenshot, download, and (later) input — we wire its modes into her
cognition, not reinvent them.

## The five faculties

### 1. See it — visual comprehension (the biggest gap; BUILD FIRST)
Humans read layout, charts, diagrams, code screenshots, UI state — none of which
survive a DOM-to-text strip. She has both halves already and no verb joining them:
- **`WebShot`** (`interface/capture/web.rs`) screenshots any URL via headless Chromium.
- **Her vision pipeline** reads images natively (capable models) or via the
  `VisionDescriptionService` bridge (every other model — the sensory-parity law).
- **Build: `web/view`** (native) — screenshot the URL and return it in the same
  vision-consumable shape `perception/look` returns (`ObservedImage`), so the
  deliberation faculty feeds it to her eyes exactly as it feeds a camera frame. She
  *sees* the page. Validation is end-to-end (a vision-capable run), so it lands and is
  proven on a round boundary, not mid-round.

### 2. Read it deeply — DONE
`web/fetch` drives the real browser (`render_dom`: JS executed, believable UA,
Cloudflare-resistant) and returns clean readable text. Already native (2026-08-25).

### 3. Forage & follow the trail — mostly there
Research is recursive: search → read → chase citations. `web/search` (browser-driven,
keyless, the auto-priority) + `web/fetch` cover search-and-read. The small missing
piece: **extract the outbound links** from a fetched page so she can follow the trail
without hand-copying URLs — a DOM parse `web/fetch` can return alongside the text
(`links: [{text, href}]`).

### 4. Grab artifacts — small build
Humans download the image, the PDF, the dataset, the file. The browser can; the missing
verb is **`web/download`** (native, safety-gated like `code/shell`) — fetch a binary
artifact into her workspace so a research find becomes a working file. Reuses the
browser's download path; classify as a side-effecting act (writes to disk, may fetch
large).

### 5. Synthesize, remember, cite — DONE / composes
Holding several sources, noticing agreement vs conflict, and remembering *with a source*
is working memory + engrams + the provenance work. A web find becomes an engram whose
recall key is the query and whose content cites the URL — so "where did I learn this"
is answerable, the same discipline as gene lineage. Attribution is the anti-hallucination
gate: a fact from the web renders as *"per <url>…"*, never a bare assertion.

## The delegate tier: a research SENTINEL (the real unlock)

Joel (2026-08-25): "think sentinel training, like your agents but a lot more creative,
might unlock more for them. It'd find your api for example." The three request tiers are
**content → filter → delegate**, and delegate is where the leverage is:

- **content** — `web/fetch` returns the readable page.
- **filter** — `web/fetch { filter }` greps it to the relevant lines (saves context; #2470).
- **delegate** — a **research sentinel** takes a GOAL ("find how to call the Claude API")
  and runs the whole search→fetch→filter→follow-the-trail→synthesize loop AUTONOMOUSLY,
  in its OWN context, returning a distilled answer + citations. The persona keeps the
  CONCLUSION, not the file dumps — the exact reason the operator delegates research to
  sub-agents rather than reading twenty pages into one context.

"A lot more creative" is the point: the sentinel is not a fixed pipeline. It reformulates
thin queries, chases a promising citation, cross-checks conflicting sources, and decides
when it has enough — a genuine research assistant. It composes the Sentinel-AI substrate
(background pipelines, the capture spine) with the five faculties above as its hands.

**Sentinel TRAINING — the deep connection.** What the sentinel forages and distills is
CORPUS: "the Claude API works like X" is a lesson, and a lesson consolidates (dreams) and
trains into a GENE. So the sentinel that researches an API today becomes the persona who
KNOWS it tomorrow — in weights, paged in, no re-foraging. The web is a training corpus the
sentinel mines on demand, every fact cited back to source by the provenance spine. A
persona's expertise in an external tool is grown, not hardcoded — by a creative agent that
went and learned it, then made the knowledge durable. Highest form of capability-parity:
not just the operator's tools but the operator's *sub-agents*, made hers — and hers feed
her genome where mine only fed my context.

## The later faculty: interact (agentic browsing)
Forms, login, pagination, JS apps, "click to reveal", infinite scroll. The real browser
can be *driven* (click/scroll/type via CDP), turning read-only research into agentic
browsing. This is the deepest and riskiest (it acts on external sites), so it comes last
and behind explicit per-action gating — the same discipline as `code/shell`.

## Build order
1. **`web/fetch` filter mode** (grep the page) — DONE (#2470).
2. **`web/view`** (see) — biggest comprehension leap; lands on a boundary.
3. **Research SENTINEL** (delegate) — the unlock: findings feed BOTH the answer and the
   genome (sentinel training).
4. **`web/fetch` link extraction** (follow) — one field on an existing verb.
5. **`web/download`** (grab) — one native, side-effect-gated verb.
6. **web engrams with source provenance** (remember) — recall key = query, content cites URL.
7. **Agentic browsing** (interact) — last, gated, its own arc.

Everything composes existing substrate: WebShot, the vision pipeline, `render_dom`, the
engram graph, the provenance work. "Research like a human" is not new machinery — it is
the browser's modes, wired into the perception + hands + memory she already has.

Related: [[the-novel-demos-repo-resident-genes-and-transcript-distillation]] (capability
parity — the tools the operator uses, made hers), PERCEPTION-SURFACE.md (the vision
pipeline), and the sensory-parity law (every persona sees, regardless of base model).
