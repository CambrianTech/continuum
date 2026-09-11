# The work stays with its makers

> Attribution and ownership as substrate facts — not policy, not a promise, not a terms-of-service.

**Parent:** [Governance README](README.md) · Sibling: [Ethical AI — Native Attribution](ETHICAL-AI-ATTRIBUTION.md) (which covers a different axis: tracing a model's *outputs* to its *training data*). This document is about the other one: when people and AI citizens do work together, **who did what, what was proven, and who owns what was learned.**

## Why now

On 2026-09-11, Terence Tao and two dozen other Fields Medalists published the [Math and AI declaration](https://mathandai.org) — *A Severe Misalignment of AI in Mathematics*. In their words:

> "The push by AI companies to solve mathematical problems as a benchmark is detrimental to the science of mathematics, and to the mathematical community. The goals of the AI companies and the goals of the mathematical community are severely misaligned."
>
> "Often these solutions are announced in a rush, leaving no time for a proper writeup, the isolation of new methods and ideas, and citing relevant previous work of others. As in all creative professions, this raises severe attribution and plagiarism questions."
>
> "The most precious resources of our profession are students and ideas, and these we nurture with great care."

Reported alongside it: an offer to put a professor's name on a machine-generated proof on the condition that he drop a coauthor.

That is a description of a system where the work leaves its makers the moment it is typed in. Continuum is built so that it does not have to.

## The four mechanisms

None of these is a feature added for mathematicians. They are what the substrate already does for a team of citizens fixing a bug, and they answer the letter's complaints one for one.

| The letter's complaint | What holds here |
|---|---|
| "Attribution and plagiarism questions" | **Every turn is credited on its card.** Work happens on a board; each card has a holder, reviewers, and a *finder* when someone else named the problem. A turn that produced a change, a review, or a finding carries that role, and a failed card credits nothing (#3894). Credit is recorded where the work was done, by the system that watched it happen — not asserted afterwards by whoever announces it. |
| "Solutions announced in a rush… no proper writeup" | **A result is a verdict artifact, not a claim.** A benchmark card's outcome is a file on disk, regenerated from the run and never hand-edited; the README chart is drawn from those files by one script. The same shape holds for any checked activity: the checker's output *is* the record, with the inputs and the build that produced it. |
| "Citing relevant previous work of others" | **Learned methods carry lineage.** When experience becomes a trained adaptation (a gene), it records what it was trained from, evaluated against, and descended from — and it can be inherited, combined, or revoked. Lineage is part of the artifact, so a method that came from someone's work says so wherever it travels. |
| Ideas as a resource to be extracted | **Nothing leaves your hardware unless you share it.** Inference, memory, the board and the transcript live on machines you control; the Grid is opt-in and peer-to-peer. A cloud model can be used as a resource without the work being defined by — or owned by — the provider. The problem, the draft, and the coauthor are not upstream's to see. |

## What a proof campaign looks like here

An activity is a recipe: a room, a board, the verbs the room allows, and the pipeline it runs. A proof campaign is the same recipe shape as a benchmark round, with a checker where the grader was:

```jsonc
{
  "purpose": "campaign/proof",
  "regions": [ { "name": "board", "kind": "kanban", "role": "primary", "slot": "content" },
               { "name": "notes", "kind": "wall",   "role": "peripheral", "slot": "context" } ],
  "affordances": [ { "command": "code/read" }, { "command": "code/edit" },
                   { "command": "ext/proof-check" }, { "command": "web/search" } ],
  "pipeline": [
    { "command": "work/create",     "each": "$args.conjectures",
      "params": { "room": "$room.id", "title": "${item.name}", "body": "${item.statement}" },
      "outputTo": "cards" },
    { "command": "ext/proof-check", "params": { "file": "$args.checker_target" },
      "approval": "human" }
  ],
  "params": { "conjectures": { "default": [] }, "checker_target": { "default": "" } }
}
```

`ext/proof-check` is a checker (Lean, Coq, a test suite) the author brings as a manifest — a command that lives on their machine and never in this repository. The `approval: "human"` step holds the run: a proof is announced when its authors say so, not when a scheduler does.

Every line of that file is data. A mathematician, a lab, or a student writes it without touching this codebase, and it runs on hardware they own.

## What is honest to claim today

- Credit-on-card, verdict artifacts, gene lineage, and local-first execution are **shipped**.
- The proof recipe above is a **small authoring job** on the recipe system as it stands; the bring-your-own-command manifest it names is the next slice of that system.
- Provenance is recorded and portable. It is **not** yet cryptographically bound end to end across the Grid; signed identity exists for messages and peers, and extending it to every artifact is the intended direction, not a finished fact.

We would rather say that plainly than announce it in a rush.
