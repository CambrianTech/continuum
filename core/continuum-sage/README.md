# continuum-sage

SAGE, the engine every worker stands on: `Component` (worker, subscriptions by topic on a `Bus`,
provenance, timers on the one `Clock`, presence and rest, need, novelty, switchable observation), the
`State`/`Stream` topics, and the flow dispatcher.

Read `docs/architecture/SAGE.md` before adding any task, region, tick, timer or worker: every one is a
`Component`, and that page says how to add one, what moves onto SAGE next, and what it measured.
