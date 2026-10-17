# Explicit prebuilt launch entered source preparation

Card: 337b2c67-3f5d-4bd3-8eb3-68781a7b2737.

## Observed

On 2026-10-17 the trusted 66e98578b CLI validated an exact rebuilt
825ff5f6f core before shutdown. All 66 modules returned durable state and
12 persona checkpoints were copied with matching SHA-256 hashes.
The subsequent `reboot --prebuilt` inherited `CONTINUUM_START_SCRIPT`.
The launch banner promised no rebuild, but `start-server.sh` ran the native
installer and then Cargo for companion binaries. The native GPU smoke check
refused to stamp its output; the script continued with its existing-binary
fallback. The owned startup tree was canceled before it launched the core.

The same verified core then launched successfully through the existing
installed-only direct prebuilt path, with the inherited script override
removed from that invocation and a non-repository working directory.
It served the original model and adapter paths. Native process identity was
not retained; the old planner selected one lane rather than two. This is
launch-path evidence, not a perfectly matched cognition comparison.

Local receipts are retained under the existing team-proof handoff directory:
`20261017-old-core-drain.json`, `20261017-old-core-checkpoint-manifest.json`,
`20261017-old-core-reboot.log`, `20261017-old-core-direct-reboot.log`, and
`20261017-old-core-live-test.json`.

## Source boundary

At canary 80cb1bc7c, `plan_launch` correctly selects `Prebuilt`, but the
production arm then chooses Bash whenever a start script is discoverable.
The existing `prebuilt_reboot_launch_retains_candidate_socket_and_cwd`
fixture tests `direct_core_command` separately; it does not exercise that
production choice. The script explicitly skips only the core binary build,
not native, companions, dependency installation, or desktop builds.

Developer warm builds also become `LaunchSource::Prebuilt` after preparation.
An explicit supplied artifact and a prepared developer build therefore need
their original caller intent retained; changing every `Prebuilt` arm would
silently change deliberate source deployments.

## Correction requirements

- Explicit prebuilt launch must not execute source builders, including via an
  inherited script override. Resolve and validate runtime prerequisites before
  draining the current core.
- Preserve existing installed companion ownership. On Windows the validated
  release launcher owns media and the service host owns browser startup;
  applying config and runtime DLL paths alone does not replace those owners.
- Preserve intentional source preparation and normal installed-service
  registration, engine selection, and receipt verification.
- Extend the existing command-construction fixture to exercise the actual
  production command selection with a discoverable/inherited source script.
  Run it in the existing CLI test lane, without a new compiler job.
- No live script modification or deployment during the old-core experiment.

## Source correction under review

Windows explicit `--prebuilt` now retains distinct caller intent from a warm
source build. Before draining, it validates and pins the installed release
closure and obtains the existing startup permit. The shared installed command
constructor invokes that release's media launcher and service host with only
the validated core path substituted. It does not register a task, copy a slot,
promote an engine, or inspect an inherited source script. The runtime pins and
startup permit remain held through readiness verification.

A genuinely uninstalled node retains the existing direct artifact path.
An existing or unreadable active receipt without its task is refused before
drain rather than treated as an uninstalled node. Legacy unsealed task
descriptors likewise refuse before drain. Normal `--service`, developer warm
builds, and macOS launchd behavior remain unchanged. Unsupervised non-Windows
script preparation is outside this Windows correction.

The existing command fixture now exercises the actual production constructor,
including media launcher, core, CLI, socket, engine, and browser arguments.
Its exact selector was added to the existing Unix CLI fixture loop; no new job
or duplicate compilation lane was introduced. Rust parsing and `git diff
--check` pass. Compiled validation and peer review are pending; no live
deployment or runtime recovery is claimed for this source change.
