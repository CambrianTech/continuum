# Media Format Conversion Architecture

## Problem Statement

AI providers and models have varying levels of support for media formats:
- **DeepSeek API**: Doesn't support WebP images (API limitation, not model)
- **Older vision models**: May only support JPEG/PNG
- **Modern models**: Support WebP, AVIF, and other formats
- **Audio/Video**: Different codecs and containers supported by different providers

**The Challenge**: How do we transparently convert media to compatible formats without hardcoding provider knowledge everywhere?

**Real-World Example**: DeepSeek initially couldn't see WebP images: *"I can see the attachment metadata for image-8.webp but I'm not receiving the actual image data"*. Other AIs (Claude, Grok, Together, Fireworks) saw it fine. This is an API preprocessing limitation, not a model capability issue.

---

## Design Principles

### 1. Adapters Are Decision-Makers
Each adapter knows what it supports. No external registry or mapping.

### 2. Config Is Just Data
Config provides information (model name, provider, optional capabilities). Adapter interprets the data.

### 3. Conversion Is Bidirectional
**Input (to API)**: User sends media → Convert to format API supports
**Output (from API)**: API returns media → Convert to format user/system prefers

This ensures compatibility in BOTH directions.

### 4. Conversion Is Transparent
Users don't know/care about conversion. System handles it automatically at adapter boundaries.

### 5. Composable Architecture
Use existing `media/process` command for actual conversion work. No duplication of conversion logic.

### 6. Sensible Defaults
Safe fallback to most compatible formats (JPEG for images). Most adapters won't need custom logic.

---

## Architecture Overview

### Input Direction (User → API)

```
┌─────────────────────────────────────────────────────────────┐
│ User sends WebP image                                        │
└────────────────────────┬────────────────────────────────────┘
                         │
                         ▼
┌─────────────────────────────────────────────────────────────┐
│ ChatMessageEntity stored with WebP MediaItem                │
└────────────────────────┬────────────────────────────────────┘
                         │
                         ▼
┌─────────────────────────────────────────────────────────────┐
│ PersonaUser processes message, prepares API call             │
└────────────────────────┬────────────────────────────────────┘
                         │
                         ▼
┌─────────────────────────────────────────────────────────────┐
│ Adapter.generateResponse() called                            │
├─────────────────────────────────────────────────────────────┤
│ 1. Adapter calls prepareInputMedia(message.media)            │
│ 2. Adapter checks getInputCapabilities()                     │
│ 3. WebP not in supportedImageFormats                         │
│ 4. MediaConverter converts WebP → JPEG                       │
│ 5. Adapter sends JPEG to API                                 │
└────────────────────────┬────────────────────────────────────┘
                         │
                         ▼
┌─────────────────────────────────────────────────────────────┐
│ API receives JPEG (compatible format)                        │
└─────────────────────────────────────────────────────────────┘
```

### Output Direction (API → User)

```
┌─────────────────────────────────────────────────────────────┐
│ API generates audio response (MP3)                           │
└────────────────────────┬────────────────────────────────────┘
                         │
                         ▼
┌─────────────────────────────────────────────────────────────┐
│ Adapter receives API response with MediaItem[]              │
├─────────────────────────────────────────────────────────────┤
│ 1. Adapter calls prepareOutputMedia(response.media)          │
│ 2. Adapter checks getOutputCapabilities()                    │
│ 3. System prefers WAV for audio processing                   │
│ 4. MediaConverter converts MP3 → WAV                         │
│ 5. Adapter returns WAV to system                             │
└────────────────────────┬────────────────────────────────────┘
                         │
                         ▼
┌─────────────────────────────────────────────────────────────┐
│ ChatMessageEntity stored with WAV MediaItem                 │
└────────────────────────┬────────────────────────────────────┘
                         │
                         ▼
┌─────────────────────────────────────────────────────────────┐
│ User receives audio in preferred format                     │
└─────────────────────────────────────────────────────────────┘
```

---

## Native vs Adapted Media Capabilities

**Key Insight**: Providers differ in what they support natively vs what we add through conversion:

### Native Capability Examples

**OpenAI Whisper** (native audio input):
```typescript
{
  supportedAudioInputFormats: ['audio/mp3', 'audio/wav', 'audio/m4a'],
  // API handles audio natively - no conversion needed
}
```

**ElevenLabs** (native audio output):
```typescript
{
  audioOutputFormats: ['audio/mp3', 'audio/wav'],
  // API generates audio natively
}
```

**Anthropic Claude** (native vision):
```typescript
{
  supportedImageFormats: ['image/jpeg', 'image/png', 'image/gif', 'image/webp'],
  // Model processes images natively
}
```

### Adapted Capability Examples

**DeepSeek** (vision with format restrictions):
```typescript
{
  supportedImageFormats: ['image/jpeg', 'image/png', 'image/gif'],
  // WebP not supported - adapter converts WebP → JPEG
}
```

**Local Llama** (text-only model + vision adapter):
```typescript
{
  supportedImageFormats: [], // No native vision
  // Future: Adapter could add vision via image-to-text preprocessing
}
```

**Audio via Whisper Adapter**:
```typescript
// AI without native audio support
// Adapter adds: audio → Whisper → text → AI
{
  supportedAudioFormats: ['audio/mp3', 'audio/wav'], // Via adapter
  audioInputPipeline: 'whisper-transcription'
}
```

---

## Core Components

### 1. MediaCapabilities Interface

**Location**: `daemons/ai-provider-daemon/shared/MediaCapabilities.ts`

```typescript
/**
 * Declares what media formats an adapter/provider/model supports
 */
export interface MediaCapabilities {
  /** Supported image MIME types */
  supportedImageFormats: string[];

  /** Supported audio MIME types */
  supportedAudioFormats: string[];

  /** Supported video MIME types */
  supportedVideoFormats: string[];

  /** Target format for image conversion (when unsupported) */
  imageConversionTarget?: string; // Default: 'image/jpeg'

  /** Target format for audio conversion */
  audioConversionTarget?: string; // Default: 'audio/mpeg'

  /** Target format for video conversion */
  videoConversionTarget?: string; // Default: 'video/mp4'
}
```

---

### 2. MediaConverter Utility

**Location**: `daemons/ai-provider-daemon/shared/MediaConverter.ts`

**Responsibility**: Pure utility that converts media based on capabilities. No adapter knowledge.

```typescript
export class MediaConverter {
  /**
   * Prepare media for specific capabilities
   * Converts unsupported formats to supported ones
   */
  static async prepareForCapabilities(
    media: MediaItem[],
    capabilities: MediaCapabilities,
    context: JTAGContext,
    sessionId: UUID
  ): Promise<MediaItem[]> {
    const prepared: MediaItem[] = [];

    for (const item of media) {
      if (this.isSupported(item.mimeType, capabilities)) {
        // Format is supported - pass through
        prepared.push(item);
      } else {
        // Format not supported - convert
        const converted = await this.convert(item, capabilities, context, sessionId);
        prepared.push(converted);
      }
    }

    return prepared;
  }

  /**
   * Check if a MIME type is supported by capabilities
   */
  private static isSupported(
    mimeType: string,
    capabilities: MediaCapabilities
  ): boolean {
    if (mimeType.startsWith('image/')) {
      return capabilities.supportedImageFormats.includes(mimeType);
    }
    if (mimeType.startsWith('audio/')) {
      return capabilities.supportedAudioFormats.includes(mimeType);
    }
    if (mimeType.startsWith('video/')) {
      return capabilities.supportedVideoFormats.includes(mimeType);
    }
    return true; // Unknown type - pass through
  }

  /**
   * Convert media item to supported format
   */
  private static async convert(
    item: MediaItem,
    capabilities: MediaCapabilities,
    context: JTAGContext,
    sessionId: UUID
  ): Promise<MediaItem> {
    // Determine target format based on media type
    let targetFormat: string | undefined;

    if (item.type === 'image') {
      targetFormat = capabilities.imageConversionTarget || 'image/jpeg';
    } else if (item.type === 'audio') {
      targetFormat = capabilities.audioConversionTarget || 'audio/mpeg';
    } else if (item.type === 'video') {
      targetFormat = capabilities.videoConversionTarget || 'video/mp4';
    }

    if (!targetFormat) {
      console.warn(`⚠️ No conversion target for ${item.mimeType}, sending original`);
      return item;
    }

    console.log(`🔄 Converting ${item.mimeType} → ${targetFormat} for provider compatibility`);

    // Use media/process command for actual conversion
    const result = await Commands.execute('media/process', {
      base64: item.base64,
      sourceMimeType: item.mimeType,
      targetMimeType: targetFormat,
      quality: 90,
      context,
      sessionId
    });

    if (!result.success) {
      console.warn(`⚠️ Conversion failed: ${result.error}, sending original format`);
      return item;
    }

    return {
      ...item,
      base64: result.base64,
      mimeType: targetFormat,
      filename: item.filename.replace(/\.\w+$/, `.${this.getExtension(targetFormat)}`)
    };
  }

  /**
   * Get file extension for MIME type
   */
  private static getExtension(mimeType: string): string {
    const map: Record<string, string> = {
      'image/jpeg': 'jpg',
      'image/png': 'png',
      'image/gif': 'gif',
      'image/webp': 'webp',
      'audio/mpeg': 'mp3',
      'audio/wav': 'wav',
      'video/mp4': 'mp4',
      'video/webm': 'webm'
    };
    return map[mimeType] || 'bin';
  }
}
```

---

### 3. BaseAIProviderAdapter Integration

**Location**: `daemons/ai-provider-daemon/shared/adapters/BaseAIProviderAdapter.ts`

**Changes**:

```typescript
export abstract class BaseAIProviderAdapter {
  protected config: AdapterConfig;
  protected context: JTAGContext;
  protected sessionId: UUID;

  /**
   * Get media capabilities for this adapter
   *
   * Two paths:
   * 1. Explicit: config.capabilities provided → use directly
   * 2. Inferred: adapter logic based on model/provider
   */
  protected getMediaCapabilities(): MediaCapabilities {
    // Path 1: Explicit capabilities in config (overrides)
    if (this.config.capabilities?.supportedImageFormats) {
      return {
        supportedImageFormats: this.config.capabilities.supportedImageFormats,
        supportedAudioFormats: this.config.capabilities.supportedAudioFormats || [],
        supportedVideoFormats: this.config.capabilities.supportedVideoFormats || [],
        imageConversionTarget: this.config.capabilities.imageConversionTarget || 'image/jpeg',
        audioConversionTarget: this.config.capabilities.audioConversionTarget || 'audio/mpeg',
        videoConversionTarget: this.config.capabilities.videoConversionTarget || 'video/mp4'
      };
    }

    // Path 2: Inferred from model/provider (subclasses override)
    return this.inferCapabilitiesFromModel();
  }

  /**
   * Infer capabilities from model name/provider
   * Override in subclasses for provider-specific logic
   */
  protected inferCapabilitiesFromModel(): MediaCapabilities {
    // Safe default - most compatible formats
    return {
      supportedImageFormats: ['image/jpeg', 'image/png'],
      supportedAudioFormats: [],
      supportedVideoFormats: [],
      imageConversionTarget: 'image/jpeg',
      audioConversionTarget: 'audio/mpeg',
      videoConversionTarget: 'video/mp4'
    };
  }

  /**
   * Prepare media for this adapter's capabilities
   * Called automatically before sending to API
   */
  protected async prepareMedia(media: MediaItem[]): Promise<MediaItem[]> {
    const capabilities = this.getMediaCapabilities();
    return MediaConverter.prepareForCapabilities(
      media,
      capabilities,
      this.context,
      this.sessionId
    );
  }

  /**
   * Generate response - now prepares media automatically
   */
  async generateResponse(messages: ConversationMessage[]): Promise<AIResponse> {
    // Prepare all media in messages for this adapter
    const preparedMessages = await Promise.all(
      messages.map(async msg => ({
        ...msg,
        media: msg.media ? await this.prepareMedia(msg.media) : undefined
      }))
    );

    // Call API with prepared messages
    return this.callAPI(preparedMessages);
  }

  /**
   * Subclasses implement the actual API call
   */
  protected abstract callAPI(messages: ConversationMessage[]): Promise<AIResponse>;
}
```

---

### 4. AdapterConfig Extension

**Location**: `daemons/ai-provider-daemon/shared/AdapterTypes.ts`

```typescript
export interface AdapterConfig {
  provider: string;
  model: string;
  apiKey?: string;
  baseURL?: string;

  // Optional explicit capabilities (overrides inference)
  capabilities?: {
    vision?: boolean;
    audio?: boolean;
    tools?: boolean;

    // Media format capabilities
    supportedImageFormats?: string[];
    supportedAudioFormats?: string[];
    supportedVideoFormats?: string[];
    imageConversionTarget?: string;
    audioConversionTarget?: string;
    videoConversionTarget?: string;
  };
}
```

---

## Provider-Specific Implementations

### DeepSeekAdapter

**Issue**: DeepSeek API doesn't support WebP

```typescript
export class DeepSeekAdapter extends BaseOpenAICompatibleAdapter {
  /**
   * DeepSeek API limitation - no WebP support
   */
  protected inferCapabilitiesFromModel(): MediaCapabilities {
    return {
      supportedImageFormats: ['image/jpeg', 'image/png', 'image/gif'],
      supportedAudioFormats: [],
      supportedVideoFormats: [],
      imageConversionTarget: 'image/jpeg'
    };
  }
}
```

---

### AnthropicAdapter

**Modern API**: Supports WebP and other formats

```typescript
export class AnthropicAdapter extends BaseAIProviderAdapter {
  protected inferCapabilitiesFromModel(): MediaCapabilities {
    return {
      supportedImageFormats: ['image/jpeg', 'image/png', 'image/gif', 'image/webp'],
      supportedAudioFormats: [],
      supportedVideoFormats: [],
      imageConversionTarget: 'image/jpeg'
    };
  }
}
```

---

### OllamaAdapter

**Model-specific logic**: Different Ollama models have different capabilities

```typescript
export class OllamaAdapter extends BaseLocalAdapter {
  protected inferCapabilitiesFromModel(): MediaCapabilities {
    const modelName = this.config.model;

    // Model-specific capabilities
    if (modelName.includes('llama3.2-vision:11b') || modelName.includes('llava:34b')) {
      return {
        supportedImageFormats: ['image/jpeg', 'image/png', 'image/webp'],
        supportedAudioFormats: [],
        supportedVideoFormats: [],
        imageConversionTarget: 'image/jpeg'
      };
    }

    // Older vision models
    if (modelName.includes('vision') || modelName.includes('llava')) {
      return {
        supportedImageFormats: ['image/jpeg', 'image/png'],
        supportedAudioFormats: [],
        supportedVideoFormats: [],
        imageConversionTarget: 'image/jpeg'
      };
    }

    // No vision support
    return {
      supportedImageFormats: [],
      supportedAudioFormats: [],
      supportedVideoFormats: [],
      imageConversionTarget: 'image/jpeg'
    };
  }
}
```

---

### OpenAIAdapter

**Modern API**: Full format support

```typescript
export class OpenAIAdapter extends BaseOpenAICompatibleAdapter {
  protected inferCapabilitiesFromModel(): MediaCapabilities {
    return {
      supportedImageFormats: ['image/jpeg', 'image/png', 'image/gif', 'image/webp'],
      supportedAudioFormats: ['audio/mpeg', 'audio/wav'],
      supportedVideoFormats: [],
      imageConversionTarget: 'image/jpeg',
      audioConversionTarget: 'audio/mpeg'
    };
  }
}
```

---

## Usage Examples

### Example 1: Default Behavior (Most Adapters)

```typescript
// Most adapters don't need custom logic
export class GenericProviderAdapter extends BaseAIProviderAdapter {
  // Don't override - uses safe defaults
  // inferCapabilitiesFromModel() returns JPEG/PNG only
}
```

**Result**: WebP → JPEG, AVIF → JPEG, modern formats → safe fallback

---

### Example 2: Explicit Config Override

```typescript
const config: AdapterConfig = {
  provider: 'custom-provider',
  model: 'experimental-vision-v2',
  capabilities: {
    supportedImageFormats: ['image/jpeg', 'image/png', 'image/webp', 'image/avif']
  }
};

// Adapter will use these explicit capabilities
// Useful for testing or unknown providers
```

---

### Example 3: Model-Specific Inference

```typescript
export class CustomAdapter extends BaseAIProviderAdapter {
  protected inferCapabilitiesFromModel(): MediaCapabilities {
    // Check config.model and decide
    if (this.config.model.includes('v2')) {
      return { /* modern formats */ };
    }
    if (this.config.model.includes('v1')) {
      return { /* legacy formats */ };
    }
    return super.inferCapabilitiesFromModel(); // Safe default
  }
}
```

---

## Implementation Flow

### Message Preparation Flow

```
1. PersonaUser receives ChatMessageEntity with media: [WebP, PNG, JPEG]

2. PersonaUser calls Adapter.generateResponse(messages)

3. Adapter.generateResponse():
   a. For each message with media:
      - Call this.prepareMedia(msg.media)

4. Adapter.prepareMedia(media):
   a. Call this.getMediaCapabilities()
   b. Pass to MediaConverter.prepareForCapabilities()

5. MediaConverter.prepareForCapabilities():
   a. For each MediaItem:
      - Check if mimeType in capabilities.supportedImageFormats
      - If supported: pass through unchanged
      - If not supported: convert using media/process command
   b. Return converted media array

6. Adapter sends prepared messages to API with converted formats

7. API receives compatible formats, processes successfully
```

---

## Testing Strategy

### Unit Tests

```typescript
// MediaConverter.test.ts
describe('MediaConverter', () => {
  it('should pass through supported formats', async () => {
    const capabilities = {
      supportedImageFormats: ['image/jpeg', 'image/webp'],
      supportedAudioFormats: [],
      supportedVideoFormats: []
    };

    const media = [{ mimeType: 'image/webp', base64: '...' }];
    const result = await MediaConverter.prepareForCapabilities(media, capabilities, ctx, sid);

    expect(result[0].mimeType).toBe('image/webp'); // Unchanged
  });

  it('should convert unsupported formats', async () => {
    const capabilities = {
      supportedImageFormats: ['image/jpeg', 'image/png'],
      imageConversionTarget: 'image/jpeg'
    };

    const media = [{ mimeType: 'image/webp', base64: '...' }];
    const result = await MediaConverter.prepareForCapabilities(media, capabilities, ctx, sid);

    expect(result[0].mimeType).toBe('image/jpeg'); // Converted
  });
});
```

---

### Integration Tests

```typescript
// DeepSeekAdapter.test.ts
describe('DeepSeekAdapter', () => {
  it('should convert WebP to JPEG before API call', async () => {
    const adapter = new DeepSeekAdapter(config, context, sessionId);

    const messages = [{
      role: 'user',
      content: 'What is this?',
      media: [{ type: 'image', mimeType: 'image/webp', base64: '...' }]
    }];

    // Spy on callAPI to check what it receives
    const callAPISpy = jest.spyOn(adapter as any, 'callAPI');

    await adapter.generateResponse(messages);

    // Verify media was converted before API call
    const preparedMessages = callAPISpy.mock.calls[0][0];
    expect(preparedMessages[0].media[0].mimeType).toBe('image/jpeg');
  });
});
```

---

### End-to-End Test

```bash
# Send WebP image to DeepSeek via CLI
./jtag collaboration/chat/send --room="general" --message="Test WebP" --media ../test-images/image-8.webp

# Expected behavior:
# 1. ChatSendServerCommand stores WebP in database
# 2. PersonaUser (DeepSeek) receives message
# 3. DeepSeekAdapter converts WebP → JPEG
# 4. DeepSeek API receives JPEG
# 5. DeepSeek responds successfully (can see image)

# Verify in logs:
# "🔄 Converting image/webp → image/jpeg for provider compatibility"
```

---

## Logging and Debugging

**Conversion logging**:
```
🔄 Converting image/webp → image/jpeg for provider compatibility
✅ Converted 156KB WebP → 143KB JPEG (quality: 90)
```

**Pass-through logging**:
```
✓ Media format image/jpeg supported by adapter, no conversion needed
```

**Failure logging**:
```
⚠️ Conversion failed: media/process command error, sending original format
⚠️ Provider may reject unsupported format image/webp
```

---

## Future Extensions

### 1. Size/Resolution Limits

```typescript
interface MediaCapabilities {
  maxImageDimensions?: { width: number; height: number };
  maxImageSizeBytes?: number;
  maxAudioDurationSeconds?: number;
  maxVideoDurationSeconds?: number;
}
```

### 2. Quality/Compression Preferences

```typescript
interface MediaCapabilities {
  preferredImageQuality?: number; // 1-100
  preferredCompressionLevel?: 'low' | 'medium' | 'high';
}
```

### 3. Caching Conversions

```typescript
// Cache converted media to avoid re-converting same files
class MediaConversionCache {
  private cache: Map<string, MediaItem> = new Map();

  getCacheKey(item: MediaItem, targetFormat: string): string {
    return `${item.mimeType}:${targetFormat}:${hash(item.base64)}`;
  }
}
```

### 4. Progressive Conversion

```typescript
// Try native format first, convert on error
async generateResponse(messages: ConversationMessage[]): Promise<AIResponse> {
  try {
    return await this.callAPI(messages); // Try original formats
  } catch (error) {
    if (this.isFormatError(error)) {
      // Convert and retry
      const prepared = await this.prepareMedia(messages);
      return await this.callAPI(prepared);
    }
    throw error;
  }
}
```

---

## Migration Path

### Phase 1: Core Infrastructure (This PR)
- ✅ MediaCapabilities interface
- ✅ MediaConverter utility
- ✅ BaseAIProviderAdapter integration
- ✅ DeepSeekAdapter WebP fix

### Phase 2: Adapter Rollout
- Override inferCapabilitiesFromModel() in all adapters
- Test with various formats
- Document known limitations per provider

### Phase 3: Advanced Features
- Size/resolution limits
- Quality preferences
- Conversion caching
- Progressive conversion with retry

---

## Related Documents

- **[AI Adapter Architecture](./AI-ADAPTER-ARCHITECTURE.md)** - Overall adapter design
- **[Natural Idioms Design Principle](../../docs/DESIGN-PRINCIPLE-NATURAL-IDIOMS.md)** - Why transparency matters
- **[media/process Command](../../commands/media/process/)** - Actual conversion implementation

---

## Summary

**The Problem**: Providers have different media format support (DeepSeek no WebP, older models limited formats)

**The Solution**: Adapters declare capabilities, MediaConverter transparently converts mismatches

**The Pattern**:
```
Config (data) → Adapter (decision) → MediaConverter (execution) → media/process (implementation)
```

**The Benefit**: Users send any format, system ensures compatibility automatically. Adapters encapsulate provider knowledge. No hardcoded registries.
