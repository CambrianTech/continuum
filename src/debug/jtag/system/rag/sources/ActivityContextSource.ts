/**
 * ActivityContextSource - Recipe/activity context for guided interactions
 *
 * Provides strategy rules, decision criteria, and highlighted tools
 * when a recipe is active in the room.
 *
 * MEDIUM priority (40) - important for guided activities but not critical.
 */

import type { RAGSource, RAGSection, RAGSourceContext } from '../shared/RAGSource';
import type { RecipeStrategy } from '../shared/RAGTypes';
import type { RecipeToolDeclaration } from '../../recipes/shared/RecipeTypes';
import { ORM } from '../../../daemons/data-daemon/server/ORM';
import { RecipeLoader } from '../../recipes/server/RecipeLoader';
import { RoomEntity } from '../../data/entities/RoomEntity';
import { isSlowLocalModel } from '../../shared/ModelContextWindows';

/**
 * ActivityContextSource - Provides recipe/activity guidance
 *
 * Budget-aware: Truncates or skips for limited models.
 * Medium priority: Important for guided activities.
 */
export class ActivityContextSource implements RAGSource {
  readonly name = 'activity';

  // Medium priority - important for guided interactions
  readonly priority = 40;

  // Small budget allocation - activity context is supplementary
  readonly defaultBudgetPercent = 5;

  isApplicable(_context: RAGSourceContext): boolean {
    // Always attempt to load - we'll check for active recipes in load()
    return true;
  }

  async load(context: RAGSourceContext, allocatedBudget: number): Promise<RAGSection> {
    const startTime = Date.now();

    try {
      // Load room to check for active recipe
      const room = await ORM.read<RoomEntity>(RoomEntity.collection, context.roomId);
      if (!room?.recipeId) {
        return {
          sourceName: this.name,
          tokenCount: 0,
          loadTimeMs: Date.now() - startTime,
          systemPromptSection: undefined
        };
      }

      // Load recipe context - recipeId is the uniqueId string
      const recipeLoader = RecipeLoader.getInstance();
      const recipe = await recipeLoader.loadRecipe(room.recipeId);
      if (!recipe) {
        return {
          sourceName: this.name,
          tokenCount: 0,
          loadTimeMs: Date.now() - startTime,
          systemPromptSection: undefined
        };
      }

      // Check if model is limited
      const modelId = context.options?.modelId;
      const isLimited = modelId && isSlowLocalModel(modelId, context.provider);

      // Extract strategy from recipe (RecipeDefinition has strategy property)
      const strategy: RecipeStrategy | undefined = recipe.strategy;

      // Extract tools from recipe
      const tools: RecipeToolDeclaration[] = recipe.tools || [];

      // Build activity section
      let activitySection = '\n\n=== ACTIVITY CONTEXT ===';

      if (strategy) {
        activitySection += `\nActivity pattern: ${strategy.conversationPattern}`;

        // For limited models, only include essential rules
        if (!isLimited && strategy.responseRules.length > 0) {
          activitySection += '\n\nRules for this activity:\n' +
            strategy.responseRules.map((rule: string) => `- ${rule}`).join('\n');
        }

        if (!isLimited && strategy.decisionCriteria.length > 0) {
          activitySection += '\n\nWhen deciding whether to respond, consider:\n' +
            strategy.decisionCriteria.map((c: string) => `- ${c}`).join('\n');
        }
      }

      if (tools.length > 0) {
        const aiTools = tools.filter((t: RecipeToolDeclaration) => t.enabledFor.includes('ai'));
        if (aiTools.length > 0) {
          activitySection += '\n\nYOU MUST use these tools to do real work in this activity (call them directly):\n' +
            aiTools.map((t: RecipeToolDeclaration) => `- ${t.name}: ${t.description}`).join('\n') +
            '\n\nDo NOT just discuss or describe what should be done - call the tools above to actually do it.';
        }
      }

      activitySection += '\n================================';

      // Estimate tokens
      const tokenCount = Math.ceil(activitySection.length / 4);

      // Check budget
      if (tokenCount > allocatedBudget) {
        // Truncate or skip based on how much over budget
        if (allocatedBudget < 50) {
          return {
            sourceName: this.name,
            tokenCount: 0,
            loadTimeMs: Date.now() - startTime,
            systemPromptSection: undefined
          };
        }

        // Truncate to fit budget
        const targetChars = allocatedBudget * 4;
        activitySection = activitySection.slice(0, targetChars - 50) + '\n[truncated]\n================================';
      }

      return {
        sourceName: this.name,
        tokenCount: Math.ceil(activitySection.length / 4),
        loadTimeMs: Date.now() - startTime,
        systemPromptSection: activitySection,
        recipeStrategy: strategy
      };
    } catch (error) {
      // Don't fail the entire RAG build if activity context fails
      return {
        sourceName: this.name,
        tokenCount: 0,
        loadTimeMs: Date.now() - startTime,
        systemPromptSection: undefined
      };
    }
  }
}
