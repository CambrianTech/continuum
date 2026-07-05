/**
 * ContentTypeRegistry — Maps content types to widgets and layouts.
 *
 * Source of truth: Recipe JSON files (system/recipes/*.json)
 * Generated types: shared/generated/ContentTypes.ts
 *
 * NO FALLBACK REGISTRY. If a content type has no recipe, it fails visibly.
 * Add a recipe JSON file, run the generator. That's it.
 */

import { getRecipeLayoutService } from '../../../system/recipes/browser/RecipeLayoutService';
import {
    CONTENT_TYPE_CONFIGS,
    isContentType,
    type ContentType,
    type ContentTypeConfig,
} from '@shared/generated/ContentTypes';

import type { RightPanelSectionPayload } from '../../../system/core/shared/EventConstants';

export type RightPanelSection = RightPanelSectionPayload;

// Single source of truth for RightPanelConfig — from RecipeTypes
import type { RightPanelConfig as _RightPanelConfig } from '../../../system/recipes/shared/RecipeTypes';
export type RightPanelConfig = _RightPanelConfig;

// Re-export generated types for consumers
export { ContentType, ContentTypeConfig, isContentType, CONTENT_TYPE_CONFIGS };

/**
 * Get widget tag name for a content type.
 * Recipe service (runtime) → generated config (build-time) → error.
 */
export function getWidgetForType(contentType: string): string {
    // 1. Generated config FIRST — always available, no async, no race conditions
    // This is the compiled-in truth from recipe JSON files.
    const generated = CONTENT_TYPE_CONFIGS[contentType as ContentType];
    if (generated) return generated.widget;

    // 2. Recipe service — runtime data from DB (may have newer recipes)
    const recipeService = getRecipeLayoutService();
    if (recipeService.isLoaded()) {
        const widget = recipeService.getWidget(contentType);
        if (widget) return widget;
    }

    // 3. No match — unknown type. Fail visibly. No fallbacks.
    throw new Error(`Unknown content type: '${contentType}'. Add a recipe in system/recipes/${contentType}.json and run the generator.`);
}

/**
 * Get full config for a content type.
 */
export function getContentTypeConfig(contentType: string): ContentTypeConfig | undefined {
    // 1. Recipe service
    const recipeService = getRecipeLayoutService();
    if (recipeService.isLoaded() && recipeService.hasRecipe(contentType)) {
        const widget = recipeService.getWidget(contentType);
        const rightPanel = recipeService.getRightPanel(contentType);
        if (widget) {
            const generated = CONTENT_TYPE_CONFIGS[contentType as ContentType];
            return {
                widget,
                displayName: recipeService.getDisplayName(contentType) || contentType,
                icon: generated?.icon || '📄',
                view: generated?.view || contentType,
                requiresEntity: generated?.requiresEntity || false,
                entityType: generated?.entityType || null,
                hasRightPanel: rightPanel !== null && rightPanel !== undefined,
                rightPanelWidget: generated?.rightPanelWidget || (rightPanel?.widget ?? null),
                rightPanelRoom: generated?.rightPanelRoom || null,
            };
        }
    }

    // 2. Generated config
    return CONTENT_TYPE_CONFIGS[contentType as ContentType];
}

/**
 * Parse URL path → content type + entity uniqueId.
 * Matches by view prefix. Returns the recipe's content type, not the view.
 *
 * /chat/general → { type: 'general-chat', entityId: 'general' }  (first 'chat' view match)
 * /live/general → { type: 'live', entityId: 'general' }
 * /factory      → { type: 'factory' }
 */
export function parseContentPath(path: string): { type?: string; entityId?: string } {
    const normalized = path.startsWith('/') ? path : `/${path}`;

    // Match by view — sort longest first to prevent /grid matching before /grid-overview
    const entries = Object.entries(CONTENT_TYPE_CONFIGS)
        .sort((a, b) => (b[1].view?.length || 0) - (a[1].view?.length || 0));

    for (const [type, config] of entries) {
        const viewPrefix = `/${config.view || type}`;
        if (normalized === viewPrefix || normalized.startsWith(viewPrefix + '/')) {
            const remainder = normalized.slice(viewPrefix.length);
            const entityId = remainder.startsWith('/') ? remainder.slice(1) : undefined;
            return { type, entityId: entityId || undefined };
        }
    }

    // Fallback: match by recipe uniqueId (backwards compat)
    for (const [type] of Object.entries(CONTENT_TYPE_CONFIGS)) {
        const prefix = `/${type}`;
        if (normalized === prefix || normalized.startsWith(prefix + '/')) {
            const remainder = normalized.slice(prefix.length);
            const entityId = remainder.startsWith('/') ? remainder.slice(1) : undefined;
            return { type, entityId: entityId || undefined };
        }
    }

    // Joel 2026-05-03: was `return { type: 'chat', ... }` — silent default
    // that opened a phantom General tab on every unknown path. No match =
    // no tab. Callers must handle undefined type explicitly.
    return { type: undefined, entityId: undefined };
}

/**
 * Build URL path from content type + entity uniqueId.
 * Uses view field for URL prefix (verb/noun).
 *
 * ('general-chat', 'general') → '/chat/general'
 * ('live', 'general')         → '/live/general'
 * ('factory')                 → '/factory'
 */
export function buildContentPath(contentType: string, entityId?: string): string {
    const config = CONTENT_TYPE_CONFIGS[contentType as ContentType];
    const view = config?.view || contentType;
    return entityId ? `/${view}/${entityId}` : `/${view}`;
}

/**
 * Get right panel configuration for a content type.
 */
export function getRightPanelConfig(contentType: string): RightPanelConfig | null {
    // 1. Recipe service is the SOURCE OF TRUTH for right panel config.
    // Always check recipes first, even if generated config says hasRightPanel: false.
    const recipeService = getRecipeLayoutService();
    if (recipeService.isLoaded() && recipeService.hasRecipe(contentType)) {
        const rightPanel = recipeService.getRightPanel(contentType);
        if (rightPanel === null) return null;  // Recipe explicitly hides right panel
        if (rightPanel) return rightPanel;     // Recipe provides right panel config
    }

    // 2. Generated config — includes right panel widget and room from recipe
    const config = CONTENT_TYPE_CONFIGS[contentType as ContentType];
    if (!config?.hasRightPanel || !config.rightPanelWidget) return null;

    return {
        widget: config.rightPanelWidget,
        room: config.rightPanelRoom || undefined,
        compact: true,
    } as RightPanelConfig;
}

/**
 * Initialize recipe layouts from server.
 * Call early in app startup (MainWidget init).
 */
export async function initializeRecipeLayouts(): Promise<void> {
    const recipeService = getRecipeLayoutService();
    await recipeService.loadLayouts();
}

/**
 * @deprecated Use CONTENT_TYPE_CONFIGS from generated types instead.
 * Kept temporarily for consumers that haven't migrated.
 */
export const CONTENT_TYPE_REGISTRY = CONTENT_TYPE_CONFIGS;
