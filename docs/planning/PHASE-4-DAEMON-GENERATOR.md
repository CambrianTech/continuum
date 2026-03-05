# Phase 4: Daemon Generator Implementation Plan

**Goal**: Enable generation of daemons from declarative specs, laying the foundation for LoRA infrastructure.

## Why This Matters

Daemon generator is the **critical path to alpha**:
- LoRA Manager Daemon → Adapter loading/unloading with LRU eviction
- Training Pipeline Daemon → Fine-tuning workflow orchestration
- These enable **cost-effective, specialized AI** ($0.10-8 per million tokens vs $3-60)

## Implementation Tasks

### Task 1: Study Daemon Patterns (~2 hours)
**Goal**: Understand existing daemon patterns to extract reusable abstractions

**Steps**:
1. Analyze `daemons/data-daemon/` - CRUD operations, storage adapter pattern
2. Analyze `daemons/ai-provider-daemon/` - Model switching, adapter registry
3. Analyze `daemons/system-daemon/` - Health monitoring, lifecycle management
4. Identify common patterns:
   - Lifecycle hooks (onStart, onStop)
   - Jobs (async methods with typed params/results)
   - Events (pub/sub with typed payloads)
   - State management (persistent vs ephemeral)
5. Document patterns in architecture doc

**Deliverable**: `generator/DAEMON-PATTERNS.md` documenting common abstractions

### Task 2: Create Daemon Templates (~4 hours)
**Goal**: Template files that generate complete, working daemons from specs

**Files to Create**:
1. `generator/templates/daemon/shared-types.template.ts`
   - Daemon params interface
   - Jobs interface (typed methods)
   - Events interface (typed payloads)
   - Daemon state interface (if needed)

2. `generator/templates/daemon/server.template.ts`
   - Daemon class extending `BaseDaemon<Params, Jobs, Events>`
   - Lifecycle hooks (onStart, onStop)
   - Job implementations (stubs with TODO comments)
   - Event emitters

3. `generator/templates/daemon/README.template.md`
   - Purpose and usage
   - Configuration parameters
   - Available jobs with examples
   - Events emitted
   - Example usage from CLI/code

**Token Replacements Needed**:
- `{{DAEMON_NAME_PASCAL}}` - LoRAManager, TrainingPipeline
- `{{DAEMON_NAME_KEBAB}}` - lora-manager, training-pipeline
- `{{PARAMS_INTERFACE}}` - Generated from spec params
- `{{JOBS_INTERFACE}}` - Generated from spec jobs
- `{{EVENTS_INTERFACE}}` - Generated from spec events
- `{{LIFECYCLE_HOOKS}}` - onStart/onStop implementations
- `{{JOB_IMPLEMENTATIONS}}` - Async methods for each job
- `{{EVENT_EMITTERS}}` - Helper methods to emit typed events

**Deliverable**: Three template files ready for token replacement

### Task 3: Extend CommandGenerator (~2 hours)
**Goal**: CommandGenerator can generate daemons from JSON specs

**Changes to `generator/CommandGenerator.ts`**:
1. Add `generateDaemon(spec: DaemonSpec): Promise<void>` method
2. Parse daemon spec JSON
3. Build tokens for daemon templates
4. Replace tokens in templates
5. Write generated files to `daemons/{daemon-name}/`
6. Test with example spec

**Spec Format**:
```json
{
  "name": "lora-manager",
  "description": "Manages LoRA adapter loading/unloading",
  "params": {
    "maxAdapters": { "type": "number", "default": 5 },
    "evictionPolicy": { "type": "enum", "values": ["lru", "priority"] }
  },
  "jobs": [
    {
      "name": "loadAdapter",
      "params": [{ "name": "path", "type": "string" }],
      "returns": "AdapterHandle",
      "async": true
    }
  ],
  "events": [
    {
      "name": "adapter:loaded",
      "payload": { "adapterId": "UUID", "domain": "string" }
    }
  ],
  "lifecycle": {
    "onStart": "Initialize adapter cache",
    "onStop": "Unload all adapters gracefully"
  }
}
```

**Deliverable**: `./jtag generate daemons/my-daemon.spec.json` works

### Task 4: Update Audit System (~2 hours)
**Goal**: Audit system validates generated daemons

**New Audit Checks**:
1. `DaemonStructureCheck` - Validates daemon extends BaseDaemon
2. `DaemonLifecycleCheck` - Validates onStart/onStop present and typed
3. `DaemonJobsCheck` - Validates jobs match interface
4. `DaemonEventsCheck` - Validates events properly typed

**Integration**:
- Add daemon checks to `generator/audit/ModuleAuditor.ts`
- Handle auto-fix for common issues
- Test with generated daemons

**Deliverable**: `./jtag generate/audit --type="daemon"` validates daemons

### Task 5: Dogfooding Test (~2 hours)
**Goal**: Verify generated daemons work end-to-end

**Test Cases**:
1. **LoRA Manager Daemon**:
   - Create `daemons/lora-manager.spec.json`
   - Generate: `./jtag generate daemons/lora-manager.spec.json`
   - Verify files created (shared/types, server/daemon, README)
   - Run audit: `./jtag generate/audit --module="daemons/lora-manager"`
   - Verify zero errors, zero warnings
   - Compile TypeScript: `npx tsc --noEmit`
   - (Optional) Test daemon actually runs

2. **Training Pipeline Daemon**:
   - Create `daemons/training-pipeline.spec.json`
   - Generate: `./jtag generate daemons/training-pipeline.spec.json`
   - Same verification steps as above

**Deliverable**: Two working daemons generated from specs, passing audits

## Success Criteria

✅ **Generator Works**: Daemon generation from spec produces complete, working daemon
✅ **Audit Passes**: Generated daemons pass all checks with zero errors
✅ **Dogfooding**: LoRA manager and training pipeline daemons work
✅ **Self-Policing**: Generator commands pass their own audits
✅ **Documentation**: README updated, examples provided

## Estimated Timeline

- Task 1: ~2 hours
- Task 2: ~4 hours
- Task 3: ~2 hours
- Task 4: ~2 hours
- Task 5: ~2 hours

**Total: 12-16 hours**

## What's Next (Phase 5)

After daemon generator is complete:
- ✅ Command generator (Phase 1 - shipped)
- ✅ Daemon generator (Phase 2 - this phase)
- 📋 Widget generator (Phase 3 - 17-20 hours)

Widget generator enables CSS/design personas → LoRA training for visual tasks.
