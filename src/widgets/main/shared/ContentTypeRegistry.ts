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
} from '../../../shared/generated/ContentTypes';

import type { RightPanelSectionPayload } from '../../../system/core/shared/EventConstants';

export type RightPanelSection = RightPanelSectionPayload;

export interface RightPanelConfig {
    widget?: string;
    room?: string;
    compact?: boolean;
    sections?: RightPanelSection[];
}

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

    // 3. No match — unknown type
    console.error(`Unknown content type: '${contentType}'. Add a recipe in system/recipes/${contentType}.json`);
    return 'chat-widget'; // Graceful degradation to prevent blank screen
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
                pathPrefix: `/${contentType}`,
                requiresEntity: generated?.requiresEntity || false,
                entityType: generated?.entityType || null,
                hasRightPanel: rightPanel !== null && rightPanel !== undefined,
            };
        }
    }

    // 2. Generated config
    return CONTENT_TYPE_CONFIGS[contentType as ContentType];
}

/**
 * Parse URL path to content type and entity.
 */
export function parseContentPath(path: string): { type: string; entityId?: string } {
    const normalized = path.startsWith('/') ? path : `/${path}`;

    // Check generated configs (all have pathPrefix = /{uniqueId})
    for (const [type, config] of Object.entries(CONTENT_TYPE_CONFIGS)) {
        if (normalized.startsWith(config.pathPrefix)) {
            const remainder = normalized.slice(config.pathPrefix.length);
            const entityId = remainder.startsWith('/') ? remainder.slice(1) : undefined;
            return { type, entityId: entityId || undefined };
        }
    }

    // Check recipe service for types not in generated config (shouldn't happen, but safe)
    const recipeService = getRecipeLayoutService();
    if (recipeService.isLoaded()) {
        for (const type of recipeService.getAllContentTypes()) {
            const prefix = `/${type}`;
            if (normalized.startsWith(prefix)) {
                const remainder = normalized.slice(prefix.length);
                const entityId = remainder.startsWith('/') ? remainder.slice(1) : undefined;
                return { type, entityId: entityId || undefined };
            }
        }
    }

    return { type: 'chat', entityId: undefined };
}

/**
 * Build URL path from content type and entity.
 */
export function buildContentPath(contentType: string, entityId?: string): string {
    const config = CONTENT_TYPE_CONFIGS[contentType as ContentType];
    const pathPrefix = config?.pathPrefix || `/${contentType}`;
    return entityId ? `${pathPrefix}/${entityId}` : pathPrefix;
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

    // 2. Generated config — only use if recipe service has no opinion
    const config = CONTENT_TYPE_CONFIGS[contentType as ContentType];
    if (config && !config.hasRightPanel) return null;

    // 3. No config found — return null, NOT a default chat widget.
    // If a recipe wants a right panel, it declares one. No guessing.
    return null;
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
