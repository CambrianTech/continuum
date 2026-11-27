# Flash-Next vs the Hard Ones (staged boundary run)

**The question (Joel, 2026-08-28):** can Flash-Next solve instances Ornith missed?
Easy probes cannot separate the two models — running the candidate on the
incumbent's MISSES is the sharpest, cheapest discriminator. Strategic frame:
best AI on a normal MacBook Pro — if this works, the portfolio is Ornith for
team speed + Flash-Next for hard problems, both on 64GB consumer hardware.

## Ornith's 20 misses (newest verdict per instance, none env-absent)

astropy-13453 · django-10914 · django-11734 · django-13809 · matplotlib-20859 ·
flask-4045 · requests-2148 · pylint-5859 · pylint-6506 · pylint-7114 ·
pylint-7993 · pytest-5103 · sphinx-10325 · sphinx-11445 · sympy-13647 ·
sympy-16988 · sympy-18057 · sympy-21379 · sympy-22005 · sympy-22840

## The hard eight (first night — repo-diverse)

django-10914, django-13809, pytest-5103, flask-4045, pylint-7114,
sphinx-10325, sympy-21379, sympy-22005

## Boundary sequence (after current rounds drain)

1. `continuum reboot` — deploys the #2550 catalog row (Flash-Next + its serving
   contract: table pin, fit off, no warmup, ubatch 512).
2. Pin Flash-Next as the serving model (`serving/pin`, id
   `AtomicChat/Qwen3.8-Flash-Next-GGUF`). The lane readiness smoke probe is the
   REQUIRED first-request warmup (measured: a chat-sized first request Metal-OOMs
   and LATCHES the backend; one tiny raw completion first fixes it — the smoke
   probe is exactly that, and readiness gates citizen turns behind it).
3. Dispatch: `benchmark/dispatch` with the hard-eight `instances` list
   (they span lite+verified — dispatch per dataset).
4. Budget: ~1-1.5 h/solve at Flash-Next speed → one overnight for eight.

## Honest caveats to carry into any verdict

- **Window asymmetry:** Ornith's misses ran at served_window 132k; Flash-Next
  serves 32k (fragile) / 16k (safe). The recipe budget adapts, but the candidate
  is handicapped on grounding. Fair-as-product (this IS what the model offers a
  MacBook), unfair-as-model-science — say so either way.
- Success = any Ornith-miss RESOLVED (graded by the same harness, provenance
  stamped). 0/8 does NOT prove the model is dumber — window + speed confounds —
  it proves the chart-to-our-harness gap is real enough to stop spending box
  time until the expert-pager changes the economics.
- If OOM latches mid-round at 32k: relaunch at 16k, note it, continue. Fresh
  boot memory is the best case (the 32k margin is ambient-cache-sensitive).
