# PDF native vision acceptance

Owner: Codex. Joel clarified on 2026-10-01 that this lane remains our work;
BIGGIEDESK installation belongs to the other Codex and must not delay it.

## Reproduce

With the existing Continuum core and a ready vision-capable bound model, run:

```powershell
./tools/scripts/tests/pdf-vision-live.ps1 -OutputDirectory <receipt-directory> -Continuum <resolved-continuum-executable>
```

The script creates a vector-only PDF, calls `perception/observe`, verifies the
text layer is empty, and passes the returned PNG to `ai/generate` using the
active model returned by `ai/inference/status`. The question contains no answer.
The expected visual facts are a blue square on the left and red circle on the
right. Source, observation, image, request, response, and hash receipt remain in
the output directory. No model fallback or separate inference server is used.

## Observed result, 2026-10-01

The first live run passed through the installed core build `84c18e611` (#5874)
and `ggml-org/Qwen3.8-27B-GGUF` via the `llama-server` adapter.

- Request: `req-1790901255903`.
- Answer: “On the left is a blue square, and on the right is a red circle.”
- Wall time: 164,962 ms; reported response time: 164,715 ms.
- PDF SHA-256: `8d502094c7348f74f0054bea96f0c45c68354244980477f6db5ff199387e6d7e`.
- PNG SHA-256: `03acaf6e65083234f09dff096b5e4761c7112eba486464c73df68dea9daace7f`.
- Local evidence: `C:/Users/joelt/.continuum/state/team-proof-20260921/pdf-visual-acceptance/`.

This establishes page rendering and visual understanding through normal commands
for one synthetic page with no extractable text. It does not establish arbitrary
PDF accuracy, the full persona workflow, or native audio/image output. The 165 s
latency is unresolved; it has not been attributed to queueing, image encoding,
prefill, or generation. The sequential reusable-script run also passed:
`req-1790901571056`, 109,722 ms, identical source/image hashes and correct answer.
Its complete evidence is in the `repro/` subdirectory. The timing difference is
not a demonstrated optimization; no runtime change occurred between runs.

Next: trace latency and carry native media through
the existing persona activity and model binding without substituting text.
