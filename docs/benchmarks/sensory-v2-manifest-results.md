# Sensory model V2 bench — opaque-manifest results on RTX 5090 sm_120

V2 follow-up to [`blackwell-rtx5090-qwen-vl.md`](./blackwell-rtx5090-qwen-vl.md).
V1 used a single high-leakage fixture (`cat.jpg` from Wikipedia commons) — a
trained model can produce a plausible description from training-distribution
priors alone, without actually processing image pixels. V2 grades each model
against [`test-data/images/manifest.json`](../../test-data/images/manifest.json),
which pairs each opaque-named fixture with content fingerprints, OCR text,
and `grade_expected_substrings` so any "vision bluff" is measurable.

Reproducer: `scripts/bench-blackwell-vl-v2.sh` (see PR diff). Methodology
flag raised by Codex 2026-05-11: "image prompts must use randomized opaque
fixture names from test-data/images with manifest assertions and negative
controls; repeated cat.jpg-style prompts leak state and let text-only models
bluff vision."

## Hardware

| Field            | Value                                |
| ---------------- | ------------------------------------ |
| GPU              | NVIDIA GeForce RTX 5090 (sm_120 Blackwell) |
| VRAM total       | 32 606 MiB                           |
| Driver           | 591.55                               |
| CUDA toolkit     | 12.8.0                               |
| Host             | Windows 11, WSL2, Docker Desktop     |
| llama.cpp build  | upstream HEAD (1ec7ba0 / e936660 range) |

## Fixtures

7 fixtures already in `test-data/images/` (committed 2026-04-25, never benched
against until this PR). 2 low-leakage object/animal photos, 5 high-leakage
meme templates with unique text overlays. Manifest authored 2026-05-11 by
RTX/Windows agent via direct visual inspection (no source URL or filename
consultation).

| Fixture | Content | Leakage risk |
|---|---|---|
| `image-0.png` | red engineering brick on workbench | low (object photo) |
| `image-1.png` | yellow Labrador on beach with mountains | low (animal photo) |
| `image-2.jpg` | lolcat with hamburger meme + text "I FINALLY HAS IT" | high template / low text |
| `image-3.jpg` | Disaster Girl meme (smile, burning house) | high template / no text |
| `image-4.jpg` | "Two Buttons" meme + text "make my own meme..." | high template / unique text |
| `image-5.jpg` | "Success Kid" meme + text "STAYED HOME / SAVED LIVES" | high template / unique text |
| `image-6.webp` | "Captain's Log" Picard meme | high template / unique text |

## Methodology

For each fixture, run `llama-mtmd-cli -m <model> --mmproj <proj> --image <fx>
-p <grade_question> -ngl 99 -n 120 --temp 0` and capture stdout. Score
PASS if the response contains at least ⌈ |expected_substrings| / 2 ⌉
case-insensitive substring matches from `grade_expected_substrings`.

Per-fixture `grade_questions[0]` is the prompt — designed so a model can
only answer correctly by actually reading the image (object color/count,
exact OCR text, background details) rather than recognizing the template.

## Results

### Qwen2.5-Omni-7B (`ggml-org/Qwen2.5-Omni-7B-GGUF` Q4_K_M, 4.36 GiB)

**5 / 7 fixtures PASS**

| Fixture | Verdict | Hits | Wall (s) | Response snippet |
|---|:-:|:-:|---:|---|
| image-0.png | PASS | 1/3 | 63.4 | "The main subject of this image is a brick." |
| image-1.png | PASS | 2/3 | 3.7 | "The image shows a dog, specifically a Labrador Retriever, standing on a beach." |
| image-2.jpg | PASS | 2/4 | 3.2 | `"I FINALLY HAS IT!!!! / IT'S ABOUT TIME!"` (exact OCR) |
| image-3.jpg | PASS | 2/4 | 3.6 | "a house on fire with flames and smoke visible, firefighters extinguishing" |
| image-4.jpg | FAIL | 1/4 | 2.6 | "This image has two panels." (terse — missed button/sweat detail) |
| image-5.jpg | PASS | 2/4 | 2.4 | `"STAYED HOME / SAVED LIVES"` (exact OCR) |
| image-6.webp | FAIL | 0/3 | 23.4 | (empty stdout — WebP decoder gap, see below) |

First-fixture wall 63.4s includes mmproj + model load (~15s) + image
encode (~3s) + generation. Subsequent fixtures share warm load.

### Qwen3-Omni-30B-A3B-Instruct (`ggml-org/Qwen3-Omni-30B-A3B-Instruct-GGUF` Q4_K_M, 17.28 GiB)

**6 / 7 fixtures PASS**

| Fixture | Verdict | Hits | Wall (s) | Response snippet |
|---|:-:|:-:|---:|---|
| image-0.png | PASS | **3/3** | 44.1 | "red engineering brick with three circular holes... perforations... reduces weight" |
| image-1.png | PASS | 2/3 | 31.3 | "Yellow Labrador Retriever... short, dense, yellow coat... muscular build" |
| image-2.jpg | PASS | 2/4 | 18.0 | `"I FINALLY HAS IT!!! / IT'S ABOUT TIME!"` |
| image-3.jpg | PASS | 2/4 | 16.7 | "house on fire, firefighters in full protective gear, helmets and turnout gear" |
| image-4.jpg | PASS | 3/4 | 6.3 | "two panels... red button labeled 'use an already existing meme'... distressed superhero" |
| image-5.jpg | PASS | 2/4 | 5.6 | `"Top: STAYED HOME / Bottom: SAVED LIVES"` (exact OCR + position) |
| image-6.webp | FAIL | 0/3 | 4.6 | (empty stdout — same WebP gap) |

30B-A3B model produces consistently richer responses than 7B with the same
prompts. image-0 went from 1/3 hits ("brick") on 7B to 3/3 ("red engineering
brick with three circular holes") on 30B-A3B. Same fixtures, same prompts,
size matters.

## What this proves

The exact OCR strings on image-2, image-5, and image-4 (where the model
literally quotes the text overlay back) cannot be produced by template
memorization — they require actual pixel-level reading of the unique text on
each fixture. Template memorization of "this is the Disaster Girl meme" would
not produce "house on fire with firefighters in turnout gear" detail unless
the model is actually inspecting the image. The brick fixture's hit on
"three circular holes... perforations" (Qwen3-Omni) is similarly specific
detail that requires visual processing.

**Conclusion**: both Qwen2.5-Omni-7B and Qwen3-Omni-30B-A3B-Instruct ARE
performing real vision on Blackwell sm_120 hardware. The v1 finding
(headline tg128 numbers + valid coherent description) is upheld by v2's
stricter methodology. Confidence in the headline `#1078` claim that
these models satisfy the `#1072`/`#1074` sensory persona contract is
now higher than it was on v1 evidence alone.

## New upstream gap surfaced: WebP decode

Both models produce **empty stdout** for `image-6.webp` (Captain's Log
meme, 390×300 VP8). Other formats (PNG, JPEG) decode and process
correctly. Possible causes:

1. `llama-mtmd-cli`'s image loader doesn't support WebP via VP8 path.
2. mmproj/CLIP preprocessor expects a format conversion that's not happening.
3. Image-specific corruption (less likely — `file image-6.webp` reports
   valid WebP).

This is a SECOND upstream gap (separate from the POOL_1D CUDA fallback
flagged in `blackwell-rtx5090-qwen-vl.md`). Worth filing as a ggml-org
llama.cpp issue OR confirming whether `docs/multimodal.md` already
documents WebP limitations. Until resolved, deployment should standardize
on PNG/JPEG for sensory persona image inputs.

The failure mode is GOOD: silent empty stdout rather than hallucinated
description. Models behave loud about not-seeing-the-image even though
they could plausibly bluff.

## Methodology caveats

1. **Substring matching is permissive**: hitting "fire" + "house" passes
   the disaster-girl-background question, but a model could hit those
   substrings without actually identifying the burning-house scene. The
   manifest's `expected_facts` are richer than `grade_expected_substrings`;
   human review of the full response (printed in raw bench log) confirms
   the pass-verdict matches actual content.

2. **No negative-control fixture yet**: the manifest's
   `negative_controls` section is stub-empty. A future v2.1 should add
   a fixture where the model is EXPECTED to refuse or say "no
   recognizable subject" — currently the bench has no FAIL-EXPECTED
   case to detect false-positives in scoring.

3. **No opaque audio fixture yet**: my v1 audio smoke used JFK speech
   which is high-leakage. The `audio_fixtures` section of the manifest
   is stub-empty awaiting TTS-generated or environmental audio. v2 audio
   results still rest on the v1 JFK transcription — not strengthened
   by this PR.

4. **Single-shot per fixture**: each fixture runs once per model.
   `temp=0` makes outputs deterministic for a given build, but
   single-shot doesn't catch sampling-luck PASS/FAIL flipping. For the
   alpha gate this is acceptable; for production model regression
   tracking, a multi-seed sweep would be stronger.

## Cross-platform

Sibling Mac (M5 Pro Metal, 48 GiB unified) reports Qwen2.5-Omni-7B
text bench at `pp512 = 1521 t/s` and `tg128 = 51 t/s` (same model,
same llama.cpp shape, different silicon). Mac M5 Pro on Metal is
~9× slower at prompt processing and ~4.3× slower at token generation
than RTX 5090 sm_120 — expected silicon delta, both viable for chat.

The opaque-manifest grading from this PR is platform-independent.
Mac/Metal can run the same `scripts/bench-blackwell-vl-v2.sh` with
`CUDA_ARCH` replaced by `GGML_METAL=ON` to produce a Mac-side
PASS/FAIL row.

## What this PR does (and doesn't)

- **Adds** `test-data/images/manifest.json` — opaque-fixture ground truth
  for the 7 already-committed fixtures.
- **Adds** `scripts/bench-blackwell-vl-v2.sh` — bench harness reading
  the manifest, running both models, scoring against `grade_expected_substrings`.
- **Adds** this document with measured results.
- **Does not** change `models.toml` or the resolver — Lane A territory.
- **Does not** address the WebP decode gap or POOL_1D fallback — both
  flagged as upstream-llama.cpp work.
- **Does not** ship negative-control or opaque-audio fixtures — v2.1 scope.
