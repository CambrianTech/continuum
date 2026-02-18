# Next Session Tasks - AI Provider Refactor

## ✅ Completed This Session

1. **llama3.2:3b model working** - Personas generating correctly
2. **Architecture designed** - DataDaemon pattern documented
3. **RAG adapter system designed** - Capability-aware image processing
4. **Genomic architecture designed** - LoRA hot-swapping, checkpointing
5. **Existing code validated** - shared/ is clean, no Node.js violations

## 🎯 Ready to Implement (Next Session)

### **Priority 1: Auto Model Installation** (2 hours)
**Current Issue**: Model installation is manual or hardcoded somewhere

**Task**: Make it self-healing via adapter pattern

**Files to Create/Modify**:
```typescript
// server/adapters/OllamaServerAdapter.ts (NEW)
export class OllamaServerAdapter implements ServerModelAdapter {
  async install(modelId: string, onProgress?: ProgressCallback): Promise<void> {
    // Use spawn() here - server-only
    const process = spawn('ollama', ['pull', modelId]);
    // Stream progress
  }
}

// shared/AIProviderDaemon.ts (MODIFY)
async generateText(request: TextGenerationRequest): Promise<TextGenerationResponse> {
  // Before generation, check availability
  if (!await adapter.checkAvailability(request.model)) {
    await adapter.install(request.model);  // Auto-install
  }
  return await adapter.generate(request);
}
```

**Success Criteria**:
- ✅ Remove any hardcoded `ollama pull`
- ✅ Persona requests llama3.2:3b → auto-downloads if missing
- ✅ Progress logs show download percentage
- ✅ Self-healing - no manual intervention

### **Priority 2: Add Commands** (1 hour)
**Create user-facing commands**:

```bash
./jtag ai/install --model=llama3.2:3b
./jtag ai/list-models --provider=ollama
./jtag ai/health
./jtag ai/recommend --free-only=true
```

**Files to Create**:
```
commands/ai/
├── install/
│   ├── shared/InstallTypes.ts
│   ├── server/InstallServerCommand.ts
│   └── README.md
├── list-models/
│   └── ... (same pattern)
├── health/
│   └── ... (same pattern)
└── recommend/
    └── ... (same pattern)
```

Each command calls `AIProviderDaemon.static methods` like DataDaemon.

### **Priority 3: Add Free Models** (2 hours)
**Add more free options beyond Ollama**:

1. **DeepSeek-R1** (API-based, 10M free tokens/month)
2. **Qwen 2.5** (via Ollama)
3. **Mistral 7B** (via Ollama)

**Files to Create**:
```
server/adapters/
├── DeepSeekServerAdapter.ts  # HTTP API only
├── QwenServerAdapter.ts       # Reuse Ollama pattern
└── MistralServerAdapter.ts    # Reuse Ollama pattern
```

Each adapter <150 lines, independent.

## 📋 Implementation Order

**Session Start** (30 mins):
1. Read REFACTOR_PLAN_DATADAEMON_PATTERN.md
2. Find hardcoded model installation code
3. Create `server/adapters/` directory

**Core Work** (3 hours):
4. Implement `OllamaServerAdapter` with `install()`
5. Update `AIProviderDaemonServer` to use adapter
6. Test auto-installation flow
7. Create `commands/ai/install` command
8. Test via `./jtag ai/install --model=llama3.2:3b`

**Polish** (1 hour):
9. Add DeepSeekServerAdapter
10. Create `commands/ai/recommend`
11. Document in ARCHITECTURE.md

## 🔍 Code Locations

**Existing working code**:
- `shared/AIProviderDaemon.ts` - Main daemon (clean)
- `shared/OllamaAdapter.ts` - Ollama integration (clean)
- `server/AIProviderDaemonServer.ts` - Server registration
- `system/user/shared/PersonaUser.ts` - Uses llama3.2:3b

**Where hardcoded install might be**:
- `AIProviderDaemonServer.ts` initialization?
- `OllamaAdapter.ts` initialization?
- `PersonaUser.ts` model setup?
- Search: `grep -r "ollama pull" .`

## ✅ Validation Tests

```bash
# 1. Auto-installation
rm -rf ~/.ollama/models/llama3.2*
./jtag exec --code="/* trigger persona response */"
# Expect: Logs show "📦 Installing llama3.2:3b... 50%... 100%"

# 2. Commands work
./jtag ai/list-models
# Expect: Shows llama3.2:3b, phi3:mini, etc.

./jtag ai/recommend --free-only=true
# Expect: Lists free models with descriptions

# 3. Architecture clean
grep -r "spawn" daemons/ai-provider-daemon/shared/
# Expect: ZERO results

grep -r "spawn" daemons/ai-provider-daemon/server/
# Expect: Only in adapters/
```

## 📚 Documentation to Read

1. `REFACTOR_PLAN_DATADAEMON_PATTERN.md` - Full architecture
2. `ARCHITECTURE.md` - Existing design
3. `ELEGANT_ADAPTER_REFACTOR.md` - Genomic system design
4. `AI_DAEMON_GENOMIC_ARCHITECTURE.md` - Full vision

## 🎯 Success Criteria

**After next session:**
- ✅ Models auto-install on first use (self-healing)
- ✅ Commands work: `./jtag ai/*`
- ✅ At least 3 free models supported (Ollama, DeepSeek, Qwen)
- ✅ All files <200 lines
- ✅ Zero Node.js code in shared/
- ✅ llama3.2:3b still works (zero downtime)

---

**Estimated Time**: 4-5 hours total
**Difficulty**: Medium (pattern is proven, just needs execution)
**Impact**: High (enables genomic persona evolution)
