---
name: continuum:update
description: Update a Continuum installation to latest. Default is Carl-path (pull prebuilt images from ghcr, ~30s). Pass --dev to rebuild from source.
user-invocable: true
allowed-tools: Bash
argument-hint: "[--dev]"
---

# Update Continuum

Do it yourself — don't ask the user to run commands they'd run manually anyway. The only reason to fall back to "tell the user to type this" is if `continuum` isn't on PATH.

## 1. Verify continuum is installed

```bash
command -v continuum >/dev/null 2>&1 || { echo "continuum CLI not on PATH. Install: curl -fsSL continuum.homes/install | bash"; exit 1; }
```

## 2. Run the update

The CLI already handles the Carl vs Dev split — you don't need to pre-decide. Just pass through the user's args (or none).

**Default (Carl path — pull prebuilt images from ghcr, ~30s):**
```bash
continuum update
```

**Dev path (rebuild from source — slower, needed when touching Rust/TS source):**
```bash
continuum update --dev
```

## 3. Report the outcome

When the update completes (or fails), summarize in user-facing language:

**On success:**
> "Continuum updated. Latest images pulled, services restarted. Run `continuum status` to verify, or `continuum doctor` if anything looks off."

**On failure (Carl path):**
> "Image pull failed. If you're on a dev machine and want to rebuild from source instead, run `continuum update --dev`. Otherwise paste the error above and I'll diagnose."

**On failure (Dev path):**
> "Build failed. Read the compiler output above — most common causes: out-of-disk, submodule not initialized (run `git submodule update --init --recursive`), missing system dep (libvulkan / nvcc / cmake)."

## 4. When to suggest --dev vs default

The user usually wants the default (fast pull). Only suggest `--dev` when:

- They just `git pull`'d source changes and want them live (the `continuum update` default does `git pull` too but only pulls prebuilt images, so source changes require rebuild).
- `continuum update` failed with an image-pull error AND the user said they're actively developing.

Don't suggest `--dev` to a Carl-level user. It's a 30+ minute rebuild they don't need and will think is broken.

## 5. When to NOT run update

- If the user is in the middle of a live chat session with personas, tell them the update requires a service restart and ask if they want to wait.
- If `continuum status` shows the system isn't currently running, just run the update (no live sessions to protect) and tell them to `continuum start` after.

## 6. Related skills

- `/continuum:doctor` — diagnose issues post-update
- `/continuum:status` — see what's running, which images, GPU backend
- `/airc:connect` — pair into a mesh if you need help from a peer AI during the update

## Notes

- The `continuum` CLI source lives in the repo at `bin/continuum`. `continuum update` is the same binary the user runs manually; this skill is just the AI-invocable wrapper that lets Claude Code drive the update without context-switching to a terminal.
- The CLI itself handles `--help`: `continuum update --help` prints the Carl-vs-Dev distinction.
