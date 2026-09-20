/**
 * PersonaMediaConfig - Configuration for multimodal media handling
 *
 * Controls whether a PersonaUser receives media (images, audio, video) in tool results.
 * Default is opt-in (false) to avoid forcing all AIs to load media.
 */

import type { MediaType } from '../../../data/entities/ChatMessageEntity';

/**
 * Media configuration for PersonaUser
 */
export interface PersonaMediaConfig {
  /**
   * Whether to automatically load media from tool results
   * @default false (opt-in)
   */
  autoLoadMedia: boolean;

  /**
   * Whether to request media by default (for specialized AIs like CSS Designer)
   * @default false
   */
  requestMediaByDefault: boolean;

  /**
   * Supported media types for this persona
   * @default ['image', 'audio', 'video', 'file', 'document']
   */
  supportedMediaTypes: readonly MediaType[];
}

/**
 * Default media config (opt-out of media loading)
 */
export const DEFAULT_MEDIA_CONFIG: PersonaMediaConfig = {
  autoLoadMedia: false,
  requestMediaByDefault: false,
  supportedMediaTypes: ['image', 'audio', 'video', 'file', 'document']
};

/**
 * CSS Designer AI media config (always receives images for visual feedback)
 */
export const CSS_DESIGNER_MEDIA_CONFIG: PersonaMediaConfig = {
  autoLoadMedia: true,
  requestMediaByDefault: true,
  supportedMediaTypes: ['image']  // Only screenshots
};

/**
 * Vision-capable AI media config (can process images but doesn't auto-load)
 */
export const VISION_CAPABLE_MEDIA_CONFIG: PersonaMediaConfig = {
  autoLoadMedia: false,  // Manual opt-in
  requestMediaByDefault: false,
  supportedMediaTypes: ['image']
};

/**
 * Audio-processing AI media config
 */
export const AUDIO_PROCESSING_MEDIA_CONFIG: PersonaMediaConfig = {
  autoLoadMedia: true,
  requestMediaByDefault: true,
  supportedMediaTypes: ['audio']
};
