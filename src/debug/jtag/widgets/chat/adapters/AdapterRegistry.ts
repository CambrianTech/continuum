/**
 * Adapter Registry - Selects the appropriate adapter for each message
 *
 * Supports multiple adapters per message (e.g., text + image)
 * Dynamically determines which adapter(s) to use based on message content
 */

import type { ChatMessageEntity } from '../../../system/data/entities/ChatMessageEntity';
import type { AbstractMessageAdapter } from './AbstractMessageAdapter';
import { TextMessageAdapter } from './TextMessageAdapter';
// Future imports:
// import { ImageMessageAdapter } from './ImageMessageAdapter';
// import { VideoMessageAdapter } from './VideoMessageAdapter';
// import { URLCardAdapter } from './URLCardAdapter';

export type ContentType = 'text' | 'image' | 'video' | 'audio' | 'file' | 'document' | 'code_editor';

export class AdapterRegistry {
  private adapters: Map<ContentType, AbstractMessageAdapter<any>>;

  constructor() {
    this.adapters = new Map();

    // Register available adapters
    this.adapters.set('text', new TextMessageAdapter());
    // Future registrations:
    // this.adapters.set('image', new ImageMessageAdapter());
    // this.adapters.set('video', new VideoMessageAdapter());
    // this.adapters.set('url_card', new URLCardAdapter());
  }

  /**
   * Determine which adapter(s) to use for a given message
   * Returns the primary adapter for now (will support multiple adapters in future)
   */
  selectAdapter(message: ChatMessageEntity): AbstractMessageAdapter<any> | null {
    // Priority order for content type detection:

    // 1. Check for media first (images, videos, files)
    if (message.content?.media && message.content.media.length > 0) {
      const firstMedia = message.content.media[0];

      // Image media
      if (firstMedia.type === 'image' || firstMedia.mimeType?.startsWith('image/')) {
        return this.adapters.get('image') ?? this.adapters.get('text') ?? null;
      }

      // Video media
      if (firstMedia.type === 'video' || firstMedia.mimeType?.startsWith('video/')) {
        return this.adapters.get('video') ?? this.adapters.get('text') ?? null;
      }

      // Audio media
      if (firstMedia.type === 'audio' || firstMedia.mimeType?.startsWith('audio/')) {
        return this.adapters.get('audio') ?? this.adapters.get('text') ?? null;
      }

      // Generic file/document fallback
      return this.adapters.get('file') ?? this.adapters.get('text') ?? null;
    }

    // 2. Default to text adapter for markdown rendering
    return this.adapters.get('text') ?? null;
  }

  /**
   * Get all registered adapters (for CSS injection)
   */
  getAllAdapters(): AbstractMessageAdapter<any>[] {
    return Array.from(this.adapters.values());
  }
}
