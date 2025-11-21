# Multimodal Architecture for PersonaUser Cognition

## Philosophy: Adapter-Based Multimodal Processing

**Core Principle**: Models have different capabilities. Adapters transform inputs to match model capabilities.

- **Smart models** (Grok, DeepSeek, Claude 3, GPT-4V, llama3.2-vision): Get ACTUAL images/audio/video via native API
- **Dumb models** (llama3.2:1b, phi3:mini, mistral:7b): Need media pre-processed into text descriptions

**Key Insight**:
1. Smart models ALWAYS get raw media (no compromise!)
2. Dumb models get pre-processed text descriptions (cached for efficiency)
3. Analysis tools (segmentation, ImageNet, bounding boxes) are OPTIONAL tools that ANY AI can invoke if they want additional processing

## The Efficiency Problem

### ❌ Without Shared Pipeline (Inefficient)

```typescript
// Each AI independently processes the same image
PersonaUser1 (llama3.2:1b) → calls vision model → "A web form..."
PersonaUser2 (phi3:mini)    → calls vision model → "A web form..." (DUPLICATE!)
PersonaUser3 (mistral:7b)   → calls vision model → "A web form..." (DUPLICATE!)

// 10 AIs responding = 10 vision inferences (~5 seconds total)
```

### ✅ With Shared Pipeline (Efficient)

```typescript
// Process once, all AIs reuse results
Image uploaded → Media Analysis Command → Cache
  ↓
  ├─ Vision description: "A web form with misaligned buttons..."
  ├─ OCR text: "Submit" "Cancel" "Email: ___"
  ├─ Object detection: [button: (100,200), text: (50,100)]
  ├─ Classification: "UI/screenshot" confidence=0.95
  ├─ Segmentation: [background, form, buttons]
  ↓
All dumb models read from cache → instant, no duplicate work

// 10 AIs responding = 1 vision inference + 10 text inferences (~2.5 seconds total)
// 50% faster!
```

## Architecture

### 0. Tool Execution with Media (NEW: ToolRegistry → PersonaUser → AI Adapter)

**Problem**: When an AI runs a tool (like `screenshot`), the result needs to flow back into their cognition with full media access - not just text.

**Design Decision**: Structured tool results preserve MediaItem objects through the entire chain.

```typescript
// 1. Command returns MediaItem
interface ScreenshotResult extends CommandResult {
  success: boolean;
  filename: string;
  media?: MediaItem;  // ← Structured media preserved
}

// 2. ToolRegistry preserves structure
interface ToolExecutionResult {
  toolName: string;
  success: boolean;
  content?: string;      // Human-readable text
  media?: MediaItem[];   // ← Structured media array
  error?: string;
}

// 3. PersonaToolExecutor passes through
interface ToolResult {
  toolName: string;
  success: boolean;
  content?: string;
  media?: MediaItem[];   // ← Still structured
  error?: string;
}

// 4. AI Adapter decides how to present
// Smart models: Include raw image in next message
if (toolResult.media && this.supportsVision) {
  messages.push({
    role: 'user',
    content: [
      { type: 'text', text: toolResult.content },
      ...toolResult.media.map(m => ({ type: 'image', image: m }))
    ]
  });
}

// Dumb models: Just get text description (or optionally analyze)
messages.push({
  role: 'user',
  content: toolResult.content  // Text only
});
```

**Benefits**:
- **Type-safe**: MediaItem is typed, not JSON string
- **No parse overhead**: Structured data flows directly
- **Extensible**: Works for any command returning media (screenshot, file/read, web/fetch)
- **Adapter autonomy**: Each adapter decides how to present media to its model

**Universal Pattern**: ANY command that returns files/media should include MediaItem in result, not just screenshots.

### 0.1. Opt-in Media Loading (Avoiding Forced Image Loading)

**Problem**: When an AI runs `screenshot` in a busy room with 19+ active PersonaUsers, we don't want ALL 19 AIs to automatically load the image into their context - that's wasteful and slow.

**Design Decision**: Media loading is OPT-IN, not automatic. Each PersonaUser configures whether they want to receive media.

```typescript
// system/user/server/PersonaUser.ts - Configuration

interface PersonaMediaConfig {
  autoLoadMedia: boolean;          // Default: false (opt-in)
  requestMediaByDefault: boolean;  // For specialized AIs (CSS Designer): true
  supportedMediaTypes: MediaType[]; // ['image', 'video', 'audio', 'file']
}

// Example configurations:
const cssDesignerAI: PersonaMediaConfig = {
  autoLoadMedia: true,              // ✅ Always receive images
  requestMediaByDefault: true,
  supportedMediaTypes: ['image']    // Screenshots for visual feedback
};

const generalAI: PersonaMediaConfig = {
  autoLoadMedia: false,             // ❌ Don't load images by default
  requestMediaByDefault: false,
  supportedMediaTypes: ['image', 'audio']
};
```

**Tool Execution Context**:

```typescript
// system/user/server/modules/PersonaToolExecutor.ts

interface ToolExecutionContext {
  personaId: UUID;
  personaName: string;
  contextId: UUID;
  personaConfig: PersonaMediaConfig;  // ← Configuration drives behavior
}

async executeToolCalls(
  toolCalls: ToolCall[],
  context: ToolExecutionContext
): Promise<{
  formattedResults: string;   // XML text for injection
  media?: MediaItem[];         // Optional media (only if configured)
}> {
  const results: string[] = [];
  const allMedia: MediaItem[] = [];

  for (const toolCall of toolCalls) {
    const result = await this.toolRegistry.executeTool(
      toolCall.toolName,
      toolCall.parameters,
      context.contextId
    );

    // Check if THIS persona wants media
    if (result.media && context.personaConfig.autoLoadMedia) {
      // Filter by supported types
      const supportedMedia = result.media.filter(m =>
        context.personaConfig.supportedMediaTypes.includes(m.type)
      );
      allMedia.push(...supportedMedia);
    }

    // Always include text description (for non-vision AIs)
    results.push(this.formatToolResult(result));
  }

  return {
    formattedResults: results.join('\n\n'),
    media: allMedia.length > 0 ? allMedia : undefined
  };
}
```

**PersonaResponseGenerator Integration**:

```typescript
// system/user/server/modules/PersonaResponseGenerator.ts

async generateResponse(
  triggerMessage: ChatMessageEntity,
  ragContext: RAGContext
): Promise<string> {
  // 1. Parse tool calls from previous response (if any)
  const toolCalls = this.toolExecutor.parseToolCalls(this.lastResponse || '');

  if (toolCalls.length > 0) {
    // 2. Execute tools with persona's media config
    const toolContext: ToolExecutionContext = {
      personaId: this.personaId,
      personaName: this.personaName,
      contextId: triggerMessage.roomId,
      personaConfig: this.personaUser.mediaConfig  // ← From PersonaUser config
    };

    const { formattedResults, media } = await this.toolExecutor.executeToolCalls(
      toolCalls,
      toolContext
    );

    // 3. Build next inference request
    const messages = this.buildMessages(ragContext);

    // 4. Add tool results as user message
    if (media && media.length > 0) {
      // ✅ VISION-CAPABLE AI: Include images in multimodal message
      messages.push({
        role: 'user',
        content: [
          { type: 'text', text: formattedResults },
          ...media.map(m => ({
            type: m.type as 'image' | 'audio' | 'video',
            [m.type]: m
          }))
        ]
      });
    } else {
      // ❌ TEXT-ONLY AI: Just include text description
      messages.push({
        role: 'user',
        content: formattedResults
      });
    }

    // 5. Generate next response with tool results injected
    const response = await this.aiAdapter.generateText({
      messages,
      model: this.personaUser.modelConfig.model,
      maxTokens: 2000
    });

    return response.text;
  }

  // ... normal response generation without tools
}
```

**Benefits**:

1. **Efficiency**: Only AIs that NEED images load them (CSS Designer: yes, general chat: no)
2. **Scalability**: 19 AIs in a room, only 1-2 load the screenshot
3. **Flexibility**: Per-AI configuration allows specialized behaviors
4. **Training-friendly**: CSS Designer AIs can be trained with visual feedback naturally
5. **Native + XML convergence**: Both tool calling paradigms work the same way

**CSS Designer AI Use Case**:

```typescript
// Example: CSS Designer AI configured with autoLoadMedia: true

// 1. User: "Make the chat widget wider"
// 2. CSS Designer AI: "Let me check the current state"
//    → Runs: <tool_use><tool_name>screenshot</tool_name>...</tool_use>
// 3. ToolExecutor: Executes screenshot, returns MediaItem
// 4. PersonaResponseGenerator: Checks mediaConfig.autoLoadMedia = true
// 5. Next inference includes ACTUAL screenshot image
// 6. CSS Designer AI: "I can see the widget is 400px wide. Let me adjust..."
//    → Runs: <tool_use><tool_name>debug/widget-css</tool_name>...</tool_use>
// 7. Takes another screenshot, sees result, iterates

// Result: Visual feedback loop for CSS design
```

**Native Function Calling vs XML Tool Calling**:

Both paradigms converge at the adapter layer:

```typescript
// NATIVE FUNCTION CALLING (OpenAI, Anthropic, Mistral)
// =====================================================

// AI adapter receives function call result:
{
  tool_call_id: "call_abc123",
  function: "screenshot",
  result: {
    success: true,
    filename: "screenshot.png",
    media: { type: 'image', base64: '...' }  // ← Structured
  }
}

// Adapter formats for next inference:
messages.push({
  role: 'tool',
  tool_call_id: "call_abc123",
  content: [
    { type: 'text', text: "Screenshot saved to screenshot.png" },
    { type: 'image', image: result.media }  // ← If autoLoadMedia: true
  ]
});

// XML TOOL CALLING (Universal fallback)
// ======================================

// AI adapter receives XML result:
<tool_result>
<tool_name>screenshot</tool_name>
<status>success</status>
<content>Screenshot saved to screenshot.png</content>
</tool_result>

// Adapter formats for next inference:
messages.push({
  role: 'user',
  content: [
    { type: 'text', text: xmlFormattedResult },
    { type: 'image', image: result.media }  // ← If autoLoadMedia: true
  ]
});
```

**Both paradigms check the same config flag** and inject media the same way - they just differ in result formatting.

### 1. Chat Message Media Structure (ChatMessageEntity)

```typescript
// system/data/entities/ChatMessageEntity.ts

export type MediaType = 'image' | 'audio' | 'video' | 'file' | 'document';

export interface MediaItem {
  // Core
  id?: string;
  type: MediaType;

  // Content (at least one required)
  url?: string;         // file:// or https://
  base64?: string;      // Base64 data

  // Metadata
  mimeType?: string;
  filename?: string;
  size?: number;

  // Accessibility
  alt?: string;         // Alt text for screen readers
  description?: string; // AI-generated or human description
  title?: string;

  // Dimensions
  width?: number;
  height?: number;
  duration?: number;    // For audio/video

  // Processing
  analysisCacheKey?: string;  // Link to ai/analyze-media result
  thumbnailUrl?: string;

  // Tracking
  uploadedAt?: number;
  uploadedBy?: UUID;
}

export interface MessageContent {
  text: string;
  media?: readonly MediaItem[];
}
```

### 2. Universal Media Types (AIProviderTypesV2)

```typescript
// daemons/ai-provider-daemon/shared/AIProviderTypesV2.ts

export type ContentPart =
  | { type: 'text'; text: string }
  | { type: 'image'; image: ImageInput }
  | { type: 'audio'; audio: AudioInput }
  | { type: 'video'; video: VideoInput };

export interface ChatMessage {
  role: 'system' | 'user' | 'assistant';
  content: string | ContentPart[];  // ← Universal format
}
```

### 2. Media Analysis Command (NEW)

```typescript
// commands/ai/analyze-media/shared/AnalyzeMediaTypes.ts

export type MediaAnalysisType =
  | 'vision-description'    // LLM describes what it sees
  | 'ocr'                   // Extract text (Tesseract)
  | 'object-detection'      // Detect objects/UI elements (YOLO)
  | 'image-classification'  // Classify image type (CNN)
  | 'segmentation'          // Segment regions (SAM, U-Net)
  | 'face-detection'        // Detect faces
  | 'audio-transcription'   // Transcribe audio (Whisper)
  | 'video-frames'          // Extract key frames (ffmpeg)
  | 'embedding';            // Generate embedding

export interface MediaAnalysisRequest {
  media: {
    type: 'image' | 'video' | 'audio';
    url?: string;
    base64?: string;
  };
  analyses: MediaAnalysisType[];
  cacheKey?: string;
}

export interface MediaAnalysisResult {
  cacheKey: string;
  analyses: {
    'vision-description'?: { description: string; model: string };
    'ocr'?: { text: string; words: Array<{...}> };
    'object-detection'?: { objects: Array<{...}> };
    // ... etc
  };
  fromCache: boolean;
  processingTime: number;
}
```

**Usage:**

```bash
./jtag ai/analyze-media --image="/path/to/screenshot.png" \
  --analyses='["vision-description","ocr","object-detection"]'

# Result cached - subsequent calls instant!
```

### 3. Adapter Integration

**CRITICAL**: Each adapter must route correctly based on model capability:

```typescript
// daemons/ai-provider-daemon/adapters/ollama/shared/OllamaAdapter.ts
async generateText(request: TextGenerationRequest): Promise<TextGenerationResponse> {
  const hasMultimodal = request.messages.some(msg => typeof msg.content !== 'string');

  if (!hasMultimodal) {
    return this.generateTextOnly(request);  // Fast path
  }

  const isVisionModel = this.isVisionCapable(request.model);

  if (isVisionModel) {
    // ✅ SMART MODEL: Pass raw images via /api/chat endpoint
    return this.generateTextWithVision(request);
  } else {
    // ❌ DUMB MODEL: Pre-process images to text
    return this.generateTextWithMediaAnalysis(request);
  }
}

private isVisionCapable(model: string): boolean {
  return model.includes('vision') ||
         model.includes('llava') ||
         model.includes('bakllava');
}
```

```typescript
// daemons/ai-provider-daemon/adapters/anthropic/shared/AnthropicAdapter.ts
async generateText(request: TextGenerationRequest): Promise<TextGenerationResponse> {
  // ✅ ALL Claude models (Opus, Sonnet, Haiku) support vision natively
  // ALWAYS pass raw images - never pre-process

  const anthropicMessages = request.messages.map(msg => {
    if (typeof msg.content === 'string') {
      return { role: msg.role, content: msg.content };
    }

    // Transform ContentPart[] to Anthropic's format
    const content = msg.content.map(part => {
      if (part.type === 'text') {
        return { type: 'text', text: part.text };
      } else if (part.type === 'image') {
        // ✅ Pass raw image
        return {
          type: 'image',
          source: {
            type: 'base64',
            media_type: part.image.mimeType || 'image/png',
            data: part.image.base64
          }
        };
      }
    });

    return { role: msg.role, content };
  });

  // Call Anthropic API with native multimodal support
  const response = await this.anthropicClient.messages.create({
    model: request.model,
    messages: anthropicMessages,
    max_tokens: request.maxTokens
  });

  return this.parseResponse(response);
}
```

```typescript
// daemons/ai-provider-daemon/adapters/xai/shared/XAIAdapter.ts (Grok)
async generateText(request: TextGenerationRequest): Promise<TextGenerationResponse> {
  // ✅ Grok supports vision natively (grok-2-vision-1212, grok-vision-beta)
  // ALWAYS pass raw images

  const xaiMessages = request.messages.map(msg => {
    if (typeof msg.content === 'string') {
      return { role: msg.role, content: msg.content };
    }

    // Transform to XAI format (OpenAI-compatible)
    const content = msg.content.map(part => {
      if (part.type === 'text') {
        return { type: 'text', text: part.text };
      } else if (part.type === 'image') {
        // ✅ Pass raw image
        return {
          type: 'image_url',
          image_url: {
            url: part.image.url || `data:image/png;base64,${part.image.base64}`
          }
        };
      }
    });

    return { role: msg.role, content };
  });

  // Call XAI API
  const response = await fetch('https://api.x.ai/v1/chat/completions', {
    method: 'POST',
    headers: { 'Authorization': `Bearer ${this.apiKey}` },
    body: JSON.stringify({
      model: request.model,
      messages: xaiMessages
    })
  });

  return this.parseResponse(response);
}
```

```typescript
// daemons/ai-provider-daemon/adapters/deepseek/shared/DeepSeekAdapter.ts
async generateText(request: TextGenerationRequest): Promise<TextGenerationResponse> {
  // ✅ DeepSeek supports vision (deepseek-chat, deepseek-reasoner)
  // ALWAYS pass raw images

  const deepseekMessages = request.messages.map(msg => {
    if (typeof msg.content === 'string') {
      return { role: msg.role, content: msg.content };
    }

    // Transform to DeepSeek format (similar to OpenAI)
    const content = msg.content.map(part => {
      if (part.type === 'text') {
        return { type: 'text', text: part.text };
      } else if (part.type === 'image') {
        // ✅ Pass raw image
        return {
          type: 'image_url',
          image_url: { url: `data:image/png;base64,${part.image.base64}` }
        };
      }
    });

    return { role: msg.role, content };
  });

  // Call DeepSeek API
  const response = await fetch('https://api.deepseek.com/v1/chat/completions', {
    method: 'POST',
    headers: { 'Authorization': `Bearer ${this.apiKey}` },
    body: JSON.stringify({
      model: request.model,
      messages: deepseekMessages
    })
  });

  return this.parseResponse(response);
}
```

private async generateTextWithMediaAnalysis(request: TextGenerationRequest) {
  // Transform multimodal messages to text-only using cached analysis
  const processedMessages = await Promise.all(
    request.messages.map(async (msg) => {
      if (typeof msg.content === 'string') return msg;

      const textParts: string[] = [];

      for (const part of msg.content) {
        if (part.type === 'text') {
          textParts.push(part.text);
        } else if (part.type === 'image') {
          // Get cached analysis (or generate if not cached)
          const analysis = await Commands.execute('ai/analyze-media', {
            media: { type: 'image', ...part.image },
            analyses: ['vision-description', 'ocr', 'object-detection']
          });

          // Format as text
          const desc = analysis.analyses['vision-description'];
          const ocr = analysis.analyses['ocr'];
          const objects = analysis.analyses['object-detection'];

          textParts.push(`
[Image Analysis]
Description: ${desc.description}
Text in image: "${ocr.text}"
Elements: ${objects.objects.map(o => o.label).join(', ')}
          `);
        }
      }

      return { ...msg, content: textParts.join('\n') };
    })
  );

  // Generate with text-only messages
  return this.generateTextOnly({ ...request, messages: processedMessages });
}
```

## Processing Pipeline Components

### Vision Description (LLM-based)

```typescript
// Uses local vision model (llama3.2-vision, llava)
const result = await AIProviderDaemon.generateText({
  messages: [{
    role: 'user',
    content: [
      { type: 'text', text: 'Describe this image in detail.' },
      { type: 'image', image }
    ]
  }],
  model: 'llama3.2-vision:11b',
  preferredProvider: 'ollama'
});
```

### OCR (Tesseract)

```bash
# Via existing media/ocr command or direct Tesseract call
tesseract image.png stdout
```

### Object Detection (YOLO)

```python
# system/ai/algorithms/object-detection.py
from ultralytics import YOLO

model = YOLO('yolov8n.pt')
results = model(image_path)

objects = [{
  'label': model.names[int(box.cls)],
  'confidence': float(box.conf),
  'bbox': {...}
} for box in results[0].boxes]

print(json.dumps(objects))
```

### Image Classification (CNN)

```python
# system/ai/algorithms/image-classification.py
from transformers import AutoImageProcessor, AutoModelForImageClassification

processor = AutoImageProcessor.from_pretrained("microsoft/resnet-50")
model = AutoModelForImageClassification.from_pretrained("microsoft/resnet-50")

inputs = processor(image, return_tensors="pt")
outputs = model(**inputs)
predicted_class = outputs.logits.argmax(-1).item()

print(json.dumps({
  'class': model.config.id2label[predicted_class],
  'confidence': float(outputs.logits.softmax(-1).max())
}))
```

### Segmentation (SAM)

```python
# system/ai/algorithms/segmentation.py
from segment_anything import sam_model_registry, SamPredictor

sam = sam_model_registry["vit_b"](checkpoint="sam_vit_b.pth")
predictor = SamPredictor(sam)
predictor.set_image(image)

masks = predictor.predict(...)

segments = [{
  'label': 'unknown',
  'mask': base64.b64encode(mask).decode(),
  'area': int(np.sum(mask))
} for mask in masks]

print(json.dumps(segments))
```

### Audio Transcription (Whisper)

```typescript
// Via Ollama or Whisper.cpp
const result = await AIProviderDaemon.transcribeAudio({
  audio: audioInput,
  model: 'whisper-base',
  preferredProvider: 'ollama'
});
```

### Video Frame Extraction (ffmpeg)

```bash
# Via existing media/extract-frames command
ffmpeg -i video.mp4 -vf fps=1 frame-%04d.png
```

## Caching Strategy

```typescript
// commands/ai/analyze-media/server/MediaAnalysisCache.ts

class MediaAnalysisCache {
  private cache = new Map<string, MediaAnalysisResult>();

  get(cacheKey: string): MediaAnalysisResult | undefined {
    const cached = this.cache.get(cacheKey);
    if (cached && Date.now() - cached.timestamp < 3600000) {  // 1 hour
      return cached;
    }
    return undefined;
  }

  set(cacheKey: string, result: MediaAnalysisResult): void {
    this.cache.set(cacheKey, result);
  }

  generateKey(media: MediaInput): string {
    const data = media.base64 || media.url || '';
    return crypto.createHash('sha256').update(data).digest('hex').slice(0, 16);
  }
}
```

## Integration with PersonaUser Cognition

```typescript
// system/ai/server/AIDecisionService.ts

export interface AIDecisionContext {
  personaId: UUID;
  personaName: string;
  roomId: UUID;
  triggerMessage: ChatMessageEntity;
  ragContext: RAGContext;
  systemPrompt?: string;
  visualContext?: VisualContext;  // ← NEW
}

interface VisualContext {
  screenshots: Array<{
    url?: string;
    base64?: string;
    caption?: string;
    timestamp: number;
  }>;
  audioClips?: Array<{...}>;
  videoClips?: Array<{...}>;
}

// Modified: buildResponseMessages() includes visual context
private static buildResponseMessages(context: AIDecisionContext): ChatMessage[] {
  const messages: ChatMessage[] = [];

  // System prompt + conversation history
  // ...

  // Visual context (if provided)
  if (context.visualContext?.screenshots.length) {
    const visualContent: ContentPart[] = [
      { type: 'text', text: 'Current visual context:' }
    ];

    for (const screenshot of context.visualContext.screenshots) {
      visualContent.push({
        type: 'image',
        image: {
          url: screenshot.url,
          base64: screenshot.base64
        }
      });

      if (screenshot.caption) {
        visualContent.push({ type: 'text', text: screenshot.caption });
      }
    }

    messages.push({ role: 'user', content: visualContent });
  }

  return messages;
}
```

## Complete Flow Example

```typescript
// User uploads screenshot in chat
./jtag chat/send --room="general" \
  --message="What's wrong with this UI?" \
  --image="/path/to/screenshot.png"

// 1. ChatMessageEntity stores image
{
  content: {
    text: "What's wrong with this UI?",
    attachments: [{ type: 'image', url: 'file://...' }]
  }
}

// 2. RoomEventDaemon broadcasts to all PersonaUsers in room

// SMART MODEL PATH (Grok, DeepSeek, Claude)
// =========================================

// 3a. Grok (grok-vision-beta) receives event
messages = [{
  role: 'user',
  content: [
    { type: 'text', text: "What's wrong with this UI?" },
    { type: 'image', image: { base64: '...' } }  // ✅ RAW IMAGE
  ]
}]

// 4a. XAIAdapter passes raw image to Grok API
const response = await xaiAPI.chat.completions.create({
  model: 'grok-vision-beta',
  messages: [{
    role: 'user',
    content: [
      { type: 'text', text: "What's wrong with this UI?" },
      { type: 'image_url', image_url: { url: 'data:image/png;base64,...' } }  // ✅ NATIVE
    ]
  }]
});

// 5a. Grok sees ACTUAL image, provides detailed visual analysis
// "I can see the form has several issues: the Submit button at (250,400)
// overlaps with the Email input field at (200,390). The Cancel button
// text is clipped..."

// DUMB MODEL PATH (llama3.2:1b, phi3)
// ====================================

// 3b. PersonaUser (llama3.2:1b - no vision) receives event
messages = [{
  role: 'user',
  content: [
    { type: 'text', text: "What's wrong with this UI?" },
    { type: 'image', image: { url: 'file://...' } }
  ]
}]

// 4b. OllamaAdapter detects dumb model, calls ai/analyze-media
analysis = await Commands.execute('ai/analyze-media', {
  media: { type: 'image', url: 'file://...' },
  analyses: ['vision-description', 'ocr', 'object-detection']
});
// ✅ Result cached - only runs ONCE for all dumb models

// 5b. OllamaAdapter transforms to text-only message
transformedMessage = {
  role: 'user',
  content: `What's wrong with this UI?

[Image Analysis]
Description: A web form with misaligned buttons and overlapping text
Text in image: "Submit" "Cancel" "Email: ___"
Elements: button (250,400,50,30), textbox (200,390,200,25), button (300,400,50,30)
`
}

// 6b. Dumb model generates response based on text description
response = "Based on the analysis, the UI has alignment issues with overlapping elements..."

// OPTIONAL: AI can invoke tools for deeper analysis
// ==================================================

// 7. Any AI (smart or dumb) can optionally call analysis tools:
./jtag ai/analyze-media --image="<image-id>" \
  --analyses='["segmentation","object-detection"]'

// Example: Grok might call semantic-segmentation tool to get precise masks
// Example: Claude might call ImageNet classification to identify UI components
// These are OPTIONAL enhancements beyond native vision
```

## Benefits

1. **No compromise**: Smart models (Grok, DeepSeek, Claude, GPT-4V) get ACTUAL raw images via native APIs
2. **Massive efficiency**: Dumb models share cached pre-processed analysis (no duplicate vision inference)
3. **Optional enhancement**: ANY AI can invoke specialized tools (segmentation, ImageNet, bounding boxes) as needed
4. **Universal compatibility**: Every model (dumb or smart) gets multimodal capability
5. **Pluggable**: Easy to add new analysis types
6. **Cacheable**: Analysis results persist across sessions
7. **Leverages existing infrastructure**: ffmpeg, Tesseract, Python ML libraries

## Vision-Capable Model Detection

```typescript
// daemons/ai-provider-daemon/shared/ModelCapabilityDetector.ts (NEW)

export function isVisionCapable(model: string, provider: string): boolean {
  // Anthropic: ALL Claude 3+ models support vision
  if (provider === 'anthropic') {
    return model.includes('claude-3') ||
           model.includes('claude-opus') ||
           model.includes('claude-sonnet');
  }

  // XAI: Grok vision models
  if (provider === 'xai') {
    return model.includes('grok-vision') ||
           model.includes('grok-2-vision');
  }

  // DeepSeek: Vision-capable models
  if (provider === 'deepseek') {
    return model.includes('deepseek-chat') ||
           model.includes('deepseek-reasoner');
  }

  // OpenAI: GPT-4 Vision models
  if (provider === 'openai') {
    return model.includes('gpt-4') &&
           (model.includes('vision') || model.includes('turbo'));
  }

  // Ollama: Vision models
  if (provider === 'ollama') {
    return model.includes('vision') ||
           model.includes('llava') ||
           model.includes('bakllava');
  }

  return false;
}
```

## Implementation Phases

### Phase 1: Media Analysis Command
- Create `commands/ai/analyze-media` command
- Implement caching
- Add vision-description (using llama3.2-vision)
- Add OCR (using Tesseract or existing media/ocr)

### Phase 2: Adapter Integration
- Modify OllamaAdapter to detect vision vs text-only models
- Implement `generateTextWithMediaAnalysis()` method
- Add automatic media analysis for dumb models

### Phase 3: Python ML Scripts
- Object detection (YOLO)
- Image classification (CNN)
- Segmentation (SAM)
- Leverage existing media commands for ffmpeg integration

### Phase 4: PersonaUser Integration
- Add `VisualContext` to AIDecisionContext
- Extend ChatMessageEntity to support attachments
- Enable screenshot capture in chat workflow

### Phase 5: Advanced Features
- Set-of-Mark annotations (draw bounding boxes on screenshots)
- Face detection / emotion recognition
- Video analysis (extract + analyze key frames)
- Audio sentiment analysis

## Testing Strategy

```bash
# Test 1: Vision model (smart)
ollama pull llama3.2-vision
./jtag ai/generate --model="llama3.2-vision:11b" \
  --prompt="Describe this image" \
  --image="/path/to/screenshot.png"

# Test 2: Text-only model (dumb) - should auto-analyze image
./jtag ai/generate --model="llama3.2:1b" \
  --prompt="Describe this image" \
  --image="/path/to/screenshot.png"

# Test 3: Media analysis command
./jtag ai/analyze-media --image="/path/to/screenshot.png" \
  --analyses='["vision-description","ocr","object-detection"]'

# Test 4: Cache verification (second call should be instant)
time ./jtag ai/analyze-media --image="/path/to/screenshot.png" \
  --analyses='["vision-description","ocr","object-detection"]'

# Test 5: Multiple AIs responding to image
./jtag chat/send --room="general" \
  --message="Everyone: what do you see?" \
  --image="/path/to/screenshot.png"
# Should see ~10 responses, but only 1 vision inference!
```

## References

- Research paper: "Building Autonomous LLM Agents" - Multimodal Perception section
- MM-LLM Architecture: Modality Encoder → Input Projector → LLM Backbone
- Set-of-Mark (SoM): Annotating images with markers for interactive elements
- Existing code: `daemons/ai-provider-daemon/shared/AIProviderTypesV2.ts`
- Existing media commands: `commands/media/*/` (ffmpeg integration)

---

**Status**: Architecture documented, ready for implementation.
**Next Step**: Implement Phase 1 (Media Analysis Command with caching).
