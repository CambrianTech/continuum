# Multi-Modal AI Provider Architecture

**Vision**: Enable personas to create custom avatars (like Vin Diesel) through fine-tuning across ALL modalities - text, audio, video, images, voice.

**Goal**: Modular npm-package-like structure where each capability is independent but shares configuration.

---

## The Future We're Building Toward

### Persona Avatar Creation Workflow

```typescript
// 1. Persona collects training data
const vinDieselData = {
  text: await persona.collectTextSamples(),      // Dialogue, scripts
  audio: await persona.collectAudioSamples(),    // Voice samples
  video: await persona.collectVideoSamples(),    // Acting clips
  images: await persona.collectImageSamples()    // Facial expressions
};

// 2. Fine-tune across all modalities
const together = new TogetherAdapter();

// Text personality
const textModel = await together.fineTuning.trainLoRA({
  modality: 'text',
  dataset: vinDieselData.text,
  baseModel: 'meta-llama/Meta-Llama-3.1-8B-Instruct-Reference'
});

// Voice cloning
const voiceModel = await together.fineTuning.trainVoice({
  modality: 'audio',
  dataset: vinDieselData.audio,
  baseModel: 'voice-clone-base'
});

// Video avatar
const videoModel = await together.fineTuning.trainVideo({
  modality: 'video',
  dataset: vinDieselData.video,
  baseModel: 'avatar-generation-base'
});

// 3. Persona uses avatar across all interactions
await persona.setAvatar({
  text: textModel,
  voice: voiceModel,
  video: videoModel
});

// 4. Unified inference across modalities
await persona.respond({
  input: userMessage,
  outputModalities: ['text', 'voice', 'video']
});
```

---

## Modular Architecture

### Directory Structure

```
daemons/ai-provider-daemon/adapters/together/
├── shared/
│   ├── TogetherBaseConfig.ts          # Shared state (API key, auth, rate limiting)
│   ├── TogetherTextAdapter.ts         # Text generation (chat, completion)
│   ├── TogetherEmbeddingsAdapter.ts   # Vector embeddings
│   ├── TogetherImageAdapter.ts        # Image generation (DALL-E style)
│   ├── TogetherAudioAdapter.ts        # Audio transcription, synthesis
│   ├── TogetherVideoAdapter.ts        # Video understanding, generation
│   ├── TogetherVoiceAdapter.ts        # Voice cloning, synthesis
│   └── index.ts                       # Barrel exports
├── server/
│   ├── fine-tuning/
│   │   ├── TogetherTextFineTuning.ts    # Text model fine-tuning
│   │   ├── TogetherAudioFineTuning.ts   # Audio model fine-tuning
│   │   ├── TogetherVideoFineTuning.ts   # Video model fine-tuning
│   │   ├── TogetherVoiceFineTuning.ts   # Voice cloning fine-tuning
│   │   └── TogetherImageFineTuning.ts   # Image model fine-tuning
│   └── index.ts
└── index.ts                           # Main adapter entry point
```

### Main Adapter Class

```typescript
// adapters/together/index.ts
export class TogetherAdapter {
  private readonly config: TogetherBaseConfig;

  // Inference capabilities
  readonly text: TogetherTextAdapter;
  readonly embeddings: TogetherEmbeddingsAdapter;
  readonly image: TogetherImageAdapter;
  readonly audio: TogetherAudioAdapter;
  readonly video: TogetherVideoAdapter;
  readonly voice: TogetherVoiceAdapter;

  // Fine-tuning capabilities
  readonly fineTuning: {
    text: TogetherTextFineTuning;
    audio: TogetherAudioFineTuning;
    video: TogetherVideoFineTuning;
    voice: TogetherVoiceFineTuning;
    image: TogetherImageFineTuning;
  };

  constructor(apiKey?: string) {
    this.config = new TogetherBaseConfig(apiKey);

    // Initialize all capabilities with shared config
    this.text = new TogetherTextAdapter(this.config);
    this.embeddings = new TogetherEmbeddingsAdapter(this.config);
    this.image = new TogetherImageAdapter(this.config);
    this.audio = new TogetherAudioAdapter(this.config);
    this.video = new TogetherVideoAdapter(this.config);
    this.voice = new TogetherVoiceAdapter(this.config);

    this.fineTuning = {
      text: new TogetherTextFineTuning(this.config),
      audio: new TogetherAudioFineTuning(this.config),
      video: new TogetherVideoFineTuning(this.config),
      voice: new TogetherVoiceFineTuning(this.config),
      image: new TogetherImageFineTuning(this.config)
    };
  }
}
```

---

## Shared Configuration (TogetherBaseConfig)

**Purpose**: Eliminate duplication - ONE place for API key, models, pricing, rate limiting.

```typescript
export class TogetherBaseConfig {
  readonly providerId = 'together';
  readonly providerName = 'Together AI';
  readonly baseUrl = 'https://api.together.xyz';
  readonly apiKey: string;

  private modelsCache?: ModelInfo[];
  private pricingCache?: Map<string, ModelPricing>;

  constructor(apiKey?: string) {
    this.apiKey = apiKey || getSecret('TOGETHER_API_KEY', 'TogetherAdapter') || '';
  }

  /**
   * Get available models (cached across all adapters)
   */
  async getModels(): Promise<ModelInfo[]> {
    if (this.modelsCache) return this.modelsCache;

    // Fetch from Together API
    const response = await fetch(`${this.baseUrl}/v1/models`, {
      headers: { 'Authorization': `Bearer ${this.apiKey}` }
    });

    this.modelsCache = await response.json();
    return this.modelsCache;
  }

  /**
   * Get pricing (cached across all adapters)
   */
  async getPricing(): Promise<Map<string, ModelPricing>> {
    if (this.pricingCache) return this.pricingCache;

    // Fetch from OpenRouter or Together API
    this.pricingCache = await PricingFetcher.fetchFromOpenRouter();
    return this.pricingCache;
  }

  /**
   * Make authenticated API request (shared by all adapters)
   */
  async request<T>(endpoint: string, options: RequestInit): Promise<T> {
    const response = await fetch(`${this.baseUrl}${endpoint}`, {
      ...options,
      headers: {
        'Authorization': `Bearer ${this.apiKey}`,
        'Content-Type': 'application/json',
        ...options.headers
      }
    });

    if (!response.ok) {
      const error = await response.text();
      throw new Error(`Together API error: ${response.status} - ${error}`);
    }

    return await response.json();
  }
}
```

---

## Benefits of This Architecture

### 1. Zero Duplication
- API key loaded once, shared everywhere
- Models fetched once, cached for all capabilities
- Pricing fetched once, reused across inference + fine-tuning
- Auth logic in ONE place

### 2. Tree-Shaking
```typescript
// Only import what you use
import { TogetherAdapter } from 'adapters/together';

const together = new TogetherAdapter();

// Only text inference? Other modules can be tree-shaken by bundler
await together.text.generate({ ... });
```

### 3. Lazy Loading
```typescript
// Load fine-tuning only when needed
if (needsFineTuning) {
  const { TogetherAdapter } = await import('adapters/together');
  await together.fineTuning.text.train({ ... });
}
```

### 4. Clear Module Boundaries
Each file is ~150-300 lines, focused on ONE capability:
- `TogetherTextAdapter.ts` - Just text inference
- `TogetherTextFineTuning.ts` - Just text fine-tuning
- `TogetherVoiceAdapter.ts` - Just voice inference
- `TogetherVoiceFineTuning.ts` - Just voice fine-tuning

### 5. Extensible
Adding new capability = add one file + one line in main adapter:

```typescript
// New capability: Multi-modal understanding
readonly multimodal: TogetherMultimodalAdapter;

constructor() {
  // ...
  this.multimodal = new TogetherMultimodalAdapter(this.config);
}
```

---

## Implementation Plan

### Phase 1: Foundation (TODAY)
1. Create `TogetherBaseConfig.ts` - Shared state
2. Refactor existing `TogetherAIAdapter.ts` → `TogetherTextAdapter.ts`
3. Create `TogetherTextFineTuning.ts` from existing fine-tuning code
4. Create main `TogetherAdapter` class
5. Update command registrations
6. Test compilation

**Time estimate**: 2-3 hours

### Phase 2: Expand Inference (NEXT)
1. Create `TogetherEmbeddingsAdapter.ts`
2. Create `TogetherImageAdapter.ts` (if Together supports)
3. Create `TogetherAudioAdapter.ts` (if Together supports)

**Time estimate**: 4-6 hours

### Phase 3: Multi-Modal Fine-Tuning (FUTURE)
1. Create `TogetherAudioFineTuning.ts`
2. Create `TogetherVideoFineTuning.ts`
3. Create `TogetherVoiceFineTuning.ts`
4. Create `TogetherImageFineTuning.ts`

**Time estimate**: 8-12 hours

### Phase 4: Persona Avatar System (FUTURE)
1. Create `PersonaAvatar` entity
2. Add avatar management commands
3. Integrate with PersonaUser
4. Build avatar creation workflow UI

**Time estimate**: 20-30 hours

---

## Rollout to Other Providers

Once Together is modularized, apply same pattern to:

### OpenAI
```
adapters/openai/
├── shared/
│   ├── OpenAIBaseConfig.ts
│   ├── OpenAITextAdapter.ts
│   ├── OpenAIImageAdapter.ts (DALL-E)
│   ├── OpenAIAudioAdapter.ts (Whisper)
│   └── index.ts
├── server/
│   ├── fine-tuning/
│   │   └── OpenAITextFineTuning.ts
│   └── index.ts
└── index.ts
```

### Anthropic
```
adapters/anthropic/
├── shared/
│   ├── AnthropicBaseConfig.ts
│   ├── AnthropicTextAdapter.ts (Claude)
│   └── index.ts
├── server/
│   ├── fine-tuning/
│   │   └── AnthropicTextFineTuning.ts (when available)
│   └── index.ts
└── index.ts
```

---

## Success Criteria

✅ **Foundation Phase Complete When**:
- Together adapter compiles with 0 errors
- Text inference works: `together.text.generate()`
- Text fine-tuning works: `together.fineTuning.text.train()`
- Zero code duplication (shared config used everywhere)
- Documentation complete

✅ **Multi-Modal Phase Complete When**:
- All inference modalities implemented
- All fine-tuning modalities implemented
- Persona can create avatar across modalities
- Avatar stored and retrieved from database

✅ **Production Ready When**:
- All tests passing
- Performance benchmarks documented
- Rate limiting implemented
- Error handling comprehensive
- Logging and monitoring integrated

---

## Next Steps

**Starting now**: Implement Phase 1 (Foundation)

1. Create `TogetherBaseConfig.ts`
2. Refactor inference adapter to use it
3. Create fine-tuning adapter using it
4. Wire together in main `TogetherAdapter`
5. Test and document

This foundation enables EVERYTHING - Vin Diesel avatars, multi-modal fine-tuning, personalized AI interactions across all modalities.
