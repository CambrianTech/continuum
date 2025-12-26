# RAG Adapter Architecture - Capability-Aware Context Building

## Overview

The RAG (Retrieval-Augmented Generation) system builds LLM context from chat history, artifacts (images/videos/files), and private memories. **Different models have different capabilities**, so we need adaptive templates that route artifact processing based on what the model can handle.

## The Problem

**Vision-Capable Models** (GPT-4V, Claude 3 Sonnet, Gemini Pro Vision):
```typescript
// Can process images directly
{
  messages: [
    { role: 'user', content: 'What's in this image?', images: [base64Image] }
  ]
}
```

**Text-Only Models** (llama3.2:3b, phi3:mini, mistral-7b):
```typescript
// Need images preprocessed into text descriptions
{
  messages: [
    { role: 'user', content: 'What's in this image? [Image contains: person wearing blue shirt, dog (golden retriever), tree (oak), grass field. Detected via YOLO with 95% confidence]' }
  ]
}
```

## Architecture

```
Chat Event (with image attachment)
         ↓
RAGBuilder.buildContext(roomId, userId, options)
         ↓
   Detect Model Capabilities
         ↓
    ┌────┴────┐
    │         │
 Vision?    Text-only?
    │         │
    ↓         ↓
[Direct]  [Preprocess]
 Include    ↓
 base64   YOLO Detection
 image    ↓
 in       Image Description
 context  ↓
          Embed text in context
```

## Model Capability Detection

**AIProviderAdapter Interface (Extended):**
```typescript
interface AIProviderAdapter {
  // ... existing methods ...

  /**
   * Report this model's capabilities
   * Used by RAG system to route artifact processing
   */
  getCapabilities(modelId: string): ModelCapabilities;
}

interface ModelCapabilities {
  readonly modelId: string;
  readonly providerId: string;
  readonly capabilities: ModelCapability[];
  readonly maxContextTokens: number;
  readonly supportsImages: boolean;
  readonly supportsFunctionCalling: boolean;
  readonly supportsStreaming: boolean;
}

type ModelCapability = 'text' | 'vision' | 'function-calling' | 'streaming' | 'embeddings' | 'multimodal';
```

**Example - Ollama Adapter:**
```typescript
// OllamaAdapter.ts
getCapabilities(modelId: string): ModelCapabilities {
  // Llama 3.2 vision models support images
  if (modelId.includes('llama3.2:11b-vision') || modelId.includes('llama3.2:90b-vision')) {
    return {
      modelId,
      providerId: 'ollama',
      capabilities: ['text', 'vision', 'multimodal'],
      maxContextTokens: 128000,
      supportsImages: true,
      supportsFunctionCalling: false,
      supportsStreaming: true
    };
  }

  // Standard text-only models
  return {
    modelId,
    providerId: 'ollama',
    capabilities: ['text', 'streaming'],
    maxContextTokens: 128000,
    supportsImages: false,
    supportsFunctionCalling: false,
    supportsStreaming: true
  };
}
```

## RAG Context Building Flow

**ChatRAGBuilder (Enhanced):**
```typescript
async buildContext(
  roomId: UUID,
  userId: UUID,
  options: RAGBuildOptions
): Promise<RAGContext> {
  // 1. Load conversation history
  const messages = await this.loadMessages(roomId, options);

  // 2. Load artifacts (images, videos, files)
  const artifacts = await this.loadArtifacts(messages);

  // 3. Detect target model capabilities
  const modelCaps = options.modelCapabilities ||
                   await this.detectModelCapabilities(options);

  // 4. Process artifacts based on capabilities
  const processedArtifacts = await this.processArtifacts(
    artifacts,
    modelCaps
  );

  // 5. Build LLM message array
  const llmMessages = await this.buildMessages(
    messages,
    processedArtifacts,
    modelCaps
  );

  return {
    domain: 'chat',
    contextId: roomId,
    personaId: userId,
    identity: await this.buildIdentity(userId, roomId),
    conversationHistory: llmMessages,
    artifacts: processedArtifacts,
    privateMemories: await this.loadMemories(userId, options),
    metadata: {
      messageCount: messages.length,
      artifactCount: processedArtifacts.length,
      memoryCount: 0,
      builtAt: new Date()
    }
  };
}

private async processArtifacts(
  artifacts: RAGArtifact[],
  capabilities: ModelCapabilities
): Promise<RAGArtifact[]> {
  const processed: RAGArtifact[] = [];

  for (const artifact of artifacts) {
    if (artifact.type === 'image') {
      // Vision model → include directly
      if (capabilities.supportsImages) {
        processed.push(artifact);
      }
      // Text-only model → preprocess
      else {
        const preprocessed = await this.preprocessImage(artifact);
        processed.push({
          ...artifact,
          preprocessed
        });
      }
    }
    else if (artifact.type === 'video') {
      // Always preprocess videos (even vision models)
      const preprocessed = await this.preprocessVideo(artifact);
      processed.push({
        ...artifact,
        preprocessed
      });
    }
    else {
      processed.push(artifact);
    }
  }

  return processed;
}
```

## Image Preprocessing Pipeline

**YOLO Object Detection:**
```typescript
private async preprocessImage(artifact: RAGArtifact): Promise<RAGArtifact['preprocessed']> {
  const startTime = Date.now();

  // 1. Send image to YOLO service
  const yoloResult = await this.yoloDetect(artifact.base64 || artifact.url);

  // 2. Format detection results as natural language
  const description = this.formatYOLOResults(yoloResult);

  return {
    type: 'yolo_detection',
    result: description,
    confidence: this.calculateAverageConfidence(yoloResult.objects),
    processingTime: Date.now() - startTime,
    model: yoloResult.model
  };
}

private formatYOLOResults(yolo: YOLODetection): string {
  if (yolo.objects.length === 0) {
    return '[Image appears to be empty or contains no detectable objects]';
  }

  const objectDescriptions = yolo.objects
    .filter(obj => obj.confidence > 0.5)
    .map(obj => `${obj.class} (${Math.round(obj.confidence * 100)}% confidence)`)
    .join(', ');

  return `[Image contains: ${objectDescriptions}. Detected via ${yolo.model} in ${yolo.processingTime}ms]`;
}
```

**Example Output for Text-Only Model:**
```typescript
// User posts image of dog in park
// YOLO preprocessing generates:
"[Image contains: dog (golden retriever, 96% confidence), person (89% confidence), tree (oak, 78% confidence), grass (92% confidence), bench (wooden, 85% confidence). Detected via YOLOv8 in 43ms]"

// Persona sees this in conversation history:
{
  role: 'user',
  content: 'Check out my dog! [Image contains: dog (golden retriever, 96% confidence), person (89% confidence), tree (oak, 78% confidence), grass (92% confidence), bench (wooden, 85% confidence). Detected via YOLOv8 in 43ms]',
  name: 'Joel'
}

// Persona can respond intelligently:
"Beautiful golden retriever! Looks like you're enjoying a sunny day at the park."
```

## Vision Model Integration Plan

### Phase 1: YOLO Object Detection (Week 1)
- [ ] Add YOLO service to AI daemon
- [ ] Implement YOLOAdapter (similar to OllamaAdapter)
- [ ] Add preprocessImage() to ChatRAGBuilder
- [ ] Test with llama3.2:3b (text-only)

### Phase 2: Model Capability Registry (Week 2)
- [ ] Add getCapabilities() to AIProviderAdapter interface
- [ ] Implement in OllamaAdapter
- [ ] Add capability detection to PersonaUser
- [ ] Auto-route preprocessing based on capabilities

### Phase 3: Vision Model Support (Week 3)
- [ ] Test llama3.2:11b-vision (if available)
- [ ] Test GPT-4V adapter
- [ ] Test Claude 3 Sonnet adapter
- [ ] Verify direct image passing works

### Phase 4: Advanced Preprocessing (Week 4)
- [ ] Add OCR for text extraction from images
- [ ] Add video summarization (frame sampling + YOLO)
- [ ] Add audio transcription (Whisper)
- [ ] Add image description generation (BLIP/LLaVA)

## YOLO Service Architecture

**Option 1: Local YOLO Server (Preferred)**
```bash
# Docker container running YOLOv8
docker run -d -p 8080:8080 ultralytics/yolov8:latest

# HTTP POST endpoint
POST http://localhost:8080/detect
Content-Type: application/json
{
  "image": "base64_encoded_image",
  "confidence": 0.5,
  "model": "yolov8n"  # nano (fast) or yolov8x (accurate)
}
```

**Option 2: Python Script (Simpler)**
```bash
# scripts/yolo-detect.py
import sys
import json
import base64
from ultralytics import YOLO

model = YOLO('yolov8n.pt')  # Download on first run
image_b64 = sys.stdin.read()
results = model.predict(base64.b64decode(image_b64))
print(json.dumps(results))
```

**Option 3: Cloud API (Easiest, costs money)**
```typescript
// Roboflow, Google Vision API, AWS Rekognition
// NOT preferred - we prioritize free local models
```

## Configuration

**User Settings (UserCapabilities):**
```typescript
interface AICapabilities {
  // ... existing settings ...

  // Image processing preferences
  imageProcessing: {
    enabled: boolean;              // Allow image preprocessing?
    yoloEndpoint: string;          // Local YOLO service URL
    useCloudVision: boolean;       // Fallback to cloud if local fails?
    minConfidence: number;         // Filter detections below this (0.5)
    maxObjectsPerImage: number;    // Limit description verbosity (10)
  };

  // Vision model preferences
  preferVisionModels: boolean;     // Prefer vision-capable models when images present?
  fallbackToPreprocessing: boolean; // If vision model unavailable, preprocess?
}
```

## Testing Strategy

**Test 1: Text-Only Model + YOLO Preprocessing**
```bash
# 1. Post image to chat
./jtag exec --code="/* upload image to general room */"

# 2. Verify YOLO preprocessing
./jtag debug/logs --filterPattern="YOLO|preprocessing" --tailLines=20

# 3. Check persona response
./jtag interface/screenshot --querySelector="chat-widget"

# Expect: Persona responds about image content despite text-only model
```

**Test 2: Vision Model + Direct Image**
```bash
# 1. Switch to vision model
# Edit PersonaUser.ts: model: 'llama3.2:11b-vision'

# 2. Post same image
./jtag exec --code="/* upload image */"

# 3. Verify NO preprocessing
./jtag debug/logs --filterPattern="YOLO" --tailLines=20

# Expect: No YOLO logs, image passed directly to model
```

**Test 3: Capability Auto-Detection**
```bash
# 1. System should detect llama3.2:3b = text-only
# 2. System should detect llama3.2:11b-vision = vision-capable
# 3. System should route accordingly

# Verify:
./jtag ai/list-providers --includeCapabilities=true
# Expect: Shows each model's capabilities
```

## Open Questions

1. **YOLO Model Size:** YOLOv8n (6MB, fast) vs YOLOv8x (131MB, accurate)?
   - Start with nano, add option for larger models later

2. **Image Description Quality:** YOLO only detects objects, not scenes/actions/emotions
   - Phase 4: Add BLIP/LLaVA for richer descriptions
   - "Person smiling while petting golden retriever in sunny park"

3. **Video Preprocessing:** Frame sampling strategy?
   - Sample 1 frame/second
   - Run YOLO on each frame
   - Aggregate results: "Video shows: person walking dog through park (0:00-0:15), dog playing with ball (0:15-0:30)"

4. **Cost of Preprocessing:** YOLO adds 40-100ms per image
   - Acceptable for personas (async anyway)
   - May need caching for repeated images

5. **Multiple Images in One Message:** How to handle?
   - Process each separately
   - Combine descriptions: "[Image 1: ...] [Image 2: ...]"

## Related Files

- [RAGTypes.ts](./shared/RAGTypes.ts) - Type definitions (just updated)
- [ChatRAGBuilder.ts](./builders/ChatRAGBuilder.ts) - Chat-specific RAG builder
- [PersonaUser.ts](/Volumes/FlashGordon/cambrian/continuum/src/debug/jtag/system/user/shared/PersonaUser.ts) - AI persona implementation
- [AIProviderTypes.ts](/Volumes/FlashGordon/cambrian/continuum/src/debug/jtag/daemons/ai-provider-daemon/shared/AIProviderTypes.ts) - AI provider interfaces

## Changelog

- **2025-10-06**: Initial RAG adapter architecture
  - Defined model capability detection
  - Designed image preprocessing pipeline
  - Outlined YOLO integration strategy
  - Created 4-phase implementation plan
