/**
 * ContentTypeRegistry - Maps content types to widget components
 *
 * This is the "React Router" of the main panel - determines which widget
 * renders based on the current tab's content type.
 *
 * IMPORTANT: Right panel configs here mirror recipe JSON layouts.
 * Recipe JSONs are the source of truth for AI behavior, this registry
 * is the source of truth for browser-side UI composition.
 */

/**
 * Right panel configuration - mirrors recipe.layout.rightPanel
 */
export interface RightPanelConfig {
  /** Widget to display (default: 'chat-widget') */
  widget?: string;
  /** For chat-widget: which room to connect to */
  room?: string;
  /** Display in compact mode for sidebar use */
  compact?: boolean;
}

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
  /** Right panel configuration. null = hidden, undefined = inherit default */
  rightPanel?: RightPanelConfig | null;
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
  // Chat IS the main content, no right panel needed
  chat: {
    widget: 'chat-widget',
    displayName: 'Chat',
    pathPrefix: '/chat',
    requiresEntity: true,  // Needs roomId
    rightPanel: null,  // Hide right panel for chat (chat IS the content)
  },

  // Settings - config.env editor, API keys
  // Help assistant in right panel for configuration guidance
  settings: {
    widget: 'settings-widget',
    displayName: 'Settings',
    pathPrefix: '/settings',
    requiresEntity: false,
    defaultTitle: 'Settings',
    rightPanel: { widget: 'chat-widget', room: 'help', compact: true },
  },

  // Theme customization
  // Help assistant for color scheme suggestions
  theme: {
    widget: 'theme-widget',
    displayName: 'Theme',
    pathPrefix: '/theme',
    requiresEntity: false,
    defaultTitle: 'Theme',
    rightPanel: { widget: 'chat-widget', room: 'help', compact: true },
  },

  // Help & onboarding
  // No right panel - help IS the main content
  help: {
    widget: 'help-widget',
    displayName: 'Help',
    pathPrefix: '/help',
    requiresEntity: false,
    defaultTitle: 'Help',
    rightPanel: null,
  },

  // Persona brain/cognitive view
  // Help assistant for understanding persona state
  persona: {
    widget: 'persona-brain-widget',
    displayName: 'Persona',
    pathPrefix: '/persona',
    requiresEntity: true,  // Needs userId (uniqueId)
    rightPanel: { widget: 'chat-widget', room: 'help', compact: true },
  },

  // Web browser (collaborative)
  // Help assistant for browser usage
  browser: {
    widget: 'browser-widget',
    displayName: 'Browser',
    pathPrefix: '/browser',
    requiresEntity: false,
    defaultTitle: 'Browser',
    rightPanel: { widget: 'chat-widget', room: 'help', compact: true },
  },

  // System diagnostics and persona logs
  // Help assistant for log interpretation
  diagnostics: {
    widget: 'diagnostics-widget',
    displayName: 'Diagnostics',
    pathPrefix: '/diagnostics',
    requiresEntity: false,
    defaultTitle: 'System Diagnostics',
    rightPanel: { widget: 'chat-widget', room: 'help', compact: true },
  },

  // Individual log viewer (opened from diagnostics)
  // Help assistant for understanding log entries
  'diagnostics-log': {
    widget: 'log-viewer-widget',
    displayName: 'Log',
    pathPrefix: '/log',
    requiresEntity: true,  // Needs log file path
    rightPanel: { widget: 'chat-widget', room: 'help', compact: true },
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

/**
 * Get right panel configuration for a content type
 * Returns null if right panel should be hidden
 * Returns config object if right panel should be shown
 */
export function getRightPanelConfig(contentType: string): RightPanelConfig | null {
  const config = CONTENT_TYPE_REGISTRY[contentType];
  if (!config) {
    // Unknown content type - default to showing help panel
    return { widget: 'chat-widget', room: 'help', compact: true };
  }

  // null means explicitly hidden, undefined means use default
  if (config.rightPanel === null) {
    return null;
  }

  // Return the config or default help panel
  return config.rightPanel || { widget: 'chat-widget', room: 'help', compact: true };
}
