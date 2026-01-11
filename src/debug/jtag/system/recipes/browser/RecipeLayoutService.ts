/**
 * RecipeLayoutService - Browser-side service for recipe-driven UI layouts
 *
 * Loads recipes from server and provides layout lookup for ContentTypeRegistry.
 * This replaces hardcoded widget mappings with dynamic recipe-driven composition.
 *
 * Supports two layout formats:
 * 1. OLD format (ActivityUILayout): { main: string[], right: PanelConfig }
 * 2. NEW format (LayoutConfig): { widgets: [{ widget, position, order, config }] }
 */

import { Commands } from '../../core/shared/Commands';
import type { ActivityUILayout, RightPanelConfig } from '../shared/RecipeTypes';

/**
 * New-format widget with position enum
 */
interface LayoutWidget {
  widget: string;
  position: 'left' | 'center' | 'right';
  order: number;
  config?: Record<string, unknown>;
}

/**
 * New-format layout config
 */
interface NewFormatLayout {
  widgets: LayoutWidget[];
  panels?: {
    left?: { visible: boolean };
    center?: { visible: boolean };
    right?: { visible: boolean };
  };
}

interface RecipeLayoutData {
  uniqueId: string;
  displayName: string;
  layout?: ActivityUILayout | NewFormatLayout;
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
    const verbose = typeof window !== 'undefined' && (window as any).JTAG_VERBOSE === true;
    try {
      if (verbose) console.log('📚 RecipeLayoutService: Loading recipe layouts via data/list...');

      // Query recipes with field projection - only fetch what we need
      const result = await Commands.execute('data/list', {
        collection: 'recipes',
        limit: 100,
        fields: ['uniqueId', 'displayName', 'layout'] // Only fetch layout-relevant fields
      } as any) as unknown as { items?: RecipeLayoutData[]; success?: boolean; error?: string };

      if (verbose) console.log('📚 RecipeLayoutService: data/list result:', result);

      if (result?.items && result.items.length > 0) {
        for (const recipe of result.items) {
          if (recipe.uniqueId) {
            if (verbose) console.log(`📚 RecipeLayoutService: Adding recipe '${recipe.uniqueId}' with layout:`, recipe.layout);
            this.layouts.set(recipe.uniqueId, {
              uniqueId: recipe.uniqueId,
              displayName: recipe.displayName || recipe.uniqueId,
              layout: recipe.layout
            });
          }
        }
        if (verbose) console.log(`✅ RecipeLayoutService: Loaded ${this.layouts.size} recipe layouts: ${Array.from(this.layouts.keys()).join(', ')}`);
      } else {
        if (verbose) console.warn('⚠️ RecipeLayoutService: No recipes returned from data/list', result);
      }

      this.loaded = true;
    } catch (error) {
      console.error('❌ RecipeLayoutService: Failed to load layouts:', error);
      this.loaded = true; // Mark as loaded to prevent infinite retries
    }
  }

  /**
   * Check if layout uses the new widgets array format
   */
  private isNewFormat(layout: ActivityUILayout | NewFormatLayout): layout is NewFormatLayout {
    return 'widgets' in layout && Array.isArray((layout as NewFormatLayout).widgets);
  }

  /**
   * Get widgets for a position from new-format layout
   */
  private getWidgetsForPosition(layout: NewFormatLayout, position: 'left' | 'center' | 'right'): LayoutWidget[] {
    return layout.widgets
      .filter(w => w.position === position)
      .sort((a, b) => a.order - b.order);
  }

  /**
   * Get layout for a content type (recipe uniqueId)
   */
  getLayout(contentType: string): ActivityUILayout | NewFormatLayout | undefined {
    const recipe = this.layouts.get(contentType);
    return recipe?.layout;
  }

  /**
   * Get widget tag for a content type
   * Handles:
   * - New format: { widgets: [{ widget, position: 'center' }] }
   * - Old format: main: string[] or PanelConfig
   * - Deprecated: mainWidget: string
   */
  getWidget(contentType: string): string | undefined {
    const layout = this.getLayout(contentType);
    if (!layout) return undefined;

    // NEW format: widgets array with position enum
    if (this.isNewFormat(layout)) {
      const centerWidgets = this.getWidgetsForPosition(layout, 'center');
      return centerWidgets[0]?.widget;
    }

    // OLD format: main can be string[] or PanelConfig
    const oldLayout = layout as ActivityUILayout;
    if (oldLayout.main) {
      if (Array.isArray(oldLayout.main)) {
        return oldLayout.main[0]; // First widget is primary
      } else if (typeof oldLayout.main === 'object' && 'widgets' in oldLayout.main) {
        return oldLayout.main.widgets[0];
      }
    }

    // Deprecated format fallback
    return oldLayout.mainWidget;
  }

  /**
   * Get right panel config for a content type
   * Handles:
   * - New format: { widgets: [{ widget, position: 'right', config: { room, compact } }] }
   * - Old format: right: string[] | PanelConfig | null
   * - Deprecated: rightPanel
   */
  getRightPanel(contentType: string): RightPanelConfig | null | undefined {
    const layout = this.getLayout(contentType);
    if (!layout) return undefined;

    // NEW format: widgets array with position enum
    if (this.isNewFormat(layout)) {
      // Check if right panel is explicitly hidden
      if (layout.panels?.right?.visible === false) {
        return null;
      }

      const rightWidgets = this.getWidgetsForPosition(layout, 'right');
      if (rightWidgets.length === 0) {
        return null; // No right panel widgets = hidden
      }

      const firstRight = rightWidgets[0];
      return {
        widget: firstRight.widget,
        room: firstRight.config?.room as string | undefined,
        compact: firstRight.config?.compact as boolean | undefined,
        ...firstRight.config
      };
    }

    // OLD format: right can be string[], PanelConfig, or null
    const oldLayout = layout as ActivityUILayout;
    if ('right' in oldLayout) {
      if (oldLayout.right === null) {
        return null; // Explicitly hidden
      }
      if (Array.isArray(oldLayout.right)) {
        // Convert string[] to RightPanelConfig
        return {
          widget: oldLayout.right[0],
          compact: true
        };
      }
      if (typeof oldLayout.right === 'object' && 'widgets' in oldLayout.right) {
        // Convert PanelConfig to RightPanelConfig
        const panelConfig = oldLayout.right as { widgets: string[]; config?: Record<string, unknown> };
        return {
          widget: panelConfig.widgets[0],
          compact: true,
          ...panelConfig.config
        };
      }
    }

    // Deprecated format fallback
    return oldLayout.rightPanel;
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
