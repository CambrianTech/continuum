/**
 * ContentTypeRegistry - Maps content types to widget components
 *
 * This is the "React Router" of the main panel - determines which widget
 * renders based on the current tab's content type.
 *
 * ARCHITECTURE: Recipe JSON files are THE source of truth for layouts.
 * This registry provides:
 * 1. Dynamic lookup from RecipeLayoutService (loaded from recipes)
 * 2. Fallback defaults for content types without recipes
 *
 * When adding a new content type:
 * 1. Create a recipe JSON file in system/recipes/{contentType}.json
 * 2. Define layout.widgets array with position/order (new format)
 * 3. The registry will automatically pick it up
 * 4. Only add to FALLBACK_REGISTRY if recipe doesn't exist yet
 */

import { getRecipeLayoutService } from '../../../system/recipes/browser/RecipeLayoutService';

/**
 * Right panel configuration - matches recipe.layout.rightPanel
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
 * Fallback registry for content types without recipe JSON files
 *
 * PREFER: Create a recipe JSON in system/recipes/{contentType}.json
 * This fallback is only for content types that haven't been migrated yet.
 */
const FALLBACK_REGISTRY: Record<string, ContentTypeConfig> = {
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
  // Theme room assistant for color scheme suggestions
  theme: {
    widget: 'theme-widget',
    displayName: 'Theme',
    pathPrefix: '/theme',
    requiresEntity: false,
    defaultTitle: 'Theme',
    rightPanel: { widget: 'chat-widget', room: 'theme', compact: true },
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
    widget: 'web-view-widget',
    displayName: 'Browser',
    pathPrefix: '/browser',
    requiresEntity: false,
    defaultTitle: 'Browser',
    rightPanel: { widget: 'chat-widget', room: 'help', compact: true },
  },

  // Drawing canvas (collaborative)
  // Chat room for discussing the drawing with vision-capable AIs
  canvas: {
    widget: 'drawing-canvas-widget',
    displayName: 'Canvas',
    pathPrefix: '/canvas',
    requiresEntity: false,
    defaultTitle: 'Drawing Canvas',
    rightPanel: { widget: 'chat-widget', room: 'canvas', compact: false },
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

  // User profile - universal view for all user types
  // Edit, freeze, delete actions + links to cognitive views for AI
  profile: {
    widget: 'user-profile-widget',
    displayName: 'Profile',
    pathPrefix: '/profile',
    requiresEntity: true,  // Needs userId (uniqueId or UUID)
    rightPanel: { widget: 'chat-widget', room: 'help', compact: true },
  },
};

/**
 * Get widget tag name for a content type
 * Checks RecipeLayoutService first, then falls back to static registry
 */
export function getWidgetForType(contentType: string): string {
  // 1. Try recipe-driven layout first
  const recipeService = getRecipeLayoutService();
  if (recipeService.isLoaded()) {
    const recipeWidget = recipeService.getWidget(contentType);
    if (recipeWidget) {
      return recipeWidget;
    }
  }

  // 2. Fall back to static registry
  const config = FALLBACK_REGISTRY[contentType];
  if (!config) {
    console.warn(`Unknown content type: ${contentType}, falling back to chat-widget`);
    return 'chat-widget';
  }
  return config.widget;
}

/**
 * Get content type config
 * Checks RecipeLayoutService first, then falls back to static registry
 */
export function getContentTypeConfig(contentType: string): ContentTypeConfig | undefined {
  // 1. Try recipe-driven layout first
  const recipeService = getRecipeLayoutService();
  if (recipeService.isLoaded() && recipeService.hasRecipe(contentType)) {
    // Use RecipeLayoutService's methods which handle both old and new formats
    const widget = recipeService.getWidget(contentType);
    const rightPanel = recipeService.getRightPanel(contentType);

    if (widget) {
      return {
        widget,
        displayName: recipeService.getDisplayName(contentType) || contentType,
        pathPrefix: `/${contentType}`,
        requiresEntity: false,
        rightPanel: rightPanel ?? undefined
      };
    }
  }

  // 2. Fall back to static registry
  return FALLBACK_REGISTRY[contentType];
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

  // Check static registry (has pathPrefix definitions)
  for (const [type, config] of Object.entries(FALLBACK_REGISTRY)) {
    if (normalized.startsWith(config.pathPrefix)) {
      const remainder = normalized.slice(config.pathPrefix.length);
      const entityId = remainder.startsWith('/') ? remainder.slice(1) : undefined;
      return { type, entityId: entityId || undefined };
    }
  }

  // Check recipe content types (use /{uniqueId} as path)
  const recipeService = getRecipeLayoutService();
  if (recipeService.isLoaded()) {
    for (const type of recipeService.getAllContentTypes()) {
      const pathPrefix = `/${type}`;
      if (normalized.startsWith(pathPrefix)) {
        const remainder = normalized.slice(pathPrefix.length);
        const entityId = remainder.startsWith('/') ? remainder.slice(1) : undefined;
        return { type, entityId: entityId || undefined };
      }
    }
  }

  // Default to chat
  return { type: 'chat', entityId: undefined };
}

/**
 * Build URL path from content type and entity
 */
export function buildContentPath(contentType: string, entityId?: string): string {
  const config = FALLBACK_REGISTRY[contentType];
  const pathPrefix = config?.pathPrefix || `/${contentType}`;

  if (entityId) {
    return `${pathPrefix}/${entityId}`;
  }
  return pathPrefix;
}

/**
 * Get right panel configuration for a content type
 * Returns null if right panel should be hidden
 * Returns config object if right panel should be shown
 */
export function getRightPanelConfig(contentType: string): RightPanelConfig | null {
  // 1. Try recipe-driven layout first
  const recipeService = getRecipeLayoutService();
  const isLoaded = recipeService.isLoaded();
  const hasRecipe = recipeService.hasRecipe(contentType);
  console.log(`🔍 getRightPanelConfig('${contentType}'): isLoaded=${isLoaded}, hasRecipe=${hasRecipe}`);

  if (isLoaded && hasRecipe) {
    const rightPanel = recipeService.getRightPanel(contentType);
    console.log(`🔍 getRightPanelConfig('${contentType}'): recipe rightPanel=`, rightPanel);
    if (rightPanel === null) return null;
    if (rightPanel) return rightPanel;
  }

  // 2. Fall back to static registry
  console.log(`🔍 getRightPanelConfig('${contentType}'): using FALLBACK_REGISTRY`);
  const config = FALLBACK_REGISTRY[contentType];
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

/**
 * Initialize recipe layouts from server
 * Call this early in app startup (e.g., MainWidget init)
 */
export async function initializeRecipeLayouts(): Promise<void> {
  const recipeService = getRecipeLayoutService();
  await recipeService.loadLayouts();
}

/**
 * Export the fallback registry for debugging/introspection
 * (Use the functions above for actual lookups)
 */
export const CONTENT_TYPE_REGISTRY = FALLBACK_REGISTRY;
