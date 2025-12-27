/**
 * RecipeLayoutService - Browser-side service for recipe-driven UI layouts
 *
 * Loads recipes from server and provides layout lookup for ContentTypeRegistry.
 * This replaces hardcoded widget mappings with dynamic recipe-driven composition.
 */

import { Commands } from '../../core/shared/Commands';
import type { ActivityUILayout, RightPanelConfig } from '../shared/RecipeTypes';

interface RecipeLayoutData {
  uniqueId: string;
  displayName: string;
  layout?: ActivityUILayout;
}

/**
 * Browser-side singleton service for recipe layouts
 */
export class RecipeLayoutService {
  private static instance: RecipeLayoutService;
  private layouts: Map<string, RecipeLayoutData> = new Map();
  private loaded = false;
  private loading: Promise<void> | null = null;

  private constructor() {}

  static getInstance(): RecipeLayoutService {
    if (!RecipeLayoutService.instance) {
      RecipeLayoutService.instance = new RecipeLayoutService();
    }
    return RecipeLayoutService.instance;
  }

  /**
   * Load all recipe layouts from server
   */
  async loadLayouts(): Promise<void> {
    if (this.loaded) return;
    if (this.loading) return this.loading;

    this.loading = this.doLoad();
    await this.loading;
  }

  private async doLoad(): Promise<void> {
    try {
      console.log('📚 RecipeLayoutService: Loading recipe layouts via data/list...');

      // Query recipes directly from database to get full entity including layout
      const result = await Commands.execute('data/list', {
        collection: 'recipes',
        limit: 100
      } as any) as unknown as { items?: RecipeLayoutData[]; success?: boolean; error?: string };

      console.log('📚 RecipeLayoutService: data/list result:', result);

      if (result?.items && result.items.length > 0) {
        for (const recipe of result.items) {
          if (recipe.uniqueId) {
            console.log(`📚 RecipeLayoutService: Adding recipe '${recipe.uniqueId}' with layout:`, recipe.layout);
            this.layouts.set(recipe.uniqueId, {
              uniqueId: recipe.uniqueId,
              displayName: recipe.displayName || recipe.uniqueId,
              layout: recipe.layout
            });
          }
        }
        console.log(`✅ RecipeLayoutService: Loaded ${this.layouts.size} recipe layouts: ${Array.from(this.layouts.keys()).join(', ')}`);
      } else {
        console.warn('⚠️ RecipeLayoutService: No recipes returned from data/list', result);
      }

      this.loaded = true;
    } catch (error) {
      console.error('❌ RecipeLayoutService: Failed to load layouts:', error);
      this.loaded = true; // Mark as loaded to prevent infinite retries
    }
  }

  /**
   * Get layout for a content type (recipe uniqueId)
   */
  getLayout(contentType: string): ActivityUILayout | undefined {
    const recipe = this.layouts.get(contentType);
    return recipe?.layout;
  }

  /**
   * Get widget tag for a content type
   * Handles both new format (main: string[]) and deprecated (mainWidget: string)
   */
  getWidget(contentType: string): string | undefined {
    const layout = this.getLayout(contentType);
    if (!layout) return undefined;

    // New format: main can be string[] or PanelConfig
    if (layout.main) {
      if (Array.isArray(layout.main)) {
        return layout.main[0]; // First widget is primary
      } else if (typeof layout.main === 'object' && 'widgets' in layout.main) {
        return layout.main.widgets[0];
      }
    }

    // Deprecated format fallback
    return layout.mainWidget;
  }

  /**
   * Get right panel config for a content type
   * Handles both new format (right: string[] | PanelConfig) and deprecated (rightPanel)
   */
  getRightPanel(contentType: string): RightPanelConfig | null | undefined {
    const layout = this.getLayout(contentType);
    if (!layout) return undefined;

    // New format: right can be string[], PanelConfig, or null
    if ('right' in layout) {
      if (layout.right === null) {
        return null; // Explicitly hidden
      }
      if (Array.isArray(layout.right)) {
        // Convert string[] to RightPanelConfig
        return {
          widget: layout.right[0],
          compact: true
        };
      }
      if (typeof layout.right === 'object' && 'widgets' in layout.right) {
        // Convert PanelConfig to RightPanelConfig
        const panelConfig = layout.right as { widgets: string[]; config?: Record<string, unknown> };
        return {
          widget: panelConfig.widgets[0],
          compact: true,
          ...panelConfig.config
        };
      }
    }

    // Deprecated format fallback
    return layout.rightPanel;
  }

  /**
   * Get display name for a content type
   */
  getDisplayName(contentType: string): string | undefined {
    const recipe = this.layouts.get(contentType);
    return recipe?.displayName;
  }

  /**
   * Check if a content type has a recipe
   */
  hasRecipe(contentType: string): boolean {
    return this.layouts.has(contentType);
  }

  /**
   * Get all loaded content types
   */
  getAllContentTypes(): string[] {
    return Array.from(this.layouts.keys());
  }

  /**
   * Check if layouts have been loaded
   */
  isLoaded(): boolean {
    return this.loaded;
  }
}

// Export singleton getter
export const getRecipeLayoutService = () => RecipeLayoutService.getInstance();
