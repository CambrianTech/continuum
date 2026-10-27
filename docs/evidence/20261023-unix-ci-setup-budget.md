# Bound dependency setup separately from Rust validation

This supports the build-loop work in AGENTS.md line 1 and the capacity-custody
repair in PR5309: validation must finish before the change can reach residents.

Exact integrated head 1c0db901211bde00f43d5144bdd91ea36aa59a1c reached the
45-minute Unix job limit twice in run39965839086. On attempt2, job120478649863
ran from14:46:45Z to15:32:09Z. Apt reported downloading52.9MB in15m4s
(58.6kB/s); compilation started around15:07Z. The retained log is
`C:/Users/joelt/.continuum/state/team-proof-20260921/5309-integrated-unix-attempt2.log`.

Before cancellation, core1230, engine1164, hands197, live353 and mind3026 tests
passed. The exact native custody interop test also passed. Cancellation occurred
during the later CLI build at15:32:04Z. This does not establish completion of
the CLI, doctest, Postgres or binding-drift gates.

The shared Unix job now has a70-minute outer bound: its existing45-minute
cold build/test allowance plus25minutes for dependency setup. The apt step has
its own25-minute bound, so an unresponsive mirror fails during setup rather than
consuming an unbounded portion of validation. This is a budget repair, not a
claim that downloads are faster. Runner selection, dependency sources/packages,
retry policy, Cargo cache, features, all test commands and required result
assertions are unchanged. No test was skipped, split out or relaxed.

Local validation is source/diff inspection only; no compiler was started beside
serving. Fresh exact-head CI remains required. A third identical rerun was not
used to conceal the repeated timeout.
