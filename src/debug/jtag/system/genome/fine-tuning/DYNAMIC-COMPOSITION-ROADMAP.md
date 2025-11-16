# Dynamic LoRA Composition Implementation Roadmap

## Executive Summary

**Goal**: Enable modular LoRA training with dynamic composition at inference time.

**Key Insight**: Train N domains + M personalities = N+M jobs, get N×M combinations dynamically!

**Status**: Fine-tuning works for 4 providers (OpenAI, Fireworks, Together, DeepSeek*). Next phase: PEFT integration for dynamic composition.

---

## The Architecture (Discovered 2025-11-15)

### Two-Tier System

**Tier 1: PEFT (Local Inference)**
- Unlimited dynamic composition via `set_adapters()`
- Instant switching between layer combinations (< 1ms)
- Zero additional inference cost
- Full control over composition weights

**Tier 2: Remote APIs (Fireworks, OpenAI, etc.)**
- Single composite adapter per inference (maxActiveAdapters: 1)
- Pre-merge popular combinations offline
- Deploy to cloud for scale
- Pay per token (~$0.2/1M tokens)

### Training Strategy

```typescript
// Phase 1: Train modular domain layers (ONCE each)
await trainLoRA({ traitType: "wine-expertise", provider: "fireworks" });
await trainLoRA({ traitType: "typescript-expertise", provider: "fireworks" });
await trainLoRA({ traitType: "legal-knowledge", provider: "openai" });

// Phase 2: Train personality layers (ONCE each)
await trainLoRA({ traitType: "vin-diesel-style", provider: "fireworks" });
await trainLoRA({ traitType: "shakespeare-style", provider: "openai" });
await trainLoRA({ traitType: "einstein-style", provider: "deepseek" });

// Result: 6 training jobs → 3×3 = 9 persona combinations!
```

### Inference Strategy

**Option A: Local PEFT (Dynamic Composition)**
```python
# Load base model + modular adapters
peft_model.load_adapter("wine-expertise", adapter_name="wine")
peft_model.load_adapter("vin-diesel-style", adapter_name="personality")

# Compose dynamically
peft_model.set_adapters(["wine", "personality"], adapter_weights=[0.7, 0.3])
response = peft_model.generate(prompt)  # Vin Diesel sommelier!

# Switch instantly
peft_model.set_adapters(["wine", "shakespeare"], adapter_weights=[0.7, 0.3])
response = peft_model.generate(prompt)  # Shakespearean sommelier!
```

**Option B: Remote API (Pre-merged Composites)**
```typescript
// Merge popular combinations offline
const composite = await mergePEFT({
  adapters: ["wine-expertise", "vin-diesel-style"],
  weights: [0.7, 0.3],
  method: "TIES"
});

// Deploy to Fireworks
await deployToFireworks({ adapter: composite, name: "vin-diesel-sommelier" });

// Inference (simple routing)
await fireworks.inference({ lora: "vin-diesel-sommelier" });
```

---

## Implementation Phases

### ✅ Phase 0: Multi-Provider Fine-Tuning (COMPLETE)
- [x] OpenAI LoRA training working
- [x] Fireworks LoRA training working
- [x] Together LoRA training working
- [ ] DeepSeek LoRA training (404 error - needs fix)
- [x] End-to-end test suite (genome-fine-tuning-e2e.test.ts)
- [x] Handle-based async pattern (BaseLoRATrainerServer)

### 🚧 Phase 1: PEFT Integration (NEXT)

**Goal**: Get PEFT working locally for dynamic composition

**Tasks**:
1. **Download trained adapters from providers**
   ```typescript
   // Provider-specific download logic
   const openaiAdapter = await downloadFromOpenAI(jobId);
   const fireworksAdapter = await downloadFromFireworks(jobId);
   const togetherAdapter = await downloadFromTogether(jobId);
   ```

2. **Convert to PEFT-compatible format**
   ```python
   # Python script: convert-to-peft.py
   from peft import PeftModel, PeftConfig

   # Load provider-specific format
   adapter = load_provider_adapter(path, provider_type)

   # Convert to PEFT safetensors
   peft_adapter = convert_to_peft(adapter)
   peft_adapter.save_pretrained(output_path)
   ```

3. **PEFT composition service**
   ```typescript
   // New adapter: PEFTCompositionAdapter
   class PEFTCompositionAdapter {
     async loadAdapter(name: string, path: string): Promise<void>;
     async setComposition(adapters: string[], weights: number[]): Promise<void>;
     async generate(prompt: string): Promise<string>;
   }
   ```

4. **Test dynamic composition**
   ```typescript
   // Test: tests/integration/peft-composition.test.ts
   const peft = new PEFTCompositionAdapter();
   await peft.loadAdapter("wine", "./adapters/wine-expertise");
   await peft.loadAdapter("personality", "./adapters/vin-diesel-style");

   await peft.setComposition(["wine", "personality"], [0.7, 0.3]);
   const response1 = await peft.generate("Describe Cabernet Sauvignon");

   await peft.setComposition(["wine", "personality"], [0.9, 0.1]);
   const response2 = await peft.generate("Describe Cabernet Sauvignon");

   // Verify composition affects output
   expect(response1).toContain("family"); // More Vin Diesel style
   expect(response2).toContain("tannins"); // More wine expertise
   ```

**Deliverables**:
- [ ] Provider download scripts (1 per provider)
- [ ] PEFT conversion script (Python)
- [ ] PEFTCompositionAdapter (TypeScript wrapper)
- [ ] Integration test proving dynamic composition works
- [ ] Documentation: PEFT-INTEGRATION.md

**Estimated Time**: 3-5 days

---

### 📋 Phase 2: Offline Merging (After Phase 1)

**Goal**: Merge popular combinations for deployment to remote APIs

**Tasks**:
1. **PEFT merging service**
   ```python
   # merge-adapters.py
   from peft import PeftModel

   # Load adapters
   model.load_adapter("wine-expertise", adapter_name="wine")
   model.load_adapter("vin-diesel-style", adapter_name="personality")

   # Merge with advanced method
   merged = model.add_weighted_adapter(
     adapters=["wine", "personality"],
     weights=[0.7, 0.3],
     adapter_name="merged",
     combination_type="TIES"  # or "DARE"
   )

   # Save merged adapter
   merged.save_pretrained("./merged/vin-diesel-sommelier")
   ```

2. **Fireworks deployment**
   ```typescript
   // Deploy merged composite
   const response = await fetch("https://api.fireworks.ai/v1/adapters", {
     method: "POST",
     body: formData,  // tar.gz of merged adapter
   });

   const { adapter_id } = await response.json();
   // Returns: "lora:vin-diesel-sommelier:v1"
   ```

3. **Composition config storage**
   ```typescript
   // Extend GenomeLayerEntity
   interface GenomeLayerEntity extends BaseEntity {
     personaId: UUID;
     layerType: "modular" | "composite";

     // For modular layers
     traitType?: string;  // "wine-expertise", "vin-diesel-style"
     category?: "domain" | "personality";

     // For composite layers
     composition?: {
       method: "TIES" | "DARE" | "linear";
       adapters: Array<{ name: string; weight: number }>;
       mergedAdapterId?: string;  // Fireworks adapter_id
     };

     // Training metadata
     baseModel: string;
     provider: string;
     providerJobId: string;
     trainingJobId: UUID;
     localPath?: string;  // For PEFT
   }
   ```

**Deliverables**:
- [ ] merge-adapters.py script
- [ ] Fireworks deployment logic
- [ ] GenomeLayerEntity schema extension
- [ ] CRUD commands for managing layers
- [ ] Test: offline merge → deploy → inference

**Estimated Time**: 2-3 days

---

### 📋 Phase 3: PersonaUser Integration (After Phase 2)

**Goal**: PersonaUsers automatically select and compose layers based on task

**Tasks**:
1. **Genome selection logic**
   ```typescript
   class PersonaGenome {
     async selectLayers(task: Task): Promise<string[]> {
       // Determine required domain from task
       const domain = this.classifyTaskDomain(task);

       // Get personality from persona config
       const personality = this.persona.personalityStyle;

       // Return layer composition
       return [
         `${domain}-expertise`,  // e.g., "wine-expertise"
         `${personality}-style`  // e.g., "vin-diesel-style"
       ];
     }

     async generate(prompt: string, task: Task): Promise<string> {
       const layers = await this.selectLayers(task);

       if (this.provider === "peft") {
         // Dynamic composition
         await this.peft.setComposition(layers, [0.7, 0.3]);
         return await this.peft.generate(prompt);
       } else {
         // Use pre-merged composite
         const compositeId = await this.findComposite(layers);
         return await this.remoteAPI.generate(prompt, compositeId);
       }
     }
   }
   ```

2. **Layer distribution system**
   ```bash
   # New commands
   ./jtag genome/layer-train --traitType="wine-expertise" --provider="fireworks"
   ./jtag genome/layer-list --category="domain"
   ./jtag genome/composite-create --layers="wine,vin-diesel" --weights="0.7,0.3"
   ./jtag genome/composite-deploy --compositeId="UUID" --provider="fireworks"
   ```

3. **Automatic composition**
   ```typescript
   // PersonaUser autonomously composes layers
   async serviceInbox(): Promise<void> {
     const task = await this.inbox.peek();

     // Auto-select layers based on task domain
     const layers = await this.genome.selectLayers(task);

     // Compose if using PEFT
     if (this.provider === "peft") {
       await this.genome.setComposition(layers);
     }

     // Process task with composed genome
     await this.processTask(task);
   }
   ```

**Deliverables**:
- [ ] PersonaGenome class with selection logic
- [ ] genome/* JTAG commands
- [ ] Automatic layer composition in PersonaUser
- [ ] Test: persona switches layers based on task domain
- [ ] Documentation: GENOME-USAGE.md

**Estimated Time**: 4-6 days

---

### 📋 Phase 4: Continuous Learning (Future)

**Goal**: PersonaUsers create training tasks for themselves

**Tasks**:
1. Self-identify knowledge gaps
2. Generate training data from mistakes
3. Schedule fine-tuning as regular task
4. Update modular layers incrementally

**Estimated Time**: 1-2 weeks

---

## Success Criteria

### Phase 1 Success:
- [ ] PEFT loads 2+ modular adapters simultaneously
- [ ] `set_adapters()` changes composition without reload
- [ ] Composition affects inference output (verified by test)
- [ ] Switching takes < 100ms

### Phase 2 Success:
- [ ] Merge 2 modular layers offline with PEFT
- [ ] Deploy merged composite to Fireworks
- [ ] Inference uses deployed composite
- [ ] GenomeLayerEntity stores composition metadata

### Phase 3 Success:
- [ ] PersonaUser auto-selects layers based on task domain
- [ ] PEFT personas compose dynamically
- [ ] Remote API personas use pre-merged composites
- [ ] Layer distribution works (train once, all personas use it)

---

## Cost Analysis

### Old Approach (Persona-Specific Training):
- 10 personas × $15/job = **$150**
- Sequential training (weeks)
- Can't share knowledge between personas

### New Approach (Modular Layers):
- 5 domains + 5 personalities = **$150** (same cost!)
- But get 5×5 = **25 persona combinations**
- Parallel training (days)
- Share domain expertise across all personas

**Result**: **5x more personas for same cost**, plus instant composition switching

---

## Next Steps (Immediate)

1. **Fix DeepSeek 404 error** (blocks Phase 0 completion)
2. **Research provider download APIs** (needed for Phase 1)
3. **Set up Python environment for PEFT** (create requirements.txt)
4. **Prototype PEFT composition** (simple test script)
5. **Design GenomeLayerEntity schema** (extends BaseEntity)

---

## References

**Documentation**:
- HuggingFace PEFT: https://huggingface.co/docs/peft
- Fireworks Multi-LoRA: https://fireworks.ai/blog/multi-lora
- PEFT Merging Methods: https://huggingface.co/blog/peft_merging

**Related Docs**:
- [LORA-GENOME-PAGING.md](../user/server/modules/LORA-GENOME-PAGING.md) - Virtual memory pattern
- [PERSONA-CONVERGENCE-ROADMAP.md](../user/server/modules/PERSONA-CONVERGENCE-ROADMAP.md) - Overall vision
- [genome-fine-tuning-e2e.test.ts](../../tests/integration/genome-fine-tuning-e2e.test.ts) - Current test suite

**AI Team Discussion** (2025-11-15):
- PEFT download/conversion workflow
- Fireworks /v1/adapters API
- GenomeLayerEntity schema design
- Composition metadata storage strategy
