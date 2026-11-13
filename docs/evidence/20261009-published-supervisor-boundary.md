# Published Windows supervisor boundary proof

Card 3a6d9cc3-8854-4213-b194-c3155d06f056 extends the existing optional
`windows-process.test.ps1 -PrebuiltCli` scenario, already used by the Windows
publisher. It runs the actual packaged CLI from an isolated supervisor layout,
uses the shared receipt writer, and supplies an inert core/engine plus a
PowerShell launcher which only initializes ServicePointManager and exits.
Child-only HOME/USERPROFILE/ProgramFiles/ProgramW6432 values isolate all paths;
existing job ownership bounds execution and removes the owned scratch tree.
No real task, protected installation, active receipt or core is started.

The actual published 2c2ad67c8371a897d84cbab8f31c2a7936c2c513 CLI passes both the
existing JSON preparation scenario and this supervisor launch scenario. Its
protocol is 3; the currently installed protected bootstrap reports 2. The
validated archive SHA256 is
6b8592a29c0f8dbb2560171c34320bbe838b0ef19b67dda6973b887c031ca6cf.
The real retrieval embedding probe passes with 1024 dimensions, 244 ms load and
354 ms embed. Its engine receipt verifies 16 files including the stamp
f45bb191b:cuda:80:portable-v1.

Public PrepareOnly nevertheless refuses safely: registered service-a is protected,
and service-b/livekit-bridge.exe is still mapped. A bridge process listens on9101;
its parent PID has been reused, so parent-number matching is not ownership proof.
The new and old bridge hashes differ. No process was killed or slot edited.
This fixture proves the published launcher boundary, not actual supervised
adoption, media handoff, or Kimi recovery. Resolving media/slot lifecycle ownership
remains necessary before another activation command.

Raw receipts in the team-proof README directory: 20261009-2c2ad-public-prepare.log,
20261009-2c2ad-published-supervisor-test.log, 20261009-2c2ad-embedding-probe.log,
and 20261009-2c2ad-verification.json.
