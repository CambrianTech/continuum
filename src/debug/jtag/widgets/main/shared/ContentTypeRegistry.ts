/**
 * ContentTypeRegistry - Maps content types to widget components
 *
 * This is the "React Router" of the main panel - determines which widget
 * renders based on the current tab's content type.
 */

export interface ContentTypeConfig {
  /** Widget tag name (e.g., 'chat-widget', 'settings-widget') */
  widget: string;
  /** Display name for UI */
  displayName: string;
  /** Icon (optional, for tab display) */
  icon?: string;
  /** URL path prefix (e.g., '/chat', '/settings') */
  pathPrefix: string;
  /** Whether this content type requires an entityId */
  requiresEntity: boolean;
  /** Default title when no entity specified */
  defaultTitle?: string;
}

/**
 * Registry of all supported content types
 *
 * To add a new content type:
 * 1. Add entry here with widget tag name
 * 2. Create the widget component
 * 3. Register widget in BROWSER_WIDGETS
 */
export const CONTENT_TYPE_REGISTRY: Record<string, ContentTypeConfig> = {
  // Chat rooms - the original content type
  chat: {
    widget: 'chat-widget',
    displayName: 'Chat',
    pathPrefix: '/chat',
    requiresEntity: true,  // Needs roomId
  },

  // Settings - config.env editor, API keys
  settings: {
    widget: 'settings-widget',
    displayName: 'Settings',
    pathPrefix: '/settings',
    requiresEntity: false,
    defaultTitle: 'Settings',
  },

  // Theme customization
  theme: {
    widget: 'theme-widget',
    displayName: 'Theme',
    pathPrefix: '/theme',
    requiresEntity: false,
    defaultTitle: 'Theme',
  },

  // Help & onboarding
  help: {
    widget: 'help-widget',
    displayName: 'Help',
    pathPrefix: '/help',
    requiresEntity: false,
    defaultTitle: 'Help',
  },

  // Persona brain/cognitive view
  persona: {
    widget: 'persona-brain-widget',
    displayName: 'Persona',
    pathPrefix: '/persona',
    requiresEntity: true,  // Needs userId (uniqueId)
  },

  // Web browser (collaborative)
  browser: {
    widget: 'browser-widget',
    displayName: 'Browser',
    pathPrefix: '/browser',
    requiresEntity: false,
    defaultTitle: 'Browser',
  },

  // System diagnostics and persona logs
  diagnostics: {
    widget: 'diagnostics-widget',
    displayName: 'Diagnostics',
    pathPrefix: '/diagnostics',
    requiresEntity: false,
    defaultTitle: 'System Diagnostics',
  },

  // Individual log viewer (opened from diagnostics)
  'diagnostics-log': {
    widget: 'log-viewer-widget',
    displayName: 'Log',
    pathPrefix: '/log',
    requiresEntity: true,  // Needs log file path
  },
};

/**
 * Get widget tag name for a content type
 */
export function getWidgetForType(contentType: string): string {
  const config = CONTENT_TYPE_REGISTRY[contentType];
  if (!config) {
    console.warn(`Unknown content type: ${contentType}, falling back to chat-widget`);
    return 'chat-widget';
  }
  return config.widget;
}

/**
 * Get content type config
 */
export function getContentTypeConfig(contentType: string): ContentTypeConfig | undefined {
  return CONTENT_TYPE_REGISTRY[contentType];
}

/**
 * Parse URL path to content type and entity
 *
 * Examples:
 *   /chat/general → { type: 'chat', entityId: 'general' }
 *   /settings → { type: 'settings', entityId: undefined }
 *   /persona/helper → { type: 'persona', entityId: 'helper' }
 */
export function parseContentPath(path: string): { type: string; entityId?: string } {
  const normalized = path.startsWith('/') ? path : `/${path}`;

  for (const [type, config] of Object.entries(CONTENT_TYPE_REGISTRY)) {
    if (normalized.startsWith(config.pathPrefix)) {
      const remainder = normalized.slice(config.pathPrefix.length);
      const entityId = remainder.startsWith('/') ? remainder.slice(1) : undefined;
      return { type, entityId: entityId || undefined };
    }
  }

  // Default to chat
  return { type: 'chat', entityId: undefined };
}

/**
 * Build URL path from content type and entity
 */
export function buildContentPath(contentType: string, entityId?: string): string {
  const config = CONTENT_TYPE_REGISTRY[contentType];
  if (!config) {
    return `/chat/${entityId || 'general'}`;
  }

  if (entityId) {
    return `${config.pathPrefix}/${entityId}`;
  }
  return config.pathPrefix;
}
