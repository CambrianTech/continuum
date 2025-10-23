/**
 * Recipe Loader - Loads recipe JSON files and caches them
 *
 * Recipes define conversation governance rules that AIs follow
 */

import * as fs from 'fs';
import * as path from 'path';
import type { RecipeDefinition } from '../shared/RecipeTypes';

/**
 * Recipe Loader - Singleton for loading and caching recipes
 */
export class RecipeLoader {
  private static instance: RecipeLoader;
  private recipes: Map<string, RecipeDefinition> = new Map();
  private recipesDir: string;

  private constructor() {
    // Recipes are in system/recipes/*.json
    this.recipesDir = path.join(__dirname, '..');
  }

  static getInstance(): RecipeLoader {
    if (!RecipeLoader.instance) {
      RecipeLoader.instance = new RecipeLoader();
    }
    return RecipeLoader.instance;
  }

  /**
   * Load a recipe by uniqueId
   */
  async loadRecipe(uniqueId: string): Promise<RecipeDefinition | null> {
    // Check cache first
    if (this.recipes.has(uniqueId)) {
      return this.recipes.get(uniqueId)!;
    }

    try {
      // Load from JSON file
      const jsonPath = path.join(this.recipesDir, `${uniqueId}.json`);

      if (!fs.existsSync(jsonPath)) {
        console.warn(`⚠️ Recipe not found: ${uniqueId} (expected at ${jsonPath})`);
        return null;
      }

      const jsonContent = fs.readFileSync(jsonPath, 'utf-8');
      const recipe: RecipeDefinition = JSON.parse(jsonContent);

      // Validate structure
      if (!recipe.uniqueId || !recipe.pipeline || !recipe.strategy) {
        console.error(`❌ Invalid recipe structure: ${uniqueId}`);
        return null;
      }

      // Cache and return
      this.recipes.set(uniqueId, recipe);
      console.log(`✅ Loaded recipe: ${recipe.displayName} (${uniqueId})`);
      return recipe;
    } catch (error) {
      console.error(`❌ Failed to load recipe ${uniqueId}:`, error);
      return null;
    }
  }

  /**
   * Preload all recipes from directory
   */
  async preloadAllRecipes(): Promise<void> {
    try {
      const files = fs.readdirSync(this.recipesDir);
      const jsonFiles = files.filter(f => f.endsWith('.json'));

      console.log(`📚 Preloading ${jsonFiles.length} recipes from ${this.recipesDir}`);

      for (const file of jsonFiles) {
        const uniqueId = file.replace('.json', '');
        await this.loadRecipe(uniqueId);
      }

      console.log(`✅ Preloaded ${this.recipes.size} recipes`);
    } catch (error) {
      console.error('❌ Failed to preload recipes:', error);
    }
  }

  /**
   * Get all loaded recipes
   */
  getAllRecipes(): RecipeDefinition[] {
    return Array.from(this.recipes.values());
  }

  /**
   * Clear cache (for testing)
   */
  clearCache(): void {
    this.recipes.clear();
  }
}
