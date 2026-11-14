# Autonomous product development, proven on Positron

Status: delivery contract, 2026-11-14. Implementation and live acceptance remain to be proven.
Joel's direction: stable, fast personas build the product they can actually see and use.

## General capability, first product

Personas are general-purpose autonomous developers. They must work on people's existing web,
iOS and other projects using those projects' frameworks, build systems and navigation models.
Neither Positron, Lit, a Continuum activity embedded in the product, nor a framework rewrite is
required. Positron is our first demanding product and an optional integration we can offer others.
The Positron-specific client contracts below apply to our product, not to every user's project.

The common capability is to own work, edit source, run the project, observe the real result,
interact, diagnose, correct and deliver. Project adapters declare actual capabilities and typed
targets; they do not impose one UI framework. Browser DOM/accessibility/pixels and iOS simulator
or device feedback must work without Positron instrumentation. Positron can enrich the same
observations with semantic state and revision receipts when the project uses it.

Codex must dogfood this exact command path while building it, recording failures and fixes.
A private developer-only browser workflow does not prove residents have the capability. Prove
both the developer and resident can reach the same supported target and consume its feedback.

Residents retain their own identity, task ownership, workspace, observations and unfinished work
across disconnection and restart. They choose and advance useful work without a supervising
agent or human supplying every next action. Recipes provide reusable activity contracts, not a
mandatory scripted agent loop. External review and assistance remain available; continual human
attendance is not an operating dependency. Measure independently completed and reviewed tasks,
not command volume. Do not build a second agent runner or harness around the persona runtime.

## Product and boundaries

Desktop, web and mobile are independent clients of the headless Rust system. They share
responsive Lit components, semantic view models, canonical URLs and navigation behavior.
Native shells add platform capabilities, not a second application or substrate. The client
can start and render without a running core or network connection. Rust owns authoritative
behavior and publishes state; the client owns rendering, local interaction and its cached projection.

This document owns delivery and acceptance across these boundaries. It does not replace
[the engine design](../architecture/CONTINUUM-ENGINE.md) or
[the ORM design](../infrastructure/RUST-ORM-ARCHITECTURE.md).
Use the existing [URI desktop](../architecture/URI-ADDRESSED-DESKTOP.md) and
[activity shell](../architecture/ACTIVITY-SHELL-CONVERGENCE.md) contracts; verify old
implementation claims against current source. Recipe is the template; activity is its instance.

## First paint and state

Render the packaged/cached application shell and the last compatible saved view immediately,
without awaiting whoami, directory, model startup, WebSocket connection or remote data.
An installation with no saved state renders a complete usable shell and explicit empty state.
Offline with saved state is a normal operating state, not an error screen or a blank spinner.
Offline browser reload requires cached application assets as well as cached data; prove both.

Persist projections under their actual identity, scope, activity and schema revision. The
cache is a read model of authoritative state, never another authority. Restore focus and
navigation with that projection. Do not expose one account's saved view after an identity change.
Hover and animation state stay local; persistence must not turn each frame into a database write.

Use typed connection/freshness states separately from content: a disconnected banner does
not erase the last content. On reconnect, apply the existing snapshot-then-live protocol with
revision/incarnation checks, gap recovery and subscription cleanup. No duplicate subscription
owners, arbitrary sleeps, polling for readiness, or timers that declare success.
Rust lifecycle readiness is published by its existing owner; rendering completion is a separate
client acknowledgment. Screenshots must be possible offline without waiting forever for live feed.

Offline writes need an explicit contract per action: locally available, pending synchronization,
or unavailable with a reason. A pending action is not a committed success. Do not invent a
general offline write queue before the first activity's operations justify it.

## Routing and responsive views

One declared route model resolves URLs to typed activity identities and recipe-selected views.
Names are presentation; stable identities bind state. URL rewriting, direct deep links, refresh,
back/forward and in-app navigation resolve the same destination. The web server must serve the
application entry for valid client routes without rewriting assets or API requests into HTML.
Native links enter that same resolver. Unknown destinations produce an explicit view.

Desktop, narrow web and mobile render the same activity and action semantics. Test keyboard,
focus restoration, touch, scroll, accessible names and viewport changes. Responsive layout
must preserve selection and pending edits, not create separate mobile truth or routes.

## The resident development cycle

1. Choose a real activity and a bounded user-visible task; record its recipe, route and expected behavior.
2. Read/edit its actual CSS/JS/TypeScript/Lit source through existing code commands in an owned worktree.
3. Have the development asset owner publish a build revision and apply the CSS/module update.
   Preserve the live client and activity state. Compilation failure retains the last valid UI
   with explicit diagnostics; this is development feedback, not a legacy implementation fallback.
4. Observe the rendered revision through both pixels and structured view/DOM/accessibility facts.
   Include console errors, failed requests, layout and interaction evidence where relevant.
5. Click, type, scroll and navigate through typed Continuum commands against that exact surface.
   Wait on the action's result and resulting view/render revision, not a guessed delay.
6. Compare expected and observed behavior, correct failures, obtain independent review, publish,
   and run the same task on the deployed artifact. Keep the useful work and review receipts.

A screenshot path alone is not perception: prove the image reaches the persona's actual input
and that she uses its contents to correct her work. A command dispatch is not interaction success.
Glass-box inspection must show the task, observations delivered, chosen actions and results,
without confusing a developer's private browser or an unrelated page with her target surface.

Use existing command dispatch, Positron observe/session APIs and perception adapters. Audit
available commands through the CLI before extending them. Target-specific required inputs belong
in enum variants and grouped borrowed structs; invalid combinations must not be a bag of options.
An observation/action receipt needs surface identity, route/activity, asset revision, state revision,
action identity and observed outcome. Stale targets and closed surfaces are typed failures.

Source inspected at 390b36e33d: `continuum-positron/src/cache.rs` retains in-memory envelopes;
that alone does not prove cold offline restoration. `commands/interface/capture` has web/iOS/Android
capture adapters and distinguishes capture from connected-client screenshot. Its comments explicitly
defer image delivery into persona perception. These are starting seams, not end-to-end acceptance.

## Ownership and current work

Agreed with Fable on 2026-11-14:

| Owner | Delivery responsibility |
|---|---|
| Fable | Single Rust lifecycle readiness owner; persisted Positron kinds replacing whoami/directory/mind/bundle polling; engine stability and speed |
| Codex | This delivery contract; client/resident edit-observe-interact loop; command/perception gaps; activity/recipe URL and offline acceptance |
| Residents with reviewer assistance | Bounded product tasks, real UI observations, corrections and published work; expand autonomy as demonstrated |

Fable reports the following open work; none is treated as merged or deployed by this document:

- [#6030](https://github.com/CambrianTech/continuum/pull/6030): immediate web mounting/paint, feed status, perception settling and clean-machine user gate.
- [#6031](https://github.com/CambrianTech/continuum/pull/6031): product-for-users plan; reconcile overlapping delivery text rather than maintaining competing plans.
- [#6032](https://github.com/CambrianTech/continuum/pull/6032): packaged desktop assets in releases.
- [#6033](https://github.com/CambrianTech/continuum/pull/6033): draft event readiness; move into lifecycle owner and retire startup/reboot scripts.

## First complete slice and acceptance

Select one existing activity with Fable and a resident after auditing the current routes/tools.
Start with an ordinary responsive content change plus one navigation/interaction task. Do not
create a parallel demo application. Capture baseline before changing the real client.

| Replay | Required evidence |
|---|---|
| Cold client, core stopped, previously saved activity | Shell and saved content paint; correct identity, route and focus; no network dependency |
| First visit with no cache | Complete shell and explicit empty state; assets load without a blank startup interval |
| Disconnect/reconnect during activity use | Saved content retained; revision-correct resync, no duplicate events or false committed actions |
| Deep link, refresh, back/forward and rewritten URL | Same typed activity/view; asset/API paths unaffected; correct focus |
| Resident CSS/JS edit, including compile failure | Applied asset revision or explicit failure; state survives; resident sees actual visual change |
| Click/navigation plus stale or closed target | Correlated result and new observation; typed refusal for stale target; no timer-based success |
| Narrow viewport, keyboard and touch | Same task completes, content remains reachable, focus and edits preserved |
| Existing non-Positron web project and iOS project | Edit/run/observe/interact/correct works through declared project capabilities without converting the app to Positron or Lit |
| Developer disconnects and resident process restarts | Durable task and observation context resumes; resident advances work without a supplied next-action script |
| Restart while saving a projection | One complete compatible snapshot or explicit invalid-cache state; no mixed revisions or cross-identity data |

Every iteration replays measured faults and representative bursts: multiple subscribed views,
resident observations, navigation, hot updates and reconnects while neighboring personas work.
Measure before/after first shell paint, saved-content paint, update-to-paint and action-to-observation
p50/p95/p99; memory, transferred bytes, redundant requests and completed correct tasks. Record
hardware, workload, sample count, source/asset/runtime revisions and cold/warm conditions.
Test windows measure behavior; they never drive readiness or action completion.

Completion includes independent review, exact-head CI, publication/integration, exact running
artifacts and a 15-minute deployed comparison with neighbors progressing. Resident-generated
activity, passing tests and screenshots alone do not establish useful product work or durable learning.
