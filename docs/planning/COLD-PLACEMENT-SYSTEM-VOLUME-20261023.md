# Cold placement excludes the system volume (aa7c5bdd)

The previous max-free selector could label the OS drive Cold. The kernel drive owner now resolves the OS path through drive_holding and excludes its mount. Unknown OS identity offers no cold tier. The pressure-broker model pool migrates its duplicate starts_with resolver to drive_holding, retaining canonical Windows prefix handling.

A read-only Intel Mac observation on 2026-10-23 showed root disk1s5s1 and Data disk1s1 sharing APFS container disk1 and physical store disk0s2. sysinfo 0.33.1 explicitly includes browsable Data mounts, but its public Disk API exposes no container identity. APFS and unknown filesystems therefore do not qualify automatically in this slice. This also withholds external APFS offload until a container-aware identity adapter is available. Mount separation does not prove physical-device separation on other filesystems either; this is not a complete measured-tier governor.

Existing kernel tests cover system-most-free, secondary threshold reversal, absent/unknown OS mount, a single volume, canonical Windows spelling, and unresolved APFS capacity. Both drive tests are selected in the existing Windows fixture loop with an exact executed-test assertion; Unix runs the kernel suite. No local build ran beside serving. CI, independent review and installed consumer evidence remain required.

Artifact-root relocation, active K3 placement, storage latency measurement, and autonomous tier migration remain separate work. No runtime files or serving state changed for this patch.
