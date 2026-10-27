# Deploy launch obeys maintenance custody

Card ff7da522-b9d0-4700-8e4b-d355a692ff24 advances stabilization and the supported development maintenance path for resident minds.

During the published b97 maintenance attempt, old core 5a765f7a1 spawned detached deploy consumer PID 32644 while Scheduler custody had disabled deployment. The raw local receipt is `C:/Users/joelt/.continuum/state/team-proof-20260921/cde631d4-live-boundary.log`. Core/engine were subsequently restored; the observation interval never began and disk recovery is not claimed.

The existing StartupPermit now owns deploy admission too: the real Windows actuator holds the existing shared launch gate through Scheduler or detached creation, and the consumer independently reacquires and holds the same permit before ledger/work begins. Any retained maintenance journal refuses deployment, including restoration. A consumer already working prevents exclusive maintenance admission; its existing lifecycle owner must quiesce it first. No second lock, timer, or scheduler is introduced.

The existing lifecycle fixture verifies exclusion, journal refusal, and normal admission after evidence removal. A real-spawner fixture verifies refusal occurs before Scheduler, child creation, or log opening; Windows CI requires exactly one executed result. Local native execution is deferred to CI because this machine serves residents. Publication and installed acceptance remain required. Older already-running binaries do not obey the new gate and still need coordinated quiescence before adoption.

## Consumer handoff and Windows execution selection

Source trace: deploy_consume calls reboot directly (deploy_consume.rs); reboot takes install.lock, prepares/stages its service and takes only a shared StartupPermit for a bare prebuilt launch. The registered task host also takes the shared StartupPermit in core_service.rs. Exclusive launch_gate acquisition belongs to SchedulerCustody disable/recovery and legacy container maintenance; none is called by the consumer/reboot handoff. The existing fixture now keeps the deploy permit while acquiring the same handoff startup permit, proving compatible shared acquisition. Independent maintenance remains excluded until consumer exit.

Both Windows contracts are opt-in for canary PRs via the existing `windows` label. Initial unlabelled checks skipped them; this PR is now labelled and a new synchronized head selects actual Windows core and lifecycle execution. A passing wrapper with skipped legs is not validation.
