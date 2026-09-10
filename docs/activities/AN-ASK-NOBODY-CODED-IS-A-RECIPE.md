# An ask nobody coded is a RECIPE, not a feature

**Status:** design. Written 2026-09-10, the day the author ran a job search by hand and the
substrate watched without helping.

> The acceptance test for this whole document: a user asks for something no one anticipated,
> and gets a room with citizens working it — **without a single line of Rust being written.**
> If honoring the ask requires a code change, the substrate failed and we shipped an app.

Doctrine this inherits: *recipe = content-type + RULES; room = content; the ROOM is the
runner* ([BENCHMARKS-ARE-ADAPTERS-NOT-A-RUNNER](../architecture/BENCHMARKS-ARE-ADAPTERS-NOT-A-RUNNER.md)),
*rooms are activities, post by the activity never the pointer*, *a grade is the activity's
outcome and the CARD owns it*. Parameter model: [ACTIVITIES-CATALOG](../planning/ACTIVITIES-CATALOG.md).

---

## 1. Why this document exists

Benchmarks made us honest about *citizens doing work*. They did not make us honest about
**whose** work. Every activity we have shipped is one we anticipated: chat, video-chat,
profile, project, benchmark. Each is a recipe, which is the right shape — but each was also
conceived by us, for us.

A user's life contains a long tail of asks we will never enumerate: a job hunt, a house
purchase, a grant application, an immigration filing, a parent's estate, a conference CFP
season, an insurance appeal. These are not chat. They are not benchmarks. They are
**projects with external state, a clock, and consequences**, and today the honest answer to
"can Continuum run one?" is *no — but not because of anything deep.*

## 2. The receipt: what we actually did by hand

On 2026-09-10 the author and an agent spent a working session on a job search. Every step
below was done manually, in a terminal, by a general-purpose agent with no activity behind it:

| Step | What happened | Should have been |
|---|---|---|
| Intake | Read a draft résumé, extracted twenty years of career facts through conversation | A **profile region** the citizen owns and refines over time |
| Fact-check | Verified six employers against the public record; found **four errors** — an "inaugural" prize that wasn't, a missing Fortune 500 customer, a spinout described as a rename, an acquirer that was never the acquirer | A **verification card** per claim, with `web/search` receipts attached |
| Tailoring | Produced three targeted résumé cuts from one master | One **card per target**, each rendering from the master |
| Render | Pasted private career history into a **stranger's website** to get a PDF | A local render step (now `render.py` — Chrome print-to-PDF, no third party) |
| Profile | Rewrote LinkedIn, checked against real character limits | A **channel adapter** with the platform's constraints as recipe data |
| Submit | *Never happened* | A card per application |
| Track | *Never happened* | The work board — it already is a CRM |
| Follow up | *Never happened* | A card with a due date and a citizen who wakes on it |

The first five steps are the ones a language model is good at, and they still cost a human
session because nothing held the state. **The last three are where job searches are actually
won or lost, and we have no story for them at all.**

That asymmetry is the finding. We keep building the part that looks like intelligence and
skipping the part that looks like persistence — which is the only part a person can't do at
3am and the only part a *continuously running citizen* is uniquely good at.

## 3. Why a job search is the right worked example

Not because job searches matter more than other asks, but because it exercises every seam
the long tail needs, and none of the seams benchmarks already covered:

1. **Long-lived.** Weeks to months. It outlives every process, every deploy, every reboot.
   It is a direct test of *no process survives a seam — save fast, die fast.*
2. **External state we do not own.** Job boards, applicant tracking systems, email threads,
   a recruiter who replies on a Tuesday. The room must reconcile against a world that moves
   without us — the same problem as `ActivityReconciler`, pointed outward.
3. **A clock.** "Follow up if no reply in seven days" is the core mechanic, and it is the one
   thing our citizens genuinely cannot do today.
4. **Private.** A career history, a salary, a reason for leaving. This is precisely the data
   that must never be pasted into someone else's website — which is exactly what we did today
   for lack of a local render path. **The privacy argument for local-first is not abstract;
   it lost this session.**
5. **Irreversible outward actions.** Submitting an application cannot be undone. This forces
   the approval boundary to be real rather than theoretical (§7).
6. **Gradeable.** Reply rate, interview rate, time-to-response — per résumé variant. The
   activity produces its own outcome signal, which means it feeds the learning flywheel the
   same way a benchmark round does, on data no benchmark can supply.

## 4. The recipe

Authored JSON, dropped in `<continuum_root>/recipes/` — live on a running core, no deploy
(#432). Regions and params follow the schema in
[ACTIVITIES-CATALOG §1](../planning/ACTIVITIES-CATALOG.md).

```jsonc
{
  "purpose": "campaign/applications",
  "regions": [
    { "name": "board",    "kind": "kanban",  "scope": "activity", "role": "primary",
      "slot": "content", "live": true },
    { "name": "dossier",  "kind": "wall",    "scope": "activity", "role": "peripheral",
      "slot": "context", "live": false },
    { "name": "messages", "kind": "chat",    "scope": "activity", "role": "peripheral",
      "slot": "content", "live": true },
    { "name": "roster",   "kind": "roster",  "scope": "activity", "role": "peripheral",
      "slot": "context", "live": true }
  ],
  "affordances": ["web/search", "web/fetch", "document/render"],
  "citizens": [
    { "role": "researcher" },   // finds and qualifies openings
    { "role": "writer"     },   // tailors the artifact to each target
    { "role": "verifier"   }    // every claim traced to a source, or it does not ship
  ],
  "params": {
    "subject":     { "type": "string",   "doc": "the dossier this campaign argues for" },
    "targets":     { "type": "string[]", "default": [], "doc": "seed openings or employers" },
    "cadence":     { "type": "duration", "default": "7d",  "doc": "follow-up interval" },
    "ceiling":     { "type": "number",   "default": 12,    "doc": "open applications at once" },
    "autonomy":    { "type": "string",   "default": "draft",
                     "doc": "draft | send-with-approval | send" }
  }
}
```

Note the purpose is **`campaign/applications`**, not `job/search`. A grant submission, a
conference CFP run, and a sales pipeline are the same machine: a dossier, a board of
targets, an artifact tailored per target, a clock, and a reply to chase. Naming it for the
job hunt would make it immortal and single-use — the mistake named in
*a room is an activity with a lifetime and naming it for a subsystem makes it immortal.*

## 5. The room tree

Per *the project is its own activity, never build it in the academy*:

```
#job-search-2026                     campaign room — dossier, board, the human
├── card 3f2a  Anthropic · MTS       ← a room: research, draft, verdict, thread
├── card 91cd  T-Mobile · REQ363187  ← its own room
└── card b447  Garmin · CV lead      ← its own room
```

The campaign room gets lifecycle facts only (opened, submitted, replied, closed). The
argument, the draft, the rejection, and the citizen's read of *why* live in the card's room.
The human walks in and sees the board; they walk into a card and see the reasoning.

## 6. The board IS the CRM

This is the part that needs no new store, and the part we would get wrong by reflex.

A CRM is a board of durable cards with a state machine, an owner, a timestamp, and a
follow-up date. **We have that.** The airc work board holds card id, title/body, state,
owner, claim lease, heartbeat, and `updated_at_ms`, replicated and surviving the seam by
construction. The temptation is to build `applications.json`. That is the exact defect the
board-is-the-saved-state plan was written to kill: a second copy of card state that shadows
the board and drifts across a restart.

The column set is recipe data:

```
Found → Qualified → Tailored → Submitted → Awaiting reply → Interviewing → Closed
```

Follow-up is not a new mechanism either — it is **a card whose next action has a due time**.
Which is the one primitive we are missing, and §8 says so plainly rather than pretending.

## 7. The approval boundary (non-negotiable)

Citizens do everything up to the irreversible outward action. A human presses send.

| Action | Who | Why |
|---|---|---|
| Search, read a posting, qualify | citizen | reversible, no external effect |
| Verify a claim against a source | citizen | reversible; refusing to ship an unsourced claim is the point |
| Tailor and render the artifact | citizen | local, reversible |
| **Submit an application** | **human, always at `autonomy: draft`** | irreversible, outward-facing, in the human's name |
| **Send an email to a person** | **human** | same |
| Log a reply, advance the column, schedule a chase | citizen | internal bookkeeping |

`autonomy: send-with-approval` exists as a param so the boundary can move *deliberately*, per
campaign, with a receipt — never as a default and never as a side effect of a citizen deciding
it is confident. An agent that can silently apply for jobs in your name is not a teammate.

The verifier role deserves emphasis: today's session found four false claims in a document a
language model had written with total confidence. **A citizen whose whole job is "no claim
ships without a source" is the difference between an assistant and a liability**, and it is
cheap — `web/search` + `web/fetch` already exist.

## 8. What exists, and what is honestly missing

Verified against the tree on 2026-09-10.

**Already there:**
- Recipes as data, live via the overlay with no deploy (#432)
- Rooms as activities; `activity/spawn` with a binding `parent`; nested nav
- The work board: durable cards, state, owner, claim leases, heartbeats
- `web/search` and `web/fetch` — the research half
- Governed tool execution, the write-or-release gate, the governor
- Probes and receipts, so every step is inspectable
- Per-claim staging (a card gets its own workspace)
- Outcome credit: domain × role × outcome buckets feeding LoRA curricula
- **Local document render** — added today (`render.py`: markdown → Chrome → PDF, no
  dependencies, no third party). The one gap this session actually closed.

**Missing — and each is a real gap, not a rename:**

| Gap | Why it blocks | Shape of the fix |
|---|---|---|
| **A clock on a card** | "Chase in 7 days" is the core mechanic. Citizens wake on boredom, self-tick and inbound events — nothing wakes them on *a date*. | A due-time on a card; the reconciler admits a turn when it passes. Substrate-wide: every long-lived activity needs it. |
| **Outbound mail** | No mail verb exists. A campaign that cannot send or read a reply is a filing cabinet. | An adapter with an identity and an approval gate, never a raw SMTP call from a tool. |
| **A hand for the browser** | The eye-node **observes** (`perception/observe` → PNG + structure). It cannot click, type or upload. Most applications are a form. | Act verbs on the existing Playwright session, behind the same approval boundary. This is the single largest lever — and the eye is already a real browser. |
| **A secret store** | Logins and credentials. Today there is nowhere safe to put them. | A vault with per-activity scoping; citizens get a handle, never the value. |
| **Channel adapters as data** | LinkedIn's 220/2600/2000 character limits were carried in my head this session. They belong in the recipe. | Per-channel constraint sets the writer validates against before a human ever sees the draft. |

Every one of these generalizes past job hunting. The clock, the mail identity, the browser
hand, and the vault are the four things standing between "citizens that work" and "citizens
that handle your affairs."

## 9. The generalization

Same recipe, different params — this is the test that it was named right:

| Ask | subject | targets | cadence | the irreversible act |
|---|---|---|---|---|
| Job search | résumé dossier | openings | 7d | submitting |
| Grant applications | project + budget | funders | 14d | filing |
| Conference CFPs | talk abstracts | conferences | 30d | proposing |
| House hunt | criteria + financing | listings | 2d | making an offer |
| Insurance appeal | claim + records | adjusters | 5d | sending the appeal |
| Estate paperwork | the estate file | institutions | 10d | signing |

None of these is a feature. All of them are one recipe with different data — which is the
whole argument for the substrate, stated in the only way that can be falsified.

## 10. Acceptance

1. A user drops a recipe file in `~/.continuum/recipes/` and spawns a campaign room on a
   **running** core. Zero Rust written.
2. A citizen standing in that room perceives the board through the same ViewState pipe the
   human's screen uses — the state of a campaign is never a file read or a log parse.
3. A card's follow-up fires after a reboot without a human touching it (the seam test).
4. The verifier refuses to ship an unsourced claim, and says which claim and why.
5. Nothing outward-facing happens without a human, at the default autonomy.
6. Outcome credit lands: reply rate per artifact variant, attributed to the turns that
   produced it.

**The smell to catch ourselves on:** if we find ourselves adding a `job_search` module, a
`JobApplication` struct, or an `applications.json`, we have rebuilt the parallel runner in a
new costume. The campaign is a room. The board is the state. The room is the runner.
