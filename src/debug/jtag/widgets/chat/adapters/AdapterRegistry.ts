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

export type ContentType = 'text' | 'image' | 'video' | 'url_card' | 'file' | 'code_editor';

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

    // 1. Check for attachments first (images, videos, files)
    if (message.content?.attachments?.length > 0) {
      const firstAttachment = message.content.attachments[0];

      // Image attachments
      if (firstAttachment.type === 'image' || firstAttachment.mimeType?.startsWith('image/')) {
        return this.adapters.get('image') ?? this.adapters.get('text') ?? null;
      }

      // Video attachments
      if (firstAttachment.type === 'video' || firstAttachment.mimeType?.startsWith('video/')) {
        return this.adapters.get('video') ?? this.adapters.get('text') ?? null;
      }

      // URL cards (special attachment type for rich link previews)
      if (firstAttachment.type === 'url_card') {
        return this.adapters.get('url_card') ?? this.adapters.get('text') ?? null;
      }

      // Generic file fallback
      return this.adapters.get('file') ?? this.adapters.get('text') ?? null;
    }

    // 2. Check for special text content types
    const text = message.content?.text || '';

    // URL detection for link previews
    if (this.isURL(text)) {
      return this.adapters.get('url_card') ?? this.adapters.get('text') ?? null;
    }

    // 3. Default to text adapter for markdown rendering
    return this.adapters.get('text') ?? null;
  }

  /**
   * Get all registered adapters (for CSS injection)
   */
  getAllAdapters(): AbstractMessageAdapter<any>[] {
    return Array.from(this.adapters.values());
  }

  /**
   * Simple URL detection
   */
  private isURL(text: string): boolean {
    const urlPattern = /^https?:\/\/[^\s]+$/;
    return urlPattern.test(text.trim());
  }
}
