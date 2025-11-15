# AI Provider Adapter Consolidation Plan

**Goal**: Move ALL fine-tuning adapters from `system/genome/fine-tuning/server/adapters/` into `daemons/ai-provider-daemon/adapters/{provider}/` to consolidate all provider code in one location.

**Date**: 2025-11-14
**Status**: Planning complete, ready for execution

---

## Current State

### Fine-Tuning Adapters (to be moved)
Located in `system/genome/fine-tuning/server/adapters/`:
- `AnthropicLoRAAdapter.ts` → `daemons/ai-provider-daemon/adapters/anthropic/`
- `DeepSeekLoRAAdapter.ts` → `daemons/ai-provider-daemon/adapters/deepseek/`
- `FireworksLoRAAdapter.ts` → `daemons/ai-provider-daemon/adapters/fireworks/`
- `MistralLoRAAdapter.ts` → ❓ (no `mistral/` directory yet)
- `OllamaLoRAAdapter.ts` → `daemons/ai-provider-daemon/adapters/ollama/`
- `OpenAILoRAAdapter.ts` → `daemons/ai-provider-daemon/adapters/openai/`
- `PEFTLoRAAdapter.ts` → ❓ (local training adapter, maybe leave in system/genome?)
- `TogetherLoRAAdapter.ts` → ✅ Already using shared config! Just needs to move

### Existing Provider Directories
Located in `daemons/ai-provider-daemon/adapters/`:
- `anthropic/` - Has inference adapter
- `deepseek/` - Has inference adapter
- `fireworks/` - Has inference adapter
- `groq/` - Has inference adapter (no fine-tuning)
- `ollama/` - Has inference adapter
- `openai/` - Has inference adapter
- `sentinel/` - Has inference adapter (no fine-tuning)
- `together/` - ✅ Has both inference AND fine-tuning (with shared config)
- `xai/` - Has inference adapter (no fine-tuning)

---

## Target Architecture

```
daemons/ai-provider-daemon/adapters/{provider}/
├── shared/
│   ├── {Provider}BaseConfig.ts          # Shared API key, base URL, models
│   ├── {Provider}InferenceAdapter.ts    # Text generation, chat
│   └── {Provider}FineTuningAdapter.ts   # LoRA training
└── index.ts                              # Unified export
```

---

## Migration Strategy

### Phase 1.5: Move Fine-Tuning Adapters (THIS PHASE)

For each provider with fine-tuning support:

1. **Create `server/` subdirectory** (if needed)
   ```bash
   mkdir -p daemons/ai-provider-daemon/adapters/{provider}/server
   ```

2. **Move fine-tuning adapter**
   ```bash
   git mv system/genome/fine-tuning/server/adapters/{Provider}LoRAAdapter.ts \
          daemons/ai-provider-daemon/adapters/{provider}/server/
   ```

3. **Fix imports** in moved file
   - Update relative paths to shared base classes
   - Update paths to types
   - Update paths to constants

4. **Update references** in other files
   - `commands/genome/train/server/GenomeTrainServerCommand.ts`
   - Any test files
   - Registry files

### Providers to Migrate

**High Priority** (have both inference + fine-tuning):
1. ✅ **Together** - Already done! Just needs final move
2. **OpenAI** - Most important, widely used
3. **Fireworks** - Already has inference adapter
4. **DeepSeek** - Already has inference adapter

**Medium Priority**:
5. **Ollama** - Local inference + fine-tuning
6. **Anthropic** - Claude fine-tuning (if supported)

**Special Cases**:
7. **Mistral** - Need to create `mistral/` directory first
8. **PEFT** - Local training, might stay in `system/genome/` as it's different

---

## Detailed Steps for Each Provider

### 1. Together (✅ Phase 1 Complete - Just Move)

```bash
# Already has TogetherBaseConfig and updated imports
# Just move the file
git mv system/genome/fine-tuning/server/adapters/TogetherLoRAAdapter.ts \
       daemons/ai-provider-daemon/adapters/together/server/TogetherFineTuningAdapter.ts

# Update import in GenomeTrainServerCommand.ts
# Old: system/genome/fine-tuning/server/adapters/TogetherLoRAAdapter
# New: daemons/ai-provider-daemon/adapters/together/server/TogetherFineTuningAdapter
```

### 2. OpenAI

```bash
# Create directory
mkdir -p daemons/ai-provider-daemon/adapters/openai/server

# Move file
git mv system/genome/fine-tuning/server/adapters/OpenAILoRAAdapter.ts \
       daemons/ai-provider-daemon/adapters/openai/server/OpenAIFineTuningAdapter.ts

# TODO: Create OpenAIBaseConfig later (like TogetherBaseConfig)
# For now just fix imports
```

### 3. Fireworks

```bash
# Create directory
mkdir -p daemons/ai-provider-daemon/adapters/fireworks/server

# Move file
git mv system/genome/fine-tuning/server/adapters/FireworksLoRAAdapter.ts \
       daemons/ai-provider-daemon/adapters/fireworks/server/FireworksFineTuningAdapter.ts
```

### 4. DeepSeek

```bash
# Create directory
mkdir -p daemons/ai-provider-daemon/adapters/deepseek/server

# Move file
git mv system/genome/fine-tuning/server/adapters/DeepSeekLoRAAdapter.ts \
       daemons/ai-provider-daemon/adapters/deepseek/server/DeepSeekFineTuningAdapter.ts
```

### 5. Ollama

```bash
# Create directory
mkdir -p daemons/ai-provider-daemon/adapters/ollama/server

# Move file
git mv system/genome/fine-tuning/server/adapters/OllamaLoRAAdapter.ts \
       daemons/ai-provider-daemon/adapters/ollama/server/OllamaFineTuningAdapter.ts
```

### 6. Mistral (New Directory)

```bash
# Create full structure
mkdir -p daemons/ai-provider-daemon/adapters/mistral/shared
mkdir -p daemons/ai-provider-daemon/adapters/mistral/server

# Move fine-tuning adapter
git mv system/genome/fine-tuning/server/adapters/MistralLoRAAdapter.ts \
       daemons/ai-provider-daemon/adapters/mistral/server/MistralFineTuningAdapter.ts

# TODO: Create inference adapter later
```

### 7. Anthropic

```bash
# Create directory
mkdir -p daemons/ai-provider-daemon/adapters/anthropic/server

# Move file (if exists and is used)
git mv system/genome/fine-tuning/server/adapters/AnthropicLoRAAdapter.ts \
       daemons/ai-provider-daemon/adapters/anthropic/server/AnthropicFineTuningAdapter.ts
```

---

## Import Path Changes

### Before (Old Paths):
```typescript
import { TogetherLoRAAdapter } from '../../../../../system/genome/fine-tuning/server/adapters/TogetherLoRAAdapter';
import { OpenAILoRAAdapter } from '../../../../../system/genome/fine-tuning/server/adapters/OpenAILoRAAdapter';
```

### After (New Paths):
```typescript
import { TogetherFineTuningAdapter } from '../../../../../daemons/ai-provider-daemon/adapters/together/server/TogetherFineTuningAdapter';
import { OpenAIFineTuningAdapter } from '../../../../../daemons/ai-provider-daemon/adapters/openai/server/OpenAIFineTuningAdapter';
```

---

## Files That Need Updating

### 1. GenomeTrainServerCommand.ts
Location: `commands/genome/train/server/GenomeTrainServerCommand.ts`

**Current imports** (need to update):
```typescript
import { OpenAILoRAAdapter } from '../../../../system/genome/fine-tuning/server/adapters/OpenAILoRAAdapter';
import { FireworksLoRAAdapter } from '../../../../system/genome/fine-tuning/server/adapters/FireworksLoRAAdapter';
import { TogetherLoRAAdapter } from '../../../../system/genome/fine-tuning/server/adapters/TogetherLoRAAdapter';
// ... etc
```

**New imports** (after consolidation):
```typescript
import { OpenAIFineTuningAdapter } from '../../../../daemons/ai-provider-daemon/adapters/openai/server/OpenAIFineTuningAdapter';
import { FireworksFineTuningAdapter } from '../../../../daemons/ai-provider-daemon/adapters/fireworks/server/FireworksFineTuningAdapter';
import { TogetherFineTuningAdapter } from '../../../../daemons/ai-provider-daemon/adapters/together/server/TogetherFineTuningAdapter';
// ... etc
```

### 2. Test Files
Any test files in `system/genome/fine-tuning/server/adapters/api-tests/` will need path updates.

---

## Testing Strategy

After each provider migration:

1. **Compilation test**:
   ```bash
   npm start
   ```

2. **Command registration test**:
   ```bash
   ./jtag list | grep genome
   ```

3. **Interface test** (if API key available):
   ```bash
   ./jtag genome/train --provider={provider} --traitType=test
   ```

---

## Benefits After Consolidation

✅ **Single Source of Truth**: All provider code in one location
✅ **Consistent Organization**: Same structure for every provider
✅ **Easy to Find**: No confusion about where adapters live
✅ **Shared Configuration**: Can extend TogetherBaseConfig pattern to all providers
✅ **Multi-Modal Ready**: Clear place to add audio, video, voice adapters
✅ **Reduced Duplication**: Easier to share code between inference + fine-tuning

---

## Next Steps

1. Start with **Together** (already has shared config, just needs final move)
2. Then **OpenAI** (most widely used)
3. Then **Fireworks**, **DeepSeek**, **Ollama** (in parallel)
4. Finally **Mistral**, **Anthropic** (new directories)
5. Leave **PEFT** in `system/genome/` for now (local training is different)

---

## Rollback Plan

If issues occur:
```bash
# Revert the moves
git mv daemons/ai-provider-daemon/adapters/{provider}/server/*.ts \
       system/genome/fine-tuning/server/adapters/

# Revert import changes
git checkout HEAD -- commands/genome/train/server/GenomeTrainServerCommand.ts

# Rebuild
npm start
```
