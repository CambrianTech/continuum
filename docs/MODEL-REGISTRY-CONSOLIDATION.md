# Model Registry Consolidation: Single Source of Truth

**Date**: 2025-01-24
**Status**: Refactoring Task
**Priority**: High (affects correctness and maintainability)

---

## The Problem: Duplicated Model Configuration

**Current State**: Model configurations are duplicated across multiple files, creating maintenance burden and inconsistency risk.

### Duplication Examples Found

1. **ChatRAGBuilder.ts (lines 623-646)**: Context window map
```typescript
const contextWindows: Record<string, number> = {
  'gpt-4': 8192,
  'gpt-4-turbo': 128000,
  'qwen2.5:7b': 128000,
  // ... 12 more models
};
```

2. **ChatRAGBuilder.ts (lines 701-717)**: DUPLICATE context window map (same file!)
```typescript
const contextWindows: Record<string, number> = {
  'gpt-4': 8192,
  'gpt-4-turbo': 128000,
  'qwen2.5:7b': 128000,
  // ... same 12 models again
};
```

3. **ModelContextWindows.ts (line 34)**: Yet another context window map
```typescript
'qwen2.5:7b': 128000,
```

4. **PersonaResponseGenerator.ts (line 120)**: Another duplicate
```typescript
'qwen2.5:7b': 128000,
```

5. **ComplexityTypes.ts (lines 23, 33)**: Free-text model references
```typescript
// - straightforward → local-fast (qwen2.5:7b, free)
// - 1. local-fast: M1+ hardware, 7B models (qwen2.5:7b) - FREE
```

6. **Probably more**: Grep for model IDs reveals many more

### The Risks

1. **Inconsistency**: Change context window in one place, forget to update others → bugs
2. **Errors**: Typos in free-text model IDs (e.g., "qwen2.5:7b" vs "qwen2.5-7b")
3. **Maintenance**: Adding new model requires updating N files
4. **Discovery**: Hard to find all places that need updating
5. **Costs**: Wrong cost data leads to budget calculation errors

**Real Example**: We fixed qwen2.5:7b from 8192 → 128000 in ONE file today, but it's duplicated in 5+ places!

---

## The Solution: ModelRegistry (Single Source of Truth)

### Create Central Registry

```typescript
// system/shared/ModelRegistry.ts

/**
 * ModelRegistry - Single source of truth for all model configurations
 *
 * CRITICAL: This is the ONLY place model configs should be defined.
 * All other code MUST import from here.
 */

export interface ModelConfig {
  readonly id: string;              // Canonical model ID
  readonly displayName: string;     // Human-readable name
  readonly provider: ModelProvider; // ollama, openai, anthropic, etc.
  readonly contextWindow: number;   // Maximum tokens
  readonly costPer1kTokens: number; // 0 for local models
  readonly tier: ModelTier;         // local-fast, ollama-capable, api-cheap, api-premium
  readonly capabilities: ModelCapability[];
  readonly deprecated?: boolean;    // Mark old models
  readonly aliases?: string[];      // Alternative names
}

export type ModelProvider =
  | 'ollama'
  | 'openai'
  | 'anthropic'
  | 'deepseek'
  | 'groq'
  | 'fireworks'
  | 'together';

export type ModelTier =
  | 'local-fast'
  | 'ollama-capable'
  | 'api-cheap'
  | 'api-premium';

export type ModelCapability =
  | 'text'
  | 'vision'
  | 'function-calling'
  | 'streaming'
  | 'embeddings';

/**
 * Central model registry - THE ONLY SOURCE OF TRUTH
 */
export const MODEL_REGISTRY: Record<string, ModelConfig> = {
  // Local Models (Ollama)
  'qwen2.5:7b': {
    id: 'qwen2.5:7b',
    displayName: 'Qwen 2.5 (7B)',
    provider: 'ollama',
    contextWindow: 128000,
    costPer1kTokens: 0,
    tier: 'local-fast',
    capabilities: ['text', 'streaming']
  },

  'llama3.2:3b': {
    id: 'llama3.2:3b',
    displayName: 'Llama 3.2 (3B)',
    provider: 'ollama',
    contextWindow: 128000,
    costPer1kTokens: 0,
    tier: 'local-fast',
    capabilities: ['text', 'vision', 'streaming']
  },

  'llama3.1:70b': {
    id: 'llama3.1:70b',
    displayName: 'Llama 3.1 (70B)',
    provider: 'ollama',
    contextWindow: 128000,
    costPer1kTokens: 0,
    tier: 'ollama-capable',
    capabilities: ['text', 'streaming', 'function-calling']
  },

  'deepseek-coder:6.7b': {
    id: 'deepseek-coder:6.7b',
    displayName: 'DeepSeek Coder (6.7B)',
    provider: 'ollama',
    contextWindow: 16000,
    costPer1kTokens: 0,
    tier: 'local-fast',
    capabilities: ['text', 'streaming']
  },

  'mistral:7b': {
    id: 'mistral:7b',
    displayName: 'Mistral (7B)',
    provider: 'ollama',
    contextWindow: 32768,
    costPer1kTokens: 0,
    tier: 'local-fast',
    capabilities: ['text', 'streaming']
  },

  // OpenAI Models
  'gpt-4': {
    id: 'gpt-4',
    displayName: 'GPT-4',
    provider: 'openai',
    contextWindow: 8192,
    costPer1kTokens: 0.03,
    tier: 'api-premium',
    capabilities: ['text', 'streaming', 'function-calling']
  },

  'gpt-4-turbo': {
    id: 'gpt-4-turbo',
    displayName: 'GPT-4 Turbo',
    provider: 'openai',
    contextWindow: 128000,
    costPer1kTokens: 0.01,
    tier: 'api-premium',
    capabilities: ['text', 'vision', 'streaming', 'function-calling']
  },

  'gpt-4o': {
    id: 'gpt-4o',
    displayName: 'GPT-4o',
    provider: 'openai',
    contextWindow: 128000,
    costPer1kTokens: 0.005,
    tier: 'api-premium',
    capabilities: ['text', 'vision', 'streaming', 'function-calling']
  },

  'gpt-3.5-turbo': {
    id: 'gpt-3.5-turbo',
    displayName: 'GPT-3.5 Turbo',
    provider: 'openai',
    contextWindow: 16385,
    costPer1kTokens: 0.0015,
    tier: 'api-cheap',
    capabilities: ['text', 'streaming', 'function-calling']
  },

  // Anthropic Models
  'claude-3-opus': {
    id: 'claude-3-opus',
    displayName: 'Claude 3 Opus',
    provider: 'anthropic',
    contextWindow: 200000,
    costPer1kTokens: 0.015,
    tier: 'api-premium',
    capabilities: ['text', 'vision', 'streaming']
  },

  'claude-3-sonnet': {
    id: 'claude-3-sonnet',
    displayName: 'Claude 3 Sonnet',
    provider: 'anthropic',
    contextWindow: 200000,
    costPer1kTokens: 0.003,
    tier: 'api-premium',
    capabilities: ['text', 'vision', 'streaming']
  },

  'claude-3-5-sonnet': {
    id: 'claude-3-5-sonnet',
    displayName: 'Claude 3.5 Sonnet',
    provider: 'anthropic',
    contextWindow: 200000,
    costPer1kTokens: 0.003,
    tier: 'api-premium',
    capabilities: ['text', 'vision', 'streaming', 'function-calling']
  },

  'claude-3-haiku': {
    id: 'claude-3-haiku',
    displayName: 'Claude 3 Haiku',
    provider: 'anthropic',
    contextWindow: 200000,
    costPer1kTokens: 0.00025,
    tier: 'api-cheap',
    capabilities: ['text', 'vision', 'streaming']
  },

  // Other APIs
  'deepseek-chat': {
    id: 'deepseek-chat',
    displayName: 'DeepSeek Chat',
    provider: 'deepseek',
    contextWindow: 64000,
    costPer1kTokens: 0.0001,
    tier: 'api-cheap',
    capabilities: ['text', 'streaming']
  },

  'grok-3': {
    id: 'grok-3',
    displayName: 'Grok 3',
    provider: 'groq',
    contextWindow: 131072,
    costPer1kTokens: 0.0005,
    tier: 'api-cheap',
    capabilities: ['text', 'streaming', 'function-calling']
  }
};

/**
 * Helper functions
 */
export class ModelRegistry {
  /**
   * Get model config by ID (canonical or alias)
   */
  static get(modelId: string): ModelConfig | undefined {
    // Try exact match first
    if (MODEL_REGISTRY[modelId]) {
      return MODEL_REGISTRY[modelId];
    }

    // Try aliases
    for (const config of Object.values(MODEL_REGISTRY)) {
      if (config.aliases?.includes(modelId)) {
        return config;
      }
    }

    return undefined;
  }

  /**
   * Get context window for model
   */
  static getContextWindow(modelId: string): number {
    const config = this.get(modelId);
    return config?.contextWindow || 8192; // Default fallback
  }

  /**
   * Get cost for model
   */
  static getCostPer1kTokens(modelId: string): number {
    const config = this.get(modelId);
    return config?.costPer1kTokens || 0;
  }

  /**
   * Get tier for model
   */
  static getTier(modelId: string): ModelTier {
    const config = this.get(modelId);
    return config?.tier || 'local-fast'; // Default fallback
  }

  /**
   * Check if model has capability
   */
  static hasCapability(modelId: string, capability: ModelCapability): boolean {
    const config = this.get(modelId);
    return config?.capabilities.includes(capability) || false;
  }

  /**
   * Get all models by provider
   */
  static getByProvider(provider: ModelProvider): ModelConfig[] {
    return Object.values(MODEL_REGISTRY).filter(m => m.provider === provider);
  }

  /**
   * Get all models by tier
   */
  static getByTier(tier: ModelTier): ModelConfig[] {
    return Object.values(MODEL_REGISTRY).filter(m => m.tier === tier);
  }

  /**
   * Get all models with capability
   */
  static getByCapability(capability: ModelCapability): ModelConfig[] {
    return Object.values(MODEL_REGISTRY).filter(m =>
      m.capabilities.includes(capability)
    );
  }

  /**
   * List all model IDs
   */
  static listIds(): string[] {
    return Object.keys(MODEL_REGISTRY);
  }

  /**
   * Validate model ID
   */
  static isValid(modelId: string): boolean {
    return this.get(modelId) !== undefined;
  }
}
```

---

## Migration Strategy

### Phase 1: Create ModelRegistry (Week 1)
1. Create `system/shared/ModelRegistry.ts` with all model configs
2. Add unit tests for helper functions
3. Document usage patterns

### Phase 2: Migrate Consumers (Week 2)
1. Update `ChatRAGBuilder.ts`:
```typescript
// OLD (lines 623-646):
const contextWindows: Record<string, number> = {
  'gpt-4': 8192,
  'qwen2.5:7b': 128000,
  // ... etc
};
const contextWindow = contextWindows[modelId] || 8192;

// NEW:
import { ModelRegistry } from '../../shared/ModelRegistry';
const contextWindow = ModelRegistry.getContextWindow(modelId);
```

2. Update `ComplexityTypes.ts`:
```typescript
// OLD (free text):
// - straightforward → local-fast (qwen2.5:7b, free)

// NEW:
import { ModelRegistry } from '../shared/ModelRegistry';
const defaultModel = ModelRegistry.getByTier('local-fast')[0];
// - straightforward → local-fast (${defaultModel.displayName}, ${defaultModel.costPer1kTokens === 0 ? 'free' : '$' + defaultModel.costPer1kTokens + '/1k'})
```

3. Update all other consumers (grep for context window maps)

### Phase 3: Remove Duplication (Week 3)
1. Delete all local context window maps
2. Delete all local cost calculations
3. Ensure all imports from ModelRegistry
4. Run tests to verify no regressions

### Phase 4: Add Validation (Week 4)
1. Add TypeScript validation for model IDs:
```typescript
// Before:
function processModel(modelId: string) { ... }

// After:
import { MODEL_REGISTRY } from './ModelRegistry';
type ValidModelId = keyof typeof MODEL_REGISTRY;

function processModel(modelId: ValidModelId) { ... }

// TypeScript will error if invalid model ID passed!
```

2. Add runtime validation:
```typescript
if (!ModelRegistry.isValid(modelId)) {
  throw new Error(`Invalid model ID: ${modelId}. Valid IDs: ${ModelRegistry.listIds().join(', ')}`);
}
```

---

## Benefits

### Before (Current State)
```typescript
// File 1 (ChatRAGBuilder.ts:623)
const contextWindows = { 'qwen2.5:7b': 128000 };

// File 2 (ChatRAGBuilder.ts:701) - DUPLICATE!
const contextWindows = { 'qwen2.5:7b': 128000 };

// File 3 (ModelContextWindows.ts)
'qwen2.5:7b': 128000,

// File 4 (PersonaResponseGenerator.ts)
'qwen2.5:7b': 128000,

// To add new model: Update 4+ files
// To fix context window: Update 4+ files
// Risk of inconsistency: HIGH
```

### After (With ModelRegistry)
```typescript
// system/shared/ModelRegistry.ts (ONLY PLACE)
export const MODEL_REGISTRY = {
  'qwen2.5:7b': {
    contextWindow: 128000,
    // ... all other config
  }
};

// All other files:
import { ModelRegistry } from '../shared/ModelRegistry';
const contextWindow = ModelRegistry.getContextWindow('qwen2.5:7b');

// To add new model: Update 1 file (ModelRegistry.ts)
// To fix context window: Update 1 file
// Risk of inconsistency: ELIMINATED
```

---

## Testing Strategy

### Unit Tests
```typescript
// tests/unit/ModelRegistry.test.ts

describe('ModelRegistry', () => {
  it('should get context window for valid model', () => {
    const window = ModelRegistry.getContextWindow('qwen2.5:7b');
    expect(window).toBe(128000);
  });

  it('should return default for invalid model', () => {
    const window = ModelRegistry.getContextWindow('invalid-model');
    expect(window).toBe(8192); // Default
  });

  it('should validate model IDs', () => {
    expect(ModelRegistry.isValid('qwen2.5:7b')).toBe(true);
    expect(ModelRegistry.isValid('invalid-model')).toBe(false);
  });

  it('should list all model IDs', () => {
    const ids = ModelRegistry.listIds();
    expect(ids).toContain('qwen2.5:7b');
    expect(ids).toContain('claude-3-5-sonnet');
  });

  it('should get models by tier', () => {
    const localFast = ModelRegistry.getByTier('local-fast');
    expect(localFast.some(m => m.id === 'qwen2.5:7b')).toBe(true);
  });
});
```

### Integration Tests
```bash
# After migration, verify no regressions
npm run build:ts  # Should compile
npx vitest tests/integration/  # All tests pass
./jtag rag/inspect  # Should show correct context windows
```

---

## Refactoring Checklist

- [ ] Create ModelRegistry.ts with all model configs
- [ ] Add ModelRegistry unit tests
- [ ] Update ChatRAGBuilder.ts (remove duplicate maps)
- [ ] Update ComplexityTypes.ts (use ModelRegistry)
- [ ] Update PersonaResponseGenerator.ts
- [ ] Update ModelContextWindows.ts (maybe delete?)
- [ ] Grep for all context window maps and replace
- [ ] Grep for all free-text model references and consolidate
- [ ] Add TypeScript type validation (ValidModelId)
- [ ] Add runtime validation in key functions
- [ ] Run full test suite
- [ ] Document ModelRegistry usage in CLAUDE.md
- [ ] Update PR #192 to use ModelRegistry

---

## Philosophy Alignment

**"A good developer improves the entire system continuously, not just their own new stuff."**

This refactoring exemplifies continuous improvement:
- ✅ Single source of truth (ModelRegistry)
- ✅ Eliminate duplication (4+ copies → 1 source)
- ✅ Prevent future errors (TypeScript validation)
- ✅ Simplify maintenance (1 place to update)
- ✅ Leave code better than we found it

When we implement Phase 2B (RAG Hippocampus), we won't add MORE duplication - we'll use ModelRegistry from the start. This compounds over time into a maintainable, elegant system.
