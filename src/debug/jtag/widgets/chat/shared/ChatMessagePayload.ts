/**
 * Chat Message Payload Architecture - Extensible Content Types
 * 
 * Supports Discord-scale rich message content including:
 * - Text with formatting (bold, italic, code, etc.)
 * - URL previews and link cards
 * - Images and media attachments
 * - Video embeds and audio clips
 * - HTML content and interactive elements
 * - File attachments of any type
 * - Custom interactive components
 * 
 * Architecture inspired by JTAGPayload for extensibility.
 */

import type { JTAGPayload } from '../../../system/core/types/JTAGTypes';

/**
 * Base content type discriminator
 */
export type ChatContentType = 
  | 'text'
  | 'rich_text' 
  | 'url_card'
  | 'image'
  | 'video'
  | 'audio'
  | 'file_attachment'
  | 'html_content'
  | 'interactive_component'
  | 'system_message'
  | 'custom';

/**
 * Rich text formatting information
 */
export interface TextFormatting {
  readonly bold?: ReadonlyArray<[number, number]>; // [start, end] positions
  readonly italic?: ReadonlyArray<[number, number]>;
  readonly underline?: ReadonlyArray<[number, number]>;
  readonly strikethrough?: ReadonlyArray<[number, number]>;
  readonly code?: ReadonlyArray<[number, number]>; // Inline code
  readonly codeBlock?: ReadonlyArray<[number, number, string]>; // [start, end, language]
  readonly spoiler?: ReadonlyArray<[number, number]>; // Hidden text
  readonly highlight?: ReadonlyArray<[number, number, string]>; // [start, end, color]
  readonly links?: ReadonlyArray<{
    readonly start: number;
    readonly end: number;
    readonly url: string;
    readonly title?: string;
  }>;
  readonly mentions?: ReadonlyArray<{
    readonly start: number;
    readonly end: number;
    readonly userId: string;
    readonly userName: string;
    readonly type: 'user' | 'role' | 'channel';
  }>;
  readonly emojis?: ReadonlyArray<{
    readonly start: number;
    readonly end: number;
    readonly emojiId: string;
    readonly emojiName: string;
    readonly isCustom: boolean;
    readonly url?: string; // For custom emojis
  }>;
}

/**
 * URL card/preview information
 */
export interface URLCardContent {
  readonly url: string;
  readonly title?: string;
  readonly description?: string;
  readonly siteName?: string;
  readonly imageUrl?: string;
  readonly imageWidth?: number;
  readonly imageHeight?: number;
  readonly videoUrl?: string;
  readonly audioUrl?: string;
  readonly author?: string;
  readonly publishedTime?: string;
  readonly tags?: ReadonlyArray<string>;
  readonly color?: string; // Hex color for embed border
}

/**
 * Media content (images, videos, audio)
 */
export interface MediaContent {
  readonly mediaId: string;
  readonly url: string;
  readonly mimeType: string;
  readonly fileName: string;
  readonly fileSize: number;
  readonly width?: number;
  readonly height?: number;
  readonly duration?: number; // For video/audio in seconds
  readonly thumbnailUrl?: string;
  readonly altText?: string; // Accessibility
  readonly caption?: string;
  readonly isAnimated?: boolean; // For GIFs
  readonly hasAudio?: boolean; // For videos
}

/**
 * File attachment information
 */
export interface FileAttachmentContent {
  readonly fileId: string;
  readonly fileName: string;
  readonly fileSize: number;
  readonly mimeType: string;
  readonly downloadUrl: string;
  readonly thumbnailUrl?: string;
  readonly previewUrl?: string; // For documents that can be previewed
  readonly isPreviewable: boolean;
  readonly uploadedBy: string;
  readonly uploadedAt: string;
  readonly virusScanStatus?: 'pending' | 'clean' | 'infected' | 'error';
  readonly description?: string;
}

/**
 * Interactive component configuration
 */
export interface InteractiveComponent {
  readonly componentId: string;
  readonly componentType: 'poll' | 'button' | 'select' | 'modal' | 'form' | 'custom';
  readonly title?: string;
  readonly description?: string;
  readonly configuration: Record<string, any>; // Component-specific config
  readonly permissions?: {
    readonly allowedUsers?: ReadonlyArray<string>;
    readonly allowedRoles?: ReadonlyArray<string>;
    readonly requiresAuth?: boolean;
  };
  readonly expiresAt?: string; // ISO timestamp
  readonly maxInteractions?: number;
  readonly currentInteractions?: number;
}

/**
 * System message content
 */
export interface SystemMessageContent {
  readonly systemType: 'user_joined' | 'user_left' | 'room_created' | 'settings_changed' | 'error' | 'notification';
  readonly systemData: Record<string, any>;
  readonly iconUrl?: string;
  readonly actionable?: boolean; // Can users interact with this system message
  readonly priority: 'low' | 'normal' | 'high' | 'urgent';
}

/**
 * Base chat message payload interface - extensible like JTAGPayload
 */
export interface BaseChatMessagePayload {
  readonly contentType: ChatContentType;
  readonly messageId: string;
  readonly timestamp: string;
  readonly editedAt?: string;
  readonly editHistory?: ReadonlyArray<{
    readonly editedAt: string;
    readonly editedBy: string;
    readonly reason?: string;
    readonly previousContent?: any;
  }>;
  
  // Threading support
  readonly threadId?: string;
  readonly parentMessageId?: string;
  readonly isThreadRoot?: boolean;
  
  // Reactions and interactions
  readonly reactions?: ReadonlyArray<{
    readonly emoji: string;
    readonly count: number;
    readonly userIds: ReadonlyArray<string>;
    readonly isCustomEmoji?: boolean;
    readonly emojiUrl?: string;
  }>;
  
  // Message metadata
  readonly isPinned?: boolean;
  readonly isDeleted?: boolean;
  readonly isEdited?: boolean;
  readonly isSpoiler?: boolean;
  readonly flags?: ReadonlyArray<'nsfw' | 'urgent' | 'announcement' | 'bot'>;
  
  // References and replies
  readonly referencedMessages?: ReadonlyArray<string>; // Message IDs this references
  readonly forwardedFrom?: {
    readonly originalMessageId: string;
    readonly originalRoomId: string;
    readonly originalAuthor: string;
  };
}

/**
 * Text content payload
 */
export interface TextChatMessagePayload extends BaseChatMessagePayload {
  readonly contentType: 'text';
  readonly content: string;
}

/**
 * Rich text content payload with formatting
 */
export interface RichTextChatMessagePayload extends BaseChatMessagePayload {
  readonly contentType: 'rich_text';
  readonly content: string;
  readonly formatting: TextFormatting;
}

/**
 * URL card content payload
 */
export interface URLCardChatMessagePayload extends BaseChatMessagePayload {
  readonly contentType: 'url_card';
  readonly content: string; // Original message text
  readonly urlCard: URLCardContent;
}

/**
 * Image content payload
 */
export interface ImageChatMessagePayload extends BaseChatMessagePayload {
  readonly contentType: 'image';
  readonly content?: string; // Optional caption text
  readonly media: MediaContent;
}

/**
 * Video content payload
 */
export interface VideoChatMessagePayload extends BaseChatMessagePayload {
  readonly contentType: 'video';
  readonly content?: string; // Optional caption text
  readonly media: MediaContent;
}

/**
 * Audio content payload
 */
export interface AudioChatMessagePayload extends BaseChatMessagePayload {
  readonly contentType: 'audio';
  readonly content?: string; // Optional caption text
  readonly media: MediaContent;
}

/**
 * File attachment payload
 */
export interface FileAttachmentChatMessagePayload extends BaseChatMessagePayload {
  readonly contentType: 'file_attachment';
  readonly content?: string; // Optional description text
  readonly attachment: FileAttachmentContent;
}

/**
 * HTML content payload (for rich custom content)
 */
export interface HTMLContentChatMessagePayload extends BaseChatMessagePayload {
  readonly contentType: 'html_content';
  readonly content: string; // Fallback text content
  readonly html: string;
  readonly sanitized: boolean; // Whether HTML has been sanitized
  readonly allowedTags?: ReadonlyArray<string>;
  readonly allowedAttributes?: Record<string, ReadonlyArray<string>>;
}

/**
 * Interactive component payload
 */
export interface InteractiveComponentChatMessagePayload extends BaseChatMessagePayload {
  readonly contentType: 'interactive_component';
  readonly content?: string; // Optional description text
  readonly component: InteractiveComponent;
}

/**
 * System message payload
 */
export interface SystemChatMessagePayload extends BaseChatMessagePayload {
  readonly contentType: 'system_message';
  readonly content: string; // Human-readable system message
  readonly system: SystemMessageContent;
}

/**
 * Custom payload for extensibility
 */
export interface CustomChatMessagePayload extends BaseChatMessagePayload {
  readonly contentType: 'custom';
  readonly content: string; // Fallback text content
  readonly customType: string; // Application-specific type identifier
  readonly customData: Record<string, any>; // Application-specific data
}

/**
 * Union type for all chat message payloads
 */
export type ChatMessagePayload = 
  | TextChatMessagePayload
  | RichTextChatMessagePayload
  | URLCardChatMessagePayload
  | ImageChatMessagePayload
  | VideoChatMessagePayload
  | AudioChatMessagePayload
  | FileAttachmentChatMessagePayload
  | HTMLContentChatMessagePayload
  | InteractiveComponentChatMessagePayload
  | SystemChatMessagePayload
  | CustomChatMessagePayload;

/**
 * Type guards for payload discrimination
 */
export const ChatMessagePayloadGuards = {
  isText: (payload: ChatMessagePayload): payload is TextChatMessagePayload => 
    payload.contentType === 'text',
  
  isRichText: (payload: ChatMessagePayload): payload is RichTextChatMessagePayload => 
    payload.contentType === 'rich_text',
  
  isURLCard: (payload: ChatMessagePayload): payload is URLCardChatMessagePayload => 
    payload.contentType === 'url_card',
  
  isImage: (payload: ChatMessagePayload): payload is ImageChatMessagePayload => 
    payload.contentType === 'image',
  
  isVideo: (payload: ChatMessagePayload): payload is VideoChatMessagePayload => 
    payload.contentType === 'video',
  
  isAudio: (payload: ChatMessagePayload): payload is AudioChatMessagePayload => 
    payload.contentType === 'audio',
  
  isFileAttachment: (payload: ChatMessagePayload): payload is FileAttachmentChatMessagePayload => 
    payload.contentType === 'file_attachment',
  
  isHTMLContent: (payload: ChatMessagePayload): payload is HTMLContentChatMessagePayload => 
    payload.contentType === 'html_content',
  
  isInteractiveComponent: (payload: ChatMessagePayload): payload is InteractiveComponentChatMessagePayload => 
    payload.contentType === 'interactive_component',
  
  isSystemMessage: (payload: ChatMessagePayload): payload is SystemChatMessagePayload => 
    payload.contentType === 'system_message',
  
  isCustom: (payload: ChatMessagePayload): payload is CustomChatMessagePayload => 
    payload.contentType === 'custom',
  
  hasMedia: (payload: ChatMessagePayload): payload is ImageChatMessagePayload | VideoChatMessagePayload | AudioChatMessagePayload => 
    payload.contentType === 'image' || payload.contentType === 'video' || payload.contentType === 'audio',
  
  hasFormatting: (payload: ChatMessagePayload): payload is RichTextChatMessagePayload => 
    payload.contentType === 'rich_text',
  
  isThreadable: (payload: ChatMessagePayload): boolean => 
    payload.contentType !== 'system_message' && !payload.isDeleted,
  
  isReactable: (payload: ChatMessagePayload): boolean => 
    payload.contentType !== 'system_message' && !payload.isDeleted
};

/**
 * Payload factory functions for creating messages
 */
export const ChatMessagePayloadFactory = {
  createTextMessage: (messageId: string, content: string, timestamp?: string): TextChatMessagePayload => ({
    contentType: 'text',
    messageId,
    content,
    timestamp: timestamp || new Date().toISOString()
  }),
  
  createRichTextMessage: (
    messageId: string, 
    content: string, 
    formatting: TextFormatting, 
    timestamp?: string
  ): RichTextChatMessagePayload => ({
    contentType: 'rich_text',
    messageId,
    content,
    formatting,
    timestamp: timestamp || new Date().toISOString()
  }),
  
  createImageMessage: (
    messageId: string, 
    media: MediaContent, 
    caption?: string, 
    timestamp?: string
  ): ImageChatMessagePayload => ({
    contentType: 'image',
    messageId,
    content: caption,
    media,
    timestamp: timestamp || new Date().toISOString()
  }),
  
  createURLCardMessage: (
    messageId: string, 
    content: string, 
    urlCard: URLCardContent, 
    timestamp?: string
  ): URLCardChatMessagePayload => ({
    contentType: 'url_card',
    messageId,
    content,
    urlCard,
    timestamp: timestamp || new Date().toISOString()
  }),
  
  createSystemMessage: (
    messageId: string, 
    content: string, 
    systemType: SystemMessageContent['systemType'], 
    systemData: Record<string, any>,
    priority: SystemMessageContent['priority'] = 'normal',
    timestamp?: string
  ): SystemChatMessagePayload => ({
    contentType: 'system_message',
    messageId,
    content,
    system: {
      systemType,
      systemData,
      priority
    },
    timestamp: timestamp || new Date().toISOString()
  }),
  
  createCustomMessage: (
    messageId: string, 
    content: string, 
    customType: string, 
    customData: Record<string, any>, 
    timestamp?: string
  ): CustomChatMessagePayload => ({
    contentType: 'custom',
    messageId,
    content,
    customType,
    customData,
    timestamp: timestamp || new Date().toISOString()
  })
};

/**
 * Utility functions for payload manipulation
 */
export const ChatMessagePayloadUtils = {
  /**
   * Extract plain text content from any payload type
   */
  extractTextContent: (payload: ChatMessagePayload): string => {
    return payload.content || '';
  },
  
  /**
   * Get content type display name
   */
  getContentTypeDisplayName: (contentType: ChatContentType): string => {
    const displayNames: Record<ChatContentType, string> = {
      'text': 'Text',
      'rich_text': 'Rich Text',
      'url_card': 'Link Preview',
      'image': 'Image',
      'video': 'Video',
      'audio': 'Audio',
      'file_attachment': 'File',
      'html_content': 'HTML Content',
      'interactive_component': 'Interactive Element',
      'system_message': 'System Message',
      'custom': 'Custom Content'
    };
    return displayNames[contentType];
  },
  
  /**
   * Check if payload contains media that requires loading
   */
  requiresMediaLoading: (payload: ChatMessagePayload): boolean => {
    return ChatMessagePayloadGuards.hasMedia(payload) || 
           ChatMessagePayloadGuards.isURLCard(payload) ||
           ChatMessagePayloadGuards.isFileAttachment(payload);
  },
  
  /**
   * Get file size for payloads with files
   */
  getFileSize: (payload: ChatMessagePayload): number | null => {
    if (ChatMessagePayloadGuards.hasMedia(payload)) {
      return payload.media.fileSize;
    }
    if (ChatMessagePayloadGuards.isFileAttachment(payload)) {
      return payload.attachment.fileSize;
    }
    return null;
  },
  
};