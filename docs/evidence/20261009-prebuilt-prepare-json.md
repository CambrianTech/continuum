# Public prebuilt preparation JSON boundary

Card 9e5e10b9-5f28-4083-8812-71e9fd6e1095 follows the actual pinned5a177 public
PrepareOnly failure: the archive verified and extracted, then PowerShell rejected
`deploy-consume` as an invalid JSON primitive. The shared preparation owner had
printed its success diagnostic before the protocol caller's JSON result.

The existing preparation function now accepts its caller's diagnostic sink.
Interactive deployment retains stdout and the deployment log; prepare-prebuilt
uses stderr and the same log. Stdout contains only its JSON result. No validation,
receipt, source-build fallback or build-input identity rule changes.

The existing Windows process fixture has an explicit candidate-CLI scenario:
it creates a tiny local Git checkout/archive, invokes the actual CLI preparation
path in an owned process tree, parses all stdout as JSON, and verifies progress
on stderr. A second request corrupts the checksum and requires failure without
success output. Child HOME/USERPROFILE isolate cache/pruning from user state.
The Windows publisher exercises this scenario after staging runtime DLLs, before
publishing its artifact. Other process scenarios are not redundantly rerun there.

The new real-CLI scenario reproduces the exact failure on published5a177 in under
two seconds. Existing process fixture:10 groups passed (7.2 seconds). Fixed-core
CLI type checking and new-candidate execution are still pending; the publisher's
actual scenario must pass before corrected release/adoption is claimed.

Existing artifact policy includes core/ and tools/scripts/lib/, so this repair
requires a matching publication. It does not relabel5a177 or privately filter its
output. No UAC, active receipt, installed runtime or persona state changed.
