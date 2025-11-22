# Provider Consolidation Plan

**Goal**: Merge fine-tuning adapters into existing inference provider directories

**Date**: 2025-11-13

---

## Current State

### Inference (Existing)
```
daemons/ai-provider-daemon/adapters/
├── openai/shared/OpenAIAdapter.ts (inference only)
├── together/shared/TogetherAIAdapter.ts (inference only)
├── ollama/shared/OllamaAdapter.ts (inference only)
├── anthropic/shared/AnthropicAdapter.ts (inference only)
├── deepseek/shared/DeepSeekAdapter.ts (inference only)
├── fireworks/shared/FireworksAdapter.ts (inference only)
└── shared/BaseAIProviderAdapter.ts
```

### Fine-Tuning (New, Separate)
```
system/genome/fine-tuning/server/adapters/
├── BaseLoRATrainerServer.ts
├── OpenAILoRAAdapter.ts (training only)
├── TogetherLoRAAdapter.ts (training only)
└── OllamaLoRAAdapter.ts (stub)
```

---

## Target State

```
daemons/ai-provider-daemon/adapters/
├── openai/
│   ├── shared/
│   │   ├── OpenAIAdapter.ts (inference + fine-tuning)
│   │   └── OpenAITypes.ts
│   └── tests/
│       └── openai-integration.test.ts
│
├── together/
│   ├── shared/
│   │   ├── TogetherAIAdapter.ts (inference + fine-tuning)
│   │   └── TogetherTypes.ts
│   └── tests/
│
├── ollama/
│   ├── shared/
│   │   ├── OllamaAdapter.ts (inference + local training)
│   │   └── OllamaTypes.ts
│   └── tests/
│
├── anthropic/
│   └── shared/AnthropicAdapter.ts (inference only - no fine-tuning)
│
└── shared/
    ├── BaseAIProviderAdapter.ts (enhanced)
    └── ProviderCapabilities.ts (NEW)
```

---

## Migration Steps

### Phase 1: Enhance Base Classes

1. **Update BaseAIProviderAdapter** to include optional fine-tuning methods:
   ```typescript
   export abstract class BaseAIProviderAdapter {
     // Existing inference methods
     abstract generateCompletion(...): Promise<...>;

     // NEW: Optional fine-tuning methods
     supportsFineTuning(): boolean { return false; }

     async trainLoRA(request: LoRATrainingRequest): Promise<TrainingResult> {
       throw new Error(`${this.providerId} does not support fine-tuning`);
     }

     async checkTrainingStatus(sessionId: UUID): Promise<TrainingStatus> {
       throw new Error(`${this.providerId} does not support fine-tuning`);
     }

     getFineTuningCapabilities(): FineTuningCapabilities | null {
       return null;
     }
   }
   ```

2. **Create ProviderCapabilities.ts** with shared types:
   ```typescript
   export interface ProviderCapabilities {
     inference: boolean;
     fineTuning: boolean;
     supportedModels: string[];
     costPerToken?: number;
     costPerExample?: number;
     // ...
   }
   ```

### Phase 2: Merge OpenAI (Template)

1. Read existing `OpenAIAdapter.ts`
2. Copy fine-tuning methods from `OpenAILoRAAdapter.ts`
3. Merge into single file
4. Update imports in commands that use it
5. Test compilation
6. Test end-to-end (inference + training)

### Phase 3: Merge Together

1. Same process as OpenAI
2. Move `TogetherLoRAAdapter.ts` → `together/shared/TogetherAIAdapter.ts`

### Phase 4: Merge Ollama

1. Ollama is special - local training with llama.cpp
2. Keep stub for now, implement later

### Phase 5: Cleanup

1. Delete `system/genome/fine-tuning/server/adapters/` directory
2. Update all imports across codebase
3. Update documentation

---

## Benefits

1. **Single Source of Truth**: One file per provider with all capabilities
2. **Model Routing**: Provider knows its own models, no separate registry needed
3. **Shared Configuration**: API keys, endpoints, rate limits in one place
4. **Simpler Testing**: Test inference + training together
5. **Better Discoverability**: `./jtag ai/providers --list` shows all capabilities

---

## Breaking Changes

### Imports
```typescript
// OLD
import { OpenAILoRAAdapter } from 'system/genome/fine-tuning/server/adapters/OpenAILoRAAdapter';

// NEW
import { OpenAIAdapter } from 'daemons/ai-provider-daemon/adapters/openai/shared/OpenAIAdapter';
```

### Commands Affected
- `commands/genome/train/server/GenomeTrainServerCommand.ts`
- Any tests importing LoRA adapters directly

---

## Risk Mitigation

1. **Create git branch**: `refactor/provider-consolidation`
2. **One provider at a time**: OpenAI first (proven working), then others
3. **Keep old files temporarily**: Mark deprecated, delete after all tests pass
4. **Incremental testing**: Test after each provider merge

---

## Testing Strategy

1. **Unit Tests**: Each provider's fine-tuning methods
2. **Integration Tests**: End-to-end training + inference
3. **Compilation**: `npm start` must pass
4. **Manual Testing**: Run actual fine-tuning job with OpenAI

---

## Timeline Estimate

- Phase 1 (Base classes): 30 minutes
- Phase 2 (OpenAI): 1 hour
- Phase 3 (Together): 30 minutes
- Phase 4 (Ollama): 15 minutes (stub)
- Phase 5 (Cleanup): 30 minutes
- **Total**: ~3 hours

---

## Decision

**Proceed with consolidation?** YES - Benefits outweigh the refactoring pain.

**Start with**: Phase 1 (enhance base classes), then Phase 2 (OpenAI as template).
