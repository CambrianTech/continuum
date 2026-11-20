# 10. Coverage

[The object model](README.md) · previous: [Modules](09-modules.md) · next: [The plan](11-plan.md)

Every trait in the core on canary c819052ef7 (224, tests excluded), and where it lands. Grouped by
destination, so each root and lineage shows everything that becomes it. A trait is covered when its
implementors are on their destination and the trait is deleted or kept as named here.

## 10.1 Traits with two or more implementors, or a supertrait

| Destination | Today's traits (crate) |
|---|---|
| `Part` and its subsystems (3.1) | `Lifecycle`, `Suspension` (persona-engine); `SettleBeforeStop` (kernel: its three settlers answer `Shutdown` with `Pending`); `Stage` (flow, kernel); `DependentStage` (kernel) |
| `Command` and `ActionCommand` (3.2) | `CommandSpec`, `ProjectsOutcome`, `ActionCommand`, `CommandHandler`, `DynCommand` (kernel, kept as the action-command system); `CommandDispatch` (core, positron); `CommandHandler` (client FFI); `ServeHandler` (client); `GridDispatch` (core) |
| `Policy` (3.3) | `AllocationPolicy`, `WindowPolicy`, `GridPlacementPolicy`, `TierDownPolicy`, `QualityModel`, `BatchScheduler`, `DispatchTermsPolicy` (engine); `FocusPolicy` (engram); `WorkingSetPolicy` (tool-working-set); `Arbiter` (cognition); `AuthPolicy`, `CommandInterceptor`, `DispatchHook` (kernel); `PersonaAdapterFactory` (mind) |
| `Check` (3.4) | `SourceRule`, `CrateRule`, `Shape` (source-hygiene); `SyntaxValidator` (hands); `PerceptionFact` (cognition); `RedactionDetector` (engram); `SalienceDetector` (academy) |
| `Sink` (3.5) | `CaptureSink` (base, renamed); `TurnCredit`, `WorkspaceCaptureSink` (cognition); `LaneCaptureSink`, `PlacementCaptureSink`, `GridCaptureSink`, `LineWatch` (engine); `RagCaptureSink` (mind); `FramePublisher` (live); `DispatchScope` (kernel) |
| `Source` (3.6) | `Source`, `Block`, `Estimate` (context, kept); `RagSource`, `CriticalSource` (grounding); `RagRenderable`, `NodeScopedView`, `RoomScopedView` (mind); `PersonaReflectionSource` (academy) |
| `Demand` (3.8) | `LeaseRequest` (kernel resources, engine capacity), the window allocator's needs, `Admission` (engine), `AdmissionGate`s, permits' requests, the serving planner's demands; answers become leases (`ThroughputLease` kept as the cleared grant) |
| `Durable` (3.7) | `SavedState`, `Rebuildable`, `Key` (saved-state, kept and generalized); `OrmEntity`, `StorageAdapter`, `EpochWatermarkStore` (kernel: the ORM medium); `AdmissionPersistenceSink` (cognition); `LineImporter` (mind) |
| Capabilities and holdings (4.1) | `SpeechToText`, `TextToSpeech`, `VoiceActivityDetection` (live); `EmbeddingProvider` (engine); `AIProviderAdapter` (engine); `Screenshotter`, `WebSearchProvider`, `Publisher`, `ArtifactSource` (core); `PullRequestHost` (hands) |
| `Region`, `FusionRegion`, `Faculty`, `Monitor` (4.2) | `BrainRegion`, `Daemon`, `GpuMonitor`, `GpuDeviceProbe`, `CapacitySource` (kernel); `Faculty` (cognition) |
| `Holder` (4.3) | `ResourcePool`, `ResourceConsumer`, `MemoryReporter` (kernel); `StagingResidency` (academy); `Resident` (cognition: her working memory's items become context blocks) |
| `Transport` (4.4) | `Transport` (client); `Transport`, `AddressedFrame`, `AddressedInbox`, `AddressedPort` (kernel routing); `GridTransport` (grid); `AircDaemonClient`, `AircEventTransport` (core); `AircInferenceTransport` (engine); `Wire` (gateway) |
| `Renderer` (4.5) | `RenderBackend`, `AvatarRenderer`, `SceneObject` (live) |
| `Room` and its views (4.6) | `AircCitizen`, `AircRosterReader`, `AircDoctrineReader`, `AircWorkReader`, `WallReader`, `RoomBoardReader`, `WorkspaceLayoutReader`, `PersonaConversation`, `CitizenConversation` (mind, cognition); `AircTranscriptReader` (cognition); `RoomPurposeSource` (experience); `PeerNames`, `MindOf` (rooms); `QueueItemBehavior` (cognition: her inputs become typed worker inputs); `SuiteAdapter` (academy, into `ActivityAdapter`) |
| `Search` (4.7) | `RecallLayer` (memory); `CandidateSource` (genome); `SearchAlgorithm` (perception) |
| Hands (6.2) | `ToolExecutor` (cognition): her hands' command path; its scripted and recording implementors are fixtures |
| The serving tier (kept) | `Tier`, `Execute` (persona-engine): the Core's residency tier, already a two-generation lineage |
| Client views (kept) | `StateSource`, `SessionSubstrate` (positron): the client-facing projection of `Published`, renamed `ViewSource` |
| Outside the part model (kept where they are) | `IpcStream` (core, an OS stream: moves to host); `PublishedCoreSource`, `DeploySpawner` (CLI deploy, to `apps/cli`); `ToolCallFormat` (protocol parser); `SlotEngine` (serving-replay); `RefusalCause` (errors are values, not parts); `OwnRate`, `AsArg` (engine: deleted with the llama-server lane launcher) |

## 10.2 Traits with one implementor or none

A trait with one implementor is one of two things:
- **a seam for a fixture or a second backend:** it stays at its owner, named for its capability, and
  descends from the root its verb belongs to;
- **an indirection with nothing behind it:** it is deleted and its one type is used directly.

A trait with no implementor is deleted, or implemented by the change that needs it.

| Crate | Traits | Destination |
|---|---|---|
| kernel | `ConcurrencyPolicy`, `LeaseArbiter`, `HostMemoryReader`, `FootprintSource`, `DiskReporter`, `FdRelief`, `ByteSized`, `ReadyBuffer`, `Inputs`, `Stage`, `EventSpec`, `ClogSink`, `ActingProvider`, `AddressedHandler`, `PresentedGrantStore`, `ProvidedCommandProvider`, `MeshPresence`, `StorageProvider`, `SubstrateGovernor`, `VectorSearchAdapter` (0), `DependentStage` (0) | resources onto `Holder` and `Monitor`; `ReadyBuffer` onto `Published`; `Stage`, `Inputs` and `DependentStage` onto the worker; `EventSpec` onto topics; `ClogSink` onto `Sink`; `PresentedGrantStore` onto `Durable`; `VectorSearchAdapter` deleted |
| engine | `Admission`, `ExpertFetcher`, `ExpertPager`, `TierPolicy`, `PagerCaptureSink`, `DeviceTenant`, `EngineExitWatcher`, `GenomeCatalog`, `ContainerMigrationCustody`, `LlamaServerControl` (42 methods), `ProducerCustody` (0) | `TierPolicy` onto `Policy`; `PagerCaptureSink` onto `Sink`; `DeviceTenant` onto `Holder`; the two custody traits are one `Completion`; `LlamaServerControl` and `EngineExitWatcher` are deleted with the llama-server lane path; the rest stay in serving at their owners |
| core | `AircQueueClient`, `AircCommandRunner`, `AircRealtimeStore`, `DaemonSocketResolver`, `PeerTierSource`, `WorkBoardReader`, `NavReader`, `RoomRecipeReader`, `ProgressSink`, `ScalingPolicy`, `GridDispatch` (0) | airc ones onto `Transport`; readers onto `Room` views; `ProgressSink` onto `Sink`; `ScalingPolicy` onto `Policy`; `GridDispatch` deleted |
| cognition | `EffectMark`, `SeenContentLookup`, `SeenEventLookup`, `IsMemorable`, `AdmissionPersistenceLoader`, `PersonaChannelReader`, `CreditSink`, `ReadReceipt` | lookups onto `Published` queries; `IsMemorable` onto `Check`; `CreditSink` onto `Sink`; the loader onto `Durable`; the reader onto `Room` views |
| mind | `PersonaResolver`, `PersonaChannelView`, `DurableRoomHistory`, `PersonaIdentityProvider`, `ScriptedCitizen`, `BoardFetch`, `ChatRoundtripProbe`, `BenchmarkSubmissionAdapter` | resolvers onto her identity branch; history onto `Durable`; views and fetches onto `Room`; the probe onto `Monitor`; the benchmark adapter onto `ActivityAdapter` |
| academy | `ActivityAdapter` (0), `BenchmarkAdapter`, `ReviewBoundaryLedger`, `TeacherServing` | the two adapters are one `ActivityAdapter`; the ledger onto `Durable`; teacher serving onto `InferenceAdapter` |
| experience | `ActivityStateStore`, `StepStateSink`, `LedgerStore`, `ExperienceSource`, `RunLedger` | onto `Durable` |
| genome | `ExamHost`, `RecallTraceStore`, `DemandAlignedRecall` | the store onto `Durable`; recall onto `Search` |
| live | `RenderSurface`, `VoiceReclaimLever`, `PhysicsBackend`, `VideoSource`, `FrameDescriber` | the first two onto `Holder`; the rest stay in render and perception at their owners |
| persona-engine | `Identified`, `Completion`, `Fence`, `NextPolicy` (0) | `Completion` onto the root `Completion`; `NextPolicy` onto `Policy`; the others stay in serving |
| others | client `ServeHandler`; FFI `EventCallback` (0); CLI `DeploySource`, `HeldCapability`; capture `PromptCaptureSink`; engram `RecallRanker`; grid `GridConnection`, `PeerTrustSource` (0); grounding `CriticalSource`; hands `CardBoards`; positron `CommandDispatch` (0); saved-state `Medium`; serving `PromptCacheControl`; llama `ExpertObserver`, `ReplyAdmission`, `OptimizerLease`; serving-replay `ServingTarget`; tool-working-set `Relevance` | each onto the root its verb names (`Sink`, `Policy`, `Search`, `Room`, `Durable`); the llama crate's three stay in the leaf crate where the native boundary needs them |

## 10.3 What remains as roots afterwards

`Part` with its subsystems, `Command`, `Policy`, `Check`, `Sink`, `Source`, `Durable`; the lineages of
chapter 4; and a small number of narrow seams at their owners, each named for its capability. The
duplicate roots of chapter 1.3 are gone.
