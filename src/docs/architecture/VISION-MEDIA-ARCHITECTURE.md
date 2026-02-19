# Vision & Media Architecture

**Status**: Phase 1 Complete (Basic Format Handling) | Phase 2-4 Planned
**Author**: Claude Code
**Date**: 2025-11-26

## Table of Contents
1. [Current Implementation](#current-implementation)
2. [Identified Problems](#identified-problems)
3. [Architecture Overview](#architecture-overview)
4. [Media Processing Pipeline](#media-processing-pipeline)
5. [RAG Integration (CSS Flexbox Analogy)](#rag-integration)
6. [Implementation Phases](#implementation-phases)
7. [Command Specifications](#command-specifications)

---

## Current Implementation

### ✅ Phase 1: Basic Format Handling (COMPLETE)

**What Works:**
- Vision models (Grok-vision, GPT-4V) receive multimodal ContentPart[] format
- Non-vision models receive flattened text (images discarded)
- Anthropic/Claude handles nested `part.image.base64` format
- OpenAI-compatible adapters detect vision capability per model

**Files Modified:**
- `adapters/anthropic/shared/AnthropicAdapter.ts:279-294` - Flexible format detection
- `shared/adapters/BaseOpenAICompatibleAdapter.ts:147-173` - Vision capability detection + message flattening
- `system/user/server/modules/PersonaResponseGenerator.ts:392-399` - ContentPart[] generation

**Format Sent from PersonaResponseGenerator:**
```typescript
{
  type: 'image',
  image: {           // Nested (OpenAI-style)
    base64: string,
    mimeType: string
  }
}
```

**Adapter Handling:**
- **Grok/XAI**: ✅ Works (BaseOpenAICompatibleAdapter)
- **Claude**: ✅ Fixed (checks both `part.base64` and `part.image?.base64`)
- **Groq/DeepSeek/Together/Fireworks**: ✅ Fixed (non-vision models get plain text)

---

## Identified Problems

### 🔴 Critical Production Issues

#### 1. **Images Discarded for Non-Vision Models**
**Problem**: Non-vision models lose all visual information when images are present.

**Example**:
```
User sends: "Describe this meme [image of Steve Buscemi]"
Groq Lightning receives: "Describe this meme"  // Image lost!
Response: "I don't see any meme to describe."
```

**Solution**: Describe images using a vision model ONCE, insert descriptions as text.

#### 2. **Context Window Explosion**
**Problem**: Base64 images consume massive amounts of context window.

**Math**:
```
1MB image (1920x1080 PNG)
→ Base64 encoding: ~1.33MB
→ As text: ~1,330,000 characters
→ Tokens (4 chars/token): ~332,500 tokens
→ With 128K context window: 260% over budget!
```

**Impact**:
- One image can exceed entire context window
- RAG system can't include conversation history
- API requests fail with "context_length_exceeded"

**Solution**: Intelligent resizing based on available context budget.

#### 3. **Format Incompatibility**
**Problem**: Some models don't support certain image formats.

**Examples**:
- WebP: Not supported by older vision models
- AVIF: Cutting edge, limited support
- HEIC: Apple format, limited support

**Solution**: Format conversion to safe fallbacks (PNG, JPEG).

#### 4. **No Cost Optimization**
**Problem**: Sending full-resolution images wastes money.

**Math**:
```
GPT-4V pricing (vision):
- Base: $0.01/1K tokens
- Image (1920x1080): ~330K tokens = $3.30 per message!
- Resized (512x288): ~20K tokens = $0.20 per message
- Savings: 94% cost reduction with minimal quality loss
```

**Solution**: Resize to minimum viable resolution for task.

#### 5. **RAG Budget Integration Missing**
**Problem**: Images don't participate in RAG's token budget system.

**Current RAG System (works for text only)**:
```typescript
const ragBudget = {
  total: 128000,           // Model's context window
  systemPrompt: 2000,      // Fixed
  messages: 100000,        // Flexible
  outputReserve: 4000      // Reserved for response
};
```

**What's Missing**: Images need to "flex" within available space, just like text messages.

---

## Architecture Overview

### The Three-Layer System

```
┌─────────────────────────────────────────────────────────┐
│ Layer 1: CAPTURE & STORAGE                             │
│ ─────────────────────────────────────────────────────── │
│ • chat/send receives media paths                        │
│ • file/load reads files as base64                       │
│ • file/mime-type detects format                         │
│ • ChatMessageEntity stores in database                  │
└─────────────────────────────────────────────────────────┘
                           ↓
┌─────────────────────────────────────────────────────────┐
│ Layer 2: MEDIA PROCESSING (NEW)                        │
│ ─────────────────────────────────────────────────────── │
│ • media/analyze - Vision model describes image          │
│ • media/convert - Format conversion (webp → png)        │
│ • media/resize - Intelligent resizing for context       │
│ • media/optimize - Compression without quality loss     │
│ • media/estimate-tokens - Calculate token cost          │
└─────────────────────────────────────────────────────────┘
                           ↓
┌─────────────────────────────────────────────────────────┐
│ Layer 3: ADAPTER FORMATTING (CURRENT)                  │
│ ─────────────────────────────────────────────────────── │
│ • Vision models: Format as ContentPart[]                │
│ • Non-vision models: Use text descriptions              │
│ • Provider-specific formatting (Anthropic vs OpenAI)    │
└─────────────────────────────────────────────────────────┘
```

---

## Media Processing Pipeline

### Phase 2: Image Description for Non-Vision Models

**Goal**: Non-vision models get semantic understanding via text descriptions.

**Flow**:
```typescript
// In BaseOpenAICompatibleAdapter.generateText()

if (!supportsVision && hasImages) {
  // Describe images ONCE using vision model
  for (const imagePart of imageParts) {
    const description = await Commands.execute('media/analyze', {
      base64: imagePart.image.base64,
      mimeType: imagePart.image.mimeType,
      prompt: 'Describe this image in detail for a text-only AI model',
      context: this.context,
      sessionId: this.sessionId
    });

    // Replace image with description
    textParts.push(`[Image: ${description.text}]`);
  }

  // Send text-only message with descriptions
  const content = textParts.join('\n');
}
```

**Caching Strategy**:
- Cache descriptions by image hash (SHA-256 of base64)
- Avoid re-describing same image across multiple messages
- Cache TTL: 24 hours (descriptions rarely change)

**Provider Selection for Description**:
1. Try Grok-vision (fast, cheap, excellent quality)
2. Fallback to GPT-4V (more expensive but very reliable)
3. Fallback to Claude Sonnet 4.5 (most expensive, best quality)

### Phase 3: Context-Aware Resizing

**Goal**: Images fit within available context budget without exceeding limits.

**Flow**:
```typescript
// In BaseOpenAICompatibleAdapter.generateText()

// 1. Calculate available context
const modelInfo = this.config.models?.find(m => m.id === model);
const contextWindow = modelInfo?.contextWindow || 128000;

// 2. Estimate current usage
const messagesTokens = this.estimateTokens(request.messages);
const systemPromptTokens = this.estimateTokens(request.systemPrompt);
const outputReserve = request.maxTokens || 4000;

// 3. Calculate available space for images
const availableForImages = contextWindow - messagesTokens - systemPromptTokens - outputReserve;

// 4. Resize each image to fit budget
for (const imagePart of imageParts) {
  const currentTokens = await Commands.execute('media/estimate-tokens', {
    base64: imagePart.image.base64,
    mimeType: imagePart.image.mimeType
  });

  if (currentTokens > availableForImages) {
    // Resize to fit
    const resized = await Commands.execute('media/resize', {
      base64: imagePart.image.base64,
      mimeType: imagePart.image.mimeType,
      maxTokens: availableForImages,
      preserveAspectRatio: true,
      quality: 85
    });

    imagePart.image.base64 = resized.base64;
    imagePart.image.mimeType = resized.mimeType;
  }
}
```

**Token Estimation Methods**:

Different models count image tokens differently:

1. **GPT-4V (Tile-Based)**:
   ```
   Tiles = ceil(width/512) × ceil(height/512)
   Tokens = (Tiles × 170) + 85 base

   Example (1920×1080):
   Tiles = 4 × 3 = 12
   Tokens = (12 × 170) + 85 = 2,125 tokens
   ```

2. **Claude (Base64 Length)**:
   ```
   Base64 chars = image bytes × 1.33
   Tokens ≈ Base64 chars / 4

   Example (1MB PNG):
   Base64 = 1,330,000 chars
   Tokens ≈ 332,500 tokens
   ```

3. **Grok (Similar to GPT-4V)**:
   ```
   Uses tile-based counting
   More efficient than Claude's approach
   ```

**Resizing Algorithm**:
```typescript
async function resizeToFitBudget(
  base64: string,
  mimeType: string,
  maxTokens: number,
  model: string
): Promise<{ base64: string, mimeType: string }> {
  // 1. Decode image dimensions
  const { width, height } = await getImageDimensions(base64);

  // 2. Calculate target dimensions
  let targetWidth = width;
  let targetHeight = height;

  while (estimateTokens(targetWidth, targetHeight, model) > maxTokens) {
    // Reduce by 20% each iteration
    targetWidth = Math.floor(targetWidth * 0.8);
    targetHeight = Math.floor(targetHeight * 0.8);
  }

  // 3. Resize with quality preservation
  return await Commands.execute('media/resize', {
    base64,
    mimeType,
    targetWidth,
    targetHeight,
    quality: 85,
    format: 'png'  // Safe fallback
  });
}
```

### Phase 4: Format Conversion

**Goal**: Convert unsupported formats to safe fallbacks.

**Supported Format Matrix**:

| Format | GPT-4V | Claude | Grok | Gemini | Notes |
|--------|--------|--------|------|--------|-------|
| PNG    | ✅     | ✅     | ✅   | ✅     | Universal support |
| JPEG   | ✅     | ✅     | ✅   | ✅     | Universal support |
| WebP   | ✅     | ❌     | ✅   | ✅     | Claude doesn't support |
| GIF    | ✅     | ❌     | ✅   | ✅     | Static only (no animation) |
| AVIF   | ❌     | ❌     | ❌   | ✅     | Too new |
| HEIC   | ❌     | ❌     | ❌   | ❌     | Apple proprietary |

**Conversion Strategy**:
```typescript
async function ensureCompatibleFormat(
  base64: string,
  mimeType: string,
  provider: string,
  model: string
): Promise<{ base64: string, mimeType: string }> {
  // Check if format is supported
  const supported = isFormatSupported(provider, model, mimeType);

  if (!supported) {
    // Convert to PNG (safest fallback)
    return await Commands.execute('media/convert', {
      base64,
      fromFormat: mimeType,
      toFormat: 'image/png',
      quality: 95  // High quality for conversion
    });
  }

  return { base64, mimeType };
}
```

---

## RAG Integration (CSS Flexbox Analogy)

### The Flexbox Mental Model

Think of RAG's context window like a CSS flexbox container:

```css
.context-window {
  display: flex;
  flex-direction: column;
  max-height: 128000px; /* tokens */
}

.system-prompt {
  flex: 0 0 2000px;  /* Fixed: 2000 tokens */
}

.messages {
  flex: 1 1 auto;    /* Flexible: Grows/shrinks */
}

.images {
  flex: 1 1 auto;    /* Flexible: Competes with messages */
  max-height: 50000px; /* Cap at 50K tokens */
}

.output-reserve {
  flex: 0 0 4000px;  /* Fixed: 4000 tokens */
}
```

### Current RAG Budget System

**Location**: `system/user/server/modules/rag-builders/ChatRAGBuilder.ts`

```typescript
interface RAGBudget {
  total: number;           // Context window
  systemPrompt: number;    // Fixed cost
  messages: number;        // Flexible (grows/shrinks)
  outputReserve: number;   // Fixed reserve
}

// Calculate message budget
const budget: RAGBudget = {
  total: modelContextWindow,
  systemPrompt: estimateTokens(systemPrompt),
  outputReserve: 4000,
  messages: 0  // Calculated below
};

budget.messages = budget.total - budget.systemPrompt - budget.outputReserve;
```

### Enhanced RAG Budget with Images

**New Structure**:
```typescript
interface EnhancedRAGBudget {
  total: number;           // Context window
  systemPrompt: number;    // Fixed cost
  messages: number;        // Flexible
  images: number;          // Flexible (NEW)
  outputReserve: number;   // Fixed reserve
}

// Dynamic allocation strategy
const budget: EnhancedRAGBudget = {
  total: modelContextWindow,
  systemPrompt: estimateTokens(systemPrompt),
  outputReserve: 4000,

  // Allocate remaining space between messages and images
  // Default split: 70% messages, 30% images
  messages: 0,  // Calculated
  images: 0     // Calculated
};

const available = budget.total - budget.systemPrompt - budget.outputReserve;
budget.messages = Math.floor(available * 0.7);
budget.images = Math.floor(available * 0.3);
```

### Dynamic Resizing (Flexbox Behavior)

**Scenario 1: No Images** (like `flex-shrink: 0` on messages)
```typescript
// Images don't exist, messages get full space
budget.messages = available;
budget.images = 0;
```

**Scenario 2: Small Images** (under budget)
```typescript
// Images fit comfortably, no resizing needed
const imageTokens = 5000;
if (imageTokens < budget.images) {
  // Keep full resolution
  // Messages get remaining space
  budget.messages = available - imageTokens;
}
```

**Scenario 3: Large Images** (over budget - resize!)
```typescript
// Images exceed budget, resize to fit (flex-shrink)
const imageTokens = 60000;  // Too large!
if (imageTokens > budget.images) {
  // Resize images to fit budget
  await resizeImagesTo(budget.images);

  // Messages get their allocated space
  budget.messages = Math.floor(available * 0.7);
}
```

**Scenario 4: Too Many Messages** (images get compressed)
```typescript
// Many messages in history, reduce image budget
const messageCount = 50;
if (messageCount > 30) {
  // Shift allocation: 85% messages, 15% images
  budget.messages = Math.floor(available * 0.85);
  budget.images = Math.floor(available * 0.15);

  // Resize images to fit smaller budget
  await resizeImagesTo(budget.images);
}
```

### Implementation in ChatRAGBuilder

**Location**: Modify `buildRAGContext()` method

```typescript
// In ChatRAGBuilder.buildRAGContext()

async buildRAGContext(params: {
  messages: ChatMessageEntity[],
  artifacts: Artifact[],
  systemPrompt: string,
  modelContextWindow: number
}): Promise<RAGContext> {
  // 1. Calculate base budget
  const budget = this.calculateBudget(params.modelContextWindow, params.systemPrompt);

  // 2. Detect images in artifacts
  const imageArtifacts = params.artifacts.filter(a => a.type === 'image');

  if (imageArtifacts.length > 0) {
    // 3. Estimate image token cost
    const imageTokens = await this.estimateImageTokens(imageArtifacts);

    // 4. Resize images if needed
    if (imageTokens > budget.images) {
      await this.resizeArtifactsToFitBudget(imageArtifacts, budget.images);
    }

    // 5. Adjust message budget (images took some space)
    const actualImageTokens = await this.estimateImageTokens(imageArtifacts);
    budget.messages = budget.total - budget.systemPrompt - budget.outputReserve - actualImageTokens;
  }

  // 6. Build message list within budget
  const messages = await this.selectMessagesWithinBudget(params.messages, budget.messages);

  return {
    messages,
    artifacts: imageArtifacts,
    systemPrompt: params.systemPrompt,
    budget
  };
}
```

---

## Implementation Phases

### ✅ Phase 1: Basic Format Handling (COMPLETE)
**Status**: Deployed 2025-11-26

- Vision capability detection per model
- Format flexibility (nested/flat base64)
- Non-vision models get plain text (images discarded)

### 🚧 Phase 2: Image Description (NEXT)
**Estimated**: 2-3 hours

**Tasks**:
1. Create `media/analyze` command
   - Use Grok-vision to describe images
   - Cache descriptions by image hash
   - Return text descriptions
2. Integrate into BaseOpenAICompatibleAdapter
   - Detect non-vision models with images
   - Call media/analyze for each image
   - Insert descriptions as `[Image: ...]` text
3. Test with non-vision models
   - Groq Lightning should understand image content
   - DeepSeek should get semantic information

**Success Criteria**:
- Non-vision models respond accurately to image content
- Descriptions cached (avoid redundant API calls)
- Cost under $0.01 per image description

### 🚧 Phase 3: Context-Aware Resizing
**Estimated**: 4-6 hours

**Tasks**:
1. Implement token estimation
   - GPT-4V tile-based calculation
   - Claude base64 length estimation
   - Grok tile-based calculation
2. Create `media/resize` command
   - Accept target token budget
   - Resize to fit within budget
   - Preserve aspect ratio
3. Create `media/estimate-tokens` command
   - Calculate token cost per model
   - Account for different counting methods
4. Integrate into BaseOpenAICompatibleAdapter
   - Calculate available context budget
   - Resize images before sending to API
   - Log token savings

**Success Criteria**:
- Images never exceed context window
- Automatic resizing maintains quality
- Token usage reduced by 80%+ for large images

### 🚧 Phase 4: Format Conversion
**Estimated**: 2-3 hours

**Tasks**:
1. Create `media/convert` command
   - Support webp → png
   - Support heic → jpeg
   - Support avif → png
2. Build format compatibility matrix
   - Per provider (anthropic, openai, xai, etc.)
   - Per model (gpt-4v, claude-3-opus, etc.)
3. Integrate into adapters
   - Auto-convert unsupported formats
   - Choose optimal output format
   - Log conversions

**Success Criteria**:
- All formats work with all providers
- Automatic conversion transparent to user
- No format-related API errors

### 🚧 Phase 5: RAG Budget Integration
**Estimated**: 6-8 hours

**Tasks**:
1. Extend RAGBudget interface
   - Add `images` field
   - Add allocation strategy
2. Implement flexbox-style allocation
   - Dynamic split between messages/images
   - Adapt based on content
3. Integrate image resizing into RAG builder
   - Resize artifacts to fit budget
   - Adjust message inclusion based on space
4. Add budget monitoring
   - Log actual vs allocated tokens
   - Warn when approaching limits

**Success Criteria**:
- Images and messages coexist within budget
- Dynamic allocation prevents overflow
- RAG builder never exceeds context window
- Budget utilization >90% (efficient use of space)

---

## Command Specifications

### `media/analyze` (Phase 2)

**Purpose**: Describe images using vision models for semantic understanding.

**Parameters**:
```typescript
interface MediaAnalyzeParams extends CommandParams {
  base64: string;           // Base64-encoded image
  mimeType: string;         // MIME type (image/png, etc.)
  prompt?: string;          // Custom prompt (optional)
  detail?: 'low' | 'high';  // Detail level (default: high)
  provider?: string;        // Force specific provider (optional)
}
```

**Response**:
```typescript
interface MediaAnalyzeResult extends CommandResult {
  text: string;                    // Image description
  provider: string;                // Provider used (grok, openai, anthropic)
  model: string;                   // Model used (grok-vision-4, etc.)
  tokensUsed: number;              // Tokens consumed
  estimatedCost: number;           // Cost in USD
  cached: boolean;                 // Was description cached?
  cacheKey: string;                // SHA-256 hash for caching
}
```

**Implementation**:
```typescript
// commands/media/analyze/server/MediaAnalyzeServerCommand.ts

async execute(params: MediaAnalyzeParams): Promise<MediaAnalyzeResult> {
  // 1. Generate cache key (SHA-256 of base64)
  const cacheKey = this.generateCacheKey(params.base64);

  // 2. Check cache
  const cached = await this.checkCache(cacheKey);
  if (cached) {
    return { ...cached, cached: true };
  }

  // 3. Select provider (Grok → GPT-4V → Claude)
  const provider = params.provider || await this.selectBestVisionProvider();

  // 4. Generate description
  const prompt = params.prompt ||
    'Describe this image in detail, focusing on key visual elements, text content, and overall context. Be concise but thorough.';

  const result = await Commands.execute('ai/generate', {
    provider,
    messages: [{
      role: 'user',
      content: [
        { type: 'text', text: prompt },
        { type: 'image', image: { base64: params.base64, mimeType: params.mimeType }}
      ]
    }],
    maxTokens: 500
  });

  // 5. Cache result
  await this.cacheDescription(cacheKey, result.text, provider, result.model);

  return {
    success: true,
    text: result.text,
    provider,
    model: result.model,
    tokensUsed: result.usage.totalTokens,
    estimatedCost: result.usage.estimatedCost,
    cached: false,
    cacheKey
  };
}
```

### `media/resize` (Phase 3)

**Purpose**: Resize images to fit within token budget.

**Parameters**:
```typescript
interface MediaResizeParams extends CommandParams {
  base64: string;                  // Base64-encoded image
  mimeType: string;                // Input MIME type

  // Resize strategy (one required)
  maxTokens?: number;              // Target token budget
  targetWidth?: number;            // Target width in pixels
  targetHeight?: number;           // Target height in pixels
  scale?: number;                  // Scale factor (0.5 = 50%)

  // Options
  preserveAspectRatio?: boolean;   // Default: true
  quality?: number;                // JPEG quality 1-100 (default: 85)
  format?: string;                 // Output format (default: input format)
  model?: string;                  // Model for token estimation
}
```

**Response**:
```typescript
interface MediaResizeResult extends CommandResult {
  base64: string;                  // Resized image (base64)
  mimeType: string;                // Output MIME type
  originalDimensions: {
    width: number;
    height: number;
  };
  newDimensions: {
    width: number;
    height: number;
  };
  originalTokens: number;          // Before resize
  newTokens: number;               // After resize
  reductionPercent: number;        // Token reduction %
  originalSize: number;            // Bytes before
  newSize: number;                 // Bytes after
}
```

### `media/estimate-tokens` (Phase 3)

**Purpose**: Estimate token cost for images per model.

**Parameters**:
```typescript
interface MediaEstimateTokensParams extends CommandParams {
  base64: string;                  // Base64-encoded image
  mimeType: string;                // MIME type
  model: string;                   // Model for estimation
  provider: string;                // Provider (openai, anthropic, xai)
}
```

**Response**:
```typescript
interface MediaEstimateTokensResult extends CommandResult {
  tokens: number;                  // Estimated tokens
  method: 'tile-based' | 'base64-length' | 'pixels';
  details: {
    width: number;
    height: number;
    tiles?: number;                // For tile-based models
    base64Length?: number;         // For base64-based models
  };
}
```

### `media/convert` (Phase 4)

**Purpose**: Convert between image formats.

**Parameters**:
```typescript
interface MediaConvertParams extends CommandParams {
  base64: string;                  // Base64-encoded image
  fromFormat: string;              // Input MIME type
  toFormat: string;                // Output MIME type
  quality?: number;                // Compression quality (default: 95)
}
```

**Response**:
```typescript
interface MediaConvertResult extends CommandResult {
  base64: string;                  // Converted image
  mimeType: string;                // Output MIME type
  originalSize: number;            // Bytes before
  newSize: number;                 // Bytes after
}
```

---

## Testing Strategy

### Unit Tests
```bash
# Phase 2: Image Description
npx vitest tests/unit/media-analyze.test.ts

# Phase 3: Resizing
npx vitest tests/unit/media-resize.test.ts
npx vitest tests/unit/media-estimate-tokens.test.ts

# Phase 4: Format Conversion
npx vitest tests/unit/media-convert.test.ts
```

### Integration Tests
```bash
# Test with real models
./jtag collaboration/chat/send --room="general" --message="Describe this" \
  --media="/path/to/test-image.png"

# Wait for responses
sleep 10

# Check responses
./jtag collaboration/chat/export --room="general" --limit=20
```

### Performance Tests
```typescript
// Test token reduction
const before = await estimateTokens(originalImage);
const after = await estimateTokens(resizedImage);
expect(after).toBeLessThan(before * 0.2);  // 80% reduction

// Test cost savings
const costBefore = calculateCost(before);
const costAfter = calculateCost(after);
expect(costAfter).toBeLessThan(costBefore * 0.2);  // 80% savings
```

---

## Success Metrics

### Phase 2: Image Description
- ✅ Non-vision models respond accurately to image content
- ✅ Description cache hit rate >50% after 1 hour
- ✅ Average description cost <$0.01 per image

### Phase 3: Context-Aware Resizing
- ✅ Zero context window exceeded errors
- ✅ Token usage reduced 80%+ for large images
- ✅ Image quality remains visually acceptable
- ✅ Resizing adds <100ms latency per image

### Phase 4: Format Conversion
- ✅ Zero format compatibility errors
- ✅ All formats work with all providers
- ✅ Conversion adds <200ms latency per image

### Phase 5: RAG Integration
- ✅ Context budget never exceeded
- ✅ Budget utilization >90% (efficient space use)
- ✅ Images and messages coexist gracefully
- ✅ Flexbox-style allocation adapts to content

---

## Open Questions

1. **Image Description Caching**: Where to store cache?
   - Option A: In-memory (lost on restart)
   - Option B: Database (persistent, slower)
   - Option C: Redis (fast + persistent)
   - **Decision**: TBD

2. **Vision Provider Selection**: Auto-select or user choice?
   - Grok-vision: Fast, cheap, good quality
   - GPT-4V: Expensive, very reliable
   - Claude Sonnet 4.5: Most expensive, best quality
   - **Decision**: Auto-select with fallback chain

3. **Token Estimation Accuracy**: How to improve?
   - Current: Approximations based on documentation
   - Better: Calibrate against actual API responses
   - Best: Provider APIs expose token counting
   - **Decision**: Start with approximations, calibrate over time

4. **Base64 in Text Content**: Worth trying?
   - Some models might understand data URIs in text
   - Could be simpler than multimodal format
   - Needs testing with each provider
   - **Decision**: Test in Phase 2, low priority

---

## Related Documentation

- `CLAUDE.md` - Main development guide
- `docs/UNIVERSAL-PRIMITIVES.md` - Commands.execute() architecture
- `system/user/server/modules/rag-builders/ChatRAGBuilder.ts` - RAG budget system
- `daemons/ai-provider-daemon/shared/adapters/BaseOpenAICompatibleAdapter.ts` - Adapter implementation
- `commands/media/resize/` - Media resize command (to be created)

---

## Changelog

**2025-11-26**: Initial document created
- Documented Phase 1 implementation (complete)
- Designed Phases 2-5 architecture
- Specified command interfaces
- Outlined RAG integration strategy
