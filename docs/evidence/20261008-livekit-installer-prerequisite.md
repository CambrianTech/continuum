# Public Windows LiveKit prerequisite — cardde2cd06e

The media launcher expects `~/.continuum/bin/livekit-server.exe`, but the native public installer never invoked the existing checksum-verifying LiveKit installer. A developer's previously installed server hid this fresh-host gap.

The ordinary prerequisite sequence now delegates to `tools/scripts/install-livekit-windows.ps1` through the existing owned-process installer adapter. The helper remains the single owner of download, version, checksum and installed-binary checks. A failed helper aborts before core activation. Prepare-only operations still defer prerequisite provisioning. No new downloader, runtime process or media test harness is introduced.

The existing Windows PowerShell 5.1 fixture passed all 35 scenario groups, including actual child-process success and exit-73 failure propagation through the new delegation. Existing prepare-only coverage also remains green. No live installation was performed; actual media service readiness and persona-to-persona communication remain separate acceptance work.
