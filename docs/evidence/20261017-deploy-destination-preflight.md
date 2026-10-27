# Deployment destination admission before artifact transfer

On 2026-10-17, the Windows consumer running `20b528bfd` completed three
extractions of published `c7deff74151ef6b4d2371945843101df13fe8f2a`, totaling
6,670,574,958 logical bytes. Each subsequent installation refused service-slot
capacity and preserved the installed files. These were resource refusals, not
evidence of a corrupt published artifact.

The deploy-only artifact fetch now resolves the existing installed `payload-root`
and checks its `bin` volume with the shared projected-capacity policy before
starting archive preparation. Zero additional bytes is a lower-bound check:
passing it does not approve any copy. The installer still measures and admits the
actual service, engine, and bootstrap writes at their destinations. A corrupt or
unavailable placement record is an operational block; a volume refusal uses the
existing typed resource condition and wait/recheck lifecycle.

Offline `prepare-prebuilt` remains independent so a verified recovery artifact can
be prepared on configured cold storage while the installed service volume is
full. No cache entry is deleted, no capacity threshold is changed, and no live
consumer is upgraded by this source change. The earlier consumer still requires
supported adoption before these semantics apply.

The existing artifact-placement fixture covers a full service volume alongside
an available cold artifact volume, explicit cold payload placement, corrupt
placement, and a successful zero-byte check followed by refusal of an actual
copy. Fresh hosted compilation and the fixture are required before integration;
post-adoption acceptance is a destination refusal before any new archive
directory/download on the deployment owner's next naturally selected candidate.
