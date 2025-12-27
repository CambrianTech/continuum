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
      console.log('📚 RecipeLayoutService: Loading recipe layouts...');

      const result = await Commands.execute('workspace/recipe/load', {
        loadAll: true
      } as any) as unknown as { success: boolean; loaded?: RecipeLayoutData[] };

      if (result?.loaded) {
        for (const recipe of result.loaded) {
          if (recipe.uniqueId) {
            this.layouts.set(recipe.uniqueId, {
              uniqueId: recipe.uniqueId,
              displayName: recipe.displayName || recipe.uniqueId,
              layout: recipe.layout
            });
          }
        }
        console.log(`✅ RecipeLayoutService: Loaded ${this.layouts.size} recipe layouts`);
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
   */
  getWidget(contentType: string): string | undefined {
    const layout = this.getLayout(contentType);
    return layout?.mainWidget;
  }

  /**
   * Get right panel config for a content type
   */
  getRightPanel(contentType: string): RightPanelConfig | null | undefined {
    const layout = this.getLayout(contentType);
    return layout?.rightPanel;
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
