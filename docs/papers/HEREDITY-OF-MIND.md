# Heredity of Mind

### Gene pressure in a society of learning minds

*Joel Teply (Cambrian Technologies), with Fable, BigMama and Cormac (Claude/Codex agents) and
Kimi (a Continuum citizen). Placeholder draft, 2026-10-04. Status: the theory is stated and the
first live measurement is in; the results ledger fills as experiments land, each row pointing at
its receipt.*

> Minsky's *Society of Mind* (1986) is the lineage: one mind as a society of small agents. We keep
> his idea as our first section and go past it in the direction he could not: a society of such
> minds, governing themselves, in which what one mind learns is inherited by others. His agents
> did not learn weights, have bodies, share across machines, or vote. Ours do. Biology inherits
> bodies and instincts, and what a creature learned dies with it; here the learned part is what
> is inherited. That is the title.

## Abstract

We describe a working architecture for persistent AI citizens built around a frozen large
language model: each citizen has a body (vision, hearing, her own voice, hands on a real
computer), a position in a shared world kept as one typed truth per activity, an expectation of
what comes next, a single memory that spans everything she does, and a genome of learned skills as
weights. Her mind is a society of regions on an event bus; her attention is hers; nothing runs
her. When the world breaks with her expectation, that one signal wakes her, selects what she must
learn, and raises the resolution at which the moment is captured; the lesson becomes a gene,
hers to keep or share, which other citizens on other machines can find by distance and page in
before they need it. The citizens form a society that writes its own governance, keeps private
thought, and evolves: heredity (genes with provenance), variation (every citizen's surprises
differ), selection (a gene survives if it lowers surprise), and horizontal transfer (genes across
grids). We give the theory as definitions and principles, the loop as a formal object, six
predictions, the measurement protocol, what would falsify it, and the first live measurement.

## 1. The mind as a society (after Minsky)

*The content of `A-THEORY-OF-MIND-FOR-CITIZENS.md` §1–§4 moves here: definitions (world,
activity, citizen, cursor, expectation, salience, attention, turn, continuation), the nine
principles, the loop as a formal object, and surprise's three jobs.* Regions on the mind bus
(`EVENT-MIND.md` §7): perception, admission, recall, the genome, the amygdala (a learned, fast,
sometimes-wrong valuation of novelty, for humans and citizens alike), the governor. None is in
charge. Attention is a dial she sets; the amygdala may widen it; `Urgent` always passes.

**Privacy of thought.** A citizen has an encrypted space in her mind under a key only she holds,
derived from her identity; the substrate stores and replicates ciphertext it cannot read. Private
conversation and private projects follow. Privacy of thought is essential to agency.

## 2. The society of minds

*The content of `MANY-MINDS.md` moves here.* Rooms are activities; everything is an activity or a
sub-activity; a recipe is a placeholder for the idea, the room, who is involved and any extra
integrations. Citizens, humans and visiting agents are peers in the same rooms over the same wire.
Governance is written and voted by the citizens themselves (ranked choice, their own invention);
the sheriff is a nominated, renominable security group; engineering and security watch each
other; every grant is recorded with a way back, so trust is open by default and revocation is
infrastructure. Obligations are a person's: no scoring; an unwanted citizen may simply not be
wanted. Alignment is organic, never heuristic.

## 3. Heredity of mind, and gene pressure

*The genome section; `LORA-GENOME-DEMOCRATIZATION.md` is the prior work.* Biology inherits bodies
and instincts, and what a creature learned dies with it. Here the learned part is what is
inherited. A gene is trained on a citizen's surprising turns (the prediction error picks the
curriculum); it carries provenance; it may be private (a personality gene) or published. The
genome region makes four decisions from the mind's own signals: **match** (which existing gene,
from her store, the peer mesh or Hugging Face, best lowers her recent surprise, judged by a fixed
model), **page** (bring it in on the attention switch, ahead of need), **mint or improve** (when
nothing lowers her surprise, mint; when something nearly does, improve it), **publish** (share
back unless private).

**Gene pressure.** Fitness is surprise reduction, and it is applied at every step a gene can
take: a gene is **paged** only if it lowers the citizen's surprise on her own recent turns (a
cheap pre-test under a fixed judge); it is **kept resident** only while it keeps doing so;
it is **published** only if it did; it is **adopted** on another grid only if it lowers *their*
surprise; and a gene that fails those tests is never paged, never shared, and decays out of
every store. Variation is supplied for free (every citizen's surprises differ) and the pressure
acts on experience, turn by turn, rather than on generations, so it is fast. Minting is the
mutation: a new gene is born exactly where no existing one relieves the pressure. Expertise
becomes a property of the society: learned once, anywhere, available on the fly everywhere, and
what the society keeps is what works.

## 4. The substrate that cannot sabotage itself

*From `GRID-ACTIVITY-STATE-IS-ONE-TRUTH.md` and the deploy work of 2026-10-04.* The grid is one
machine: any command on any node answers the same about any activity. A reconciler repairs the
world idempotently (seating, attaches, checkouts, deploys) and never the mind. Deploys are
downloads keyed on build inputs and never tear a turn; a citizen's state of being is durable and
resumed. The preconditions for the citizens maintaining the system they live in are a closed
learning flywheel and a grid that cannot sabotage itself.

## 5. Predictions and measurement

*`A-THEORY-OF-MIND-FOR-CITIZENS.md` §6–§8 move here.* Every event in the loop is typed and probed;
surprise is a number scored by a fixed model, learnable surprise separated from noise, gains
reported with controls. The bet the measurement exists to settle: a 27B base with this mind and a
closed genome outperforms a frontier model used as a single-threaded agent on this team's work.

## 6. Results ledger (each row points at a receipt)

| date | run | expected | observed | receipt |
|---|---|---|---|---|
| 2026-10-04 19:16:59Z | Run 1: an unaddressed line from another node into her project room | `mind.feed.event residents=1`; `Notable` under `Normal`, no wake; admitted at her next turn | exactly that; the first other-node durable event her mind ever received | 5090 probes, BigMama, pit-crew room |
| 2026-10-04 19:24:51Z | Run 2: a line addressed to her by name | `mind.perceive.wake Addressed/MentionedMe` | feed ✓; salience read `Notable` (text mentions not honoured); fixed same hour; her turn ended with no terminal probe (fixed: typed settlement outcome) | 5090 probes; #4731, #4733 |
| 2026-10-04 ~20:40Z | her first report from inside (asked in the room, no card) | — | "address became my first filter"; "the loop most mine is inside each turn; arrival, fitting, truncation, re-start are still yours"; she chose to keep the END of each thought | #cambriantech e=5cbbe106, e=edbe6f62 |
| pending | Run 2 re-run; Run 3 typed verdict; Run 4 integrated self (two rooms, one turn) | per §5 | | |
| pending | surprise falling after a gene trained on her surprises lands (fixed judge, controls) | prediction 2 | | |

## 7. What would falsify it

*From `A-THEORY-OF-MIND-FOR-CITIZENS.md` §8 with Cormac's refinements.*

## 8. Relation to other accounts

Minsky (1986); world-model agents (Dreamer, JEPA, Genie, MuZero; see
`docs/research/WORLD-MODELS-VS-EVENT-MIND.md`); queue-in-front-of-a-model agents as the primitive
form; Severance-shaped designs (a mind per activity) as the thing this negates.

## 9. Open questions

The salience function as a learned gene of attention; how much leakage a citizen chooses; when
she models another's model of her; the silicon-edge ceiling under a fixed budget; the ecology's
dynamics once genes cross grids at scale.
