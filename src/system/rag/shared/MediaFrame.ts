/**
 * MediaFrame — Unified media representation with lazy, cached representations.
 *
 * Inspired by AR's CBarFrame: one media item, multiple representations,
 * lazy processing, process-once-serve-many.
 *
 * The same image/audio/video can be consumed by:
 * - Vision models → raw base64
 * - Text-only models → text description (via VisionDescriptionService)
 * - Object detection → YOLO bounding boxes
 * - OCR → extracted text
 * - Audio models → raw PCM
 * - Text models → STT transcript
 *
 * Each representation is generated on first request and cached.
 * Content-addressed: same content = same cache key across all consumers.
 *
 * Architecture parallel:
 *   Audio:  raw stream → VAD (trigger) → STT (transcript) → timeline
 *   Visual: raw frame  → YOLO (trigger) → description     → timeline
 *   Both:   source → cheap detection → rich processing → shared cache → all consumers
 */

/**
 * Types of representations a media frame can provide.
 * Each representation is lazily generated on first request.
 */
export type MediaRepresentationType =
  // Image representations
  | 'image/base64'           // Raw image data (instant if source is image)
  | 'image/description'      // Natural language description (VisionDescriptionService)
  | 'image/objects'          // YOLO/object detection results
  | 'image/segmentation'    // Semantic segmentation map
  | 'image/ocr'             // Extracted text via OCR
  // Audio representations
  | 'audio/pcm'             // Raw PCM audio data
  | 'audio/transcript'      // STT transcription
  | 'audio/speaker-id'      // Speaker identification
  // Video representations
  | 'video/frames'          // Key frames extracted
  | 'video/summary'         // Natural language summary
  // Structured representations
  | 'metadata/scene'        // Scene-level metadata (objects, people, actions)
  | 'metadata/caption';     // Short caption suitable for system prompt injection

/**
 * A single representation of a media frame.
 */
export interface MediaRepresentation {
  /** The type of this representation */
  type: MediaRepresentationType;
  /** The content — string for text, base64 for binary, object for structured */
  content: string | Record<string, unknown>;
  /** How confident we are in this representation (0-1) */
  confidence: number;
  /** How long it took to generate this representation (ms) */
  processingTimeMs: number;
  /** Model/adapter that produced this (e.g., 'candle/llava:7b', 'whisper', 'yolov8') */
  producer: string;
  /** When this representation was generated */
  generatedAt: number;
}

/**
 * Source types for media frames.
 */
export type MediaFrameSource =
  | 'chat-attachment'        // Drag-and-drop or pasted in chat
  | 'screenshot'             // interface/screenshot capture
  | 'avatar-snapshot'        // Bevy avatar scene capture
  | 'live-video'             // Live video call frame
  | 'webcam'                 // User's camera feed
  | 'screen-share'           // Screen sharing stream
  | 'canvas'                 // Drawing canvas content
  | 'generated';             // AI-generated image

/**
 * A media frame — one piece of media with lazily populated representations.
 *
 * Content-addressed by a hash of the raw data. Two identical images
 * produce the same frame key and share cached representations.
 *
 * The frame itself is lightweight — it holds metadata and a cache key.
 * Raw data (base64) is stored only if explicitly requested.
 */
export interface MediaFrame {
  /** Content-addressed key (hash of raw data) */
  readonly key: string;
  /** Where this media came from */
  readonly source: MediaFrameSource;
  /** When the media was captured/uploaded */
  readonly timestamp: number;
  /** MIME type of the original media */
  readonly mimeType: string;
  /** Original filename if from file upload */
  readonly filename?: string;
  /** Who produced this frame (user ID or 'system') */
  readonly producerId: string;
  /** Session/call ID if from a live context */
  readonly sessionId?: string;

  /** Available representations (lazily populated) */
  readonly representations: ReadonlyMap<MediaRepresentationType, MediaRepresentation>;

  /** Whether this frame has been described (for cache-hit tracking) */
  readonly isDescribed: boolean;

  /** Byte size of the original media (for budget calculations) */
  readonly originalSizeBytes: number;
}

/**
 * Factory for creating MediaFrame instances.
 * Handles content-addressing and initial representation setup.
 */
export interface MediaFrameFactory {
  /**
   * Create a MediaFrame from raw base64 data.
   * Computes content key and stores the base64 as the initial representation.
   */
  fromBase64(
    base64: string,
    mimeType: string,
    source: MediaFrameSource,
    metadata?: {
      filename?: string;
      producerId?: string;
      sessionId?: string;
    }
  ): MediaFrame;

  /**
   * Create a MediaFrame from a URL reference (no base64 loaded yet).
   * The raw data can be fetched lazily when a representation is requested.
   */
  fromUrl(
    url: string,
    mimeType: string,
    source: MediaFrameSource,
    metadata?: {
      filename?: string;
      producerId?: string;
      sessionId?: string;
    }
  ): MediaFrame;
}

/**
 * Adapter interface for generating a specific representation type.
 *
 * Follows the polymorphism pattern (OpenCV-style):
 * - Each adapter handles one representation type
 * - Registered in a factory/registry
 * - Selected at runtime based on what the consumer needs
 *
 * Examples:
 * - VisionDescriptionAdapter → 'image/description' (uses VisionDescriptionService)
 * - YoloDetectionAdapter → 'image/objects' (uses ONNX YOLO model in Rust)
 * - SttTranscriptionAdapter → 'audio/transcript' (uses STT pipeline)
 * - OcrAdapter → 'image/ocr' (uses Tesseract or vision model)
 */
export interface RepresentationAdapter {
  /** Which representation type this adapter produces */
  readonly outputType: MediaRepresentationType;

  /** Which input types this adapter can consume (e.g., image/base64 → image/description) */
  readonly inputTypes: MediaRepresentationType[];

  /** Human-readable name for logging */
  readonly name: string;

  /**
   * Generate a representation from an existing one.
   * @param input - An existing representation to transform
   * @param frame - The parent frame (for metadata)
   * @returns The new representation, or null if generation failed
   */
  generate(
    input: MediaRepresentation,
    frame: MediaFrame
  ): Promise<MediaRepresentation | null>;
}
