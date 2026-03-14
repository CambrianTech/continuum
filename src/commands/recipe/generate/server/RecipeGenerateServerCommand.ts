/**
 * Recipe Generate Command — LLM-powered recipe creation from natural language.
 *
 * Flow:
 * 1. Build a schema-aware system prompt with examples
 * 2. Call LLM with the user's natural language description
 * 3. Parse and validate the generated JSON
 * 4. Save to system/recipes/<uniqueId>.json (unless dryRun)
 */

import * as fs from 'fs';
import * as path from 'path';
import { CommandBase, type ICommandDaemon } from '../../../../daemons/command-daemon/shared/CommandBase';
import type { JTAGContext, JTAGPayload } from '../../../../system/core/types/JTAGTypes';
import { transformPayload } from '../../../../system/core/types/JTAGTypes';
import type { RecipeGenerateParams, RecipeGenerateResult } from '../shared/RecipeGenerateTypes';
import type { RecipeDefinition } from '../../../../system/recipes/shared/RecipeTypes';
import { AIProviderDaemon } from '../../../../daemons/ai-provider-daemon/shared/AIProviderDaemon';
import { TemplateRegistry } from '../../../../system/sentinel/pipelines/TemplateRegistry';
import { RecipeLoader } from '../../../../system/recipes/server/RecipeLoader';

export class RecipeGenerateServerCommand extends CommandBase<RecipeGenerateParams, RecipeGenerateResult> {
  constructor(context: JTAGContext, subpath: string, commander: ICommandDaemon) {
    super('recipe/generate', context, subpath, commander);
  }

  async execute(params: JTAGPayload): Promise<RecipeGenerateResult> {
    const genParams = params as RecipeGenerateParams;
    const { description, dryRun = false, hints, provider = 'anthropic' } = genParams;

    if (!description || description.trim().length === 0) {
      return transformPayload(params, {
        success: false,
        error: 'Description is required. Describe the recipe you want in natural language.',
      });
    }

    // 1. Build the generation prompt
    const systemPrompt = this.buildSystemPrompt();
    const userPrompt = this.buildUserPrompt(description, hints);

    // 2. Call LLM
    try {
      const response = await AIProviderDaemon.generateText({
        messages: [
          { role: 'system', content: systemPrompt },
          { role: 'user', content: userPrompt },
        ],
        model: genParams.model || this.defaultModelForProvider(provider),
        provider,
        temperature: 0.4,
        maxTokens: 4000,
      });

      // 3. Parse JSON from response
      const jsonMatch = response.text.match(/\{[\s\S]*\}/);
      if (!jsonMatch) {
        return transformPayload(params, {
          success: false,
          error: 'LLM did not return valid JSON. Raw response saved for debugging.',
          validationErrors: [`Raw output: ${response.text.slice(0, 500)}`],
        });
      }

      let recipe: RecipeDefinition;
      try {
        recipe = JSON.parse(jsonMatch[0]) as RecipeDefinition;
      } catch (parseError) {
        return transformPayload(params, {
          success: false,
          error: 'LLM returned malformed JSON.',
          validationErrors: [
            parseError instanceof Error ? parseError.message : String(parseError),
            `Raw JSON: ${jsonMatch[0].slice(0, 500)}`,
          ],
        });
      }

      // 4. Apply uniqueId override
      if (genParams.uniqueId) {
        recipe.uniqueId = genParams.uniqueId;
      }

      // 5. Validate
      const validationErrors = this.validateRecipe(recipe);
      if (validationErrors.length > 0) {
        return transformPayload(params, {
          success: false,
          recipe,
          validationErrors,
          error: `Generated recipe has ${validationErrors.length} validation error(s).`,
        });
      }

      // 6. Save (unless dryRun)
      let savedTo: string | undefined;
      if (!dryRun) {
        savedTo = this.saveRecipe(recipe);

        // Reload into cache
        const loader = RecipeLoader.getInstance();
        loader.clearCache();
        await loader.loadRecipe(recipe.uniqueId);
      }

      return transformPayload(params, {
        success: true,
        recipe,
        savedTo,
      });
    } catch (error) {
      return transformPayload(params, {
        success: false,
        error: error instanceof Error ? error.message : String(error),
      });
    }
  }

  private buildSystemPrompt(): string {
    // Gather available templates for reference
    const templates = TemplateRegistry.list();
    const templateList = templates
      .map(t => `  - ${t.name}: ${t.description} (required: ${t.requiredFields.join(', ')})`)
      .join('\n');

    return `You are a recipe generator for the Continuum collaborative AI platform.

Your job is to generate a valid RecipeDefinition JSON object from a natural language description.

## RecipeDefinition Schema

\`\`\`typescript
interface RecipeDefinition {
  uniqueId: string;           // kebab-case identifier (e.g., "novel-writing", "data-analysis")
  name: string;               // Human-readable name
  displayName: string;        // Short display name (1-3 words)
  description: string;        // One-sentence description
  version: number;            // Always 1 for new recipes

  pipeline: RecipeStep[];     // Command execution pipeline
  ragTemplate: RAGTemplate;   // Context building config
  strategy: RecipeStrategy;   // AI behavior rules

  tools?: RecipeToolDeclaration[];  // Highlighted tools
  sentinelTemplates?: string[];     // Linked workflow templates
  roles?: RecipeRole[];             // Team role requirements

  layout?: {                  // UI layout (optional)
    main: string[];
    right?: string[] | null;
  };

  isPublic: boolean;          // Always true for generated recipes
  tags: string[];             // Categorization tags
}

interface RecipeStep {
  command: string;            // e.g., "rag/build", "ai/should-respond", "ai/generate"
  params: Record<string, unknown>;
  outputTo?: string;          // Variable name for next step
  condition?: string;         // JS expression for conditional execution
  onError?: "fail" | "skip" | "retry";
}

interface RAGTemplate {
  messageHistory: {
    maxMessages: number;      // 10-50 depending on activity
    orderBy: "chronological" | "relevance" | "importance";
    includeTimestamps: boolean;
  };
  participants?: {
    includeRoles: boolean;
    includeExpertise: boolean;
    includeHistory: boolean;
  };
  artifacts?: {
    types: string[];          // ["image", "code", "document"]
    maxItems: number;
    includeMetadata: boolean;
  };
  roomMetadata?: boolean;
  sources?: string[];         // RAG source names to activate
}

interface RecipeStrategy {
  conversationPattern: "human-focused" | "collaborative" | "competitive" | "teaching" | "exploring" | "cooperative";
  responseRules: string[];    // Behavioral rules for the AI
  decisionCriteria: string[]; // What to consider when deciding to respond
  feedbackLoopRules?: string[]; // Mandatory verification rules
}

type RecipeRoleType = "organizational" | "perceptual" | "creative";

interface RecipeRole {
  role: string;               // Role identifier
  type: RecipeRoleType;
  requires: string[];         // Required capabilities: "coding", "prose", "review", "planning", "research", "tool-use", "reasoning", "image-input", "audio-input"
  prefers?: string[];         // Preferred capabilities
  preferLocal?: boolean;
  description?: string;
}

interface RecipeToolDeclaration {
  name: string;               // Tool command name
  description: string;
  enabledFor: ("ai" | "human")[];
}
\`\`\`

## Available Sentinel Templates

${templateList}

## Standard Pipeline Pattern

Most recipes follow this pipeline:
1. \`rag/build\` — Build context from conversation
2. \`ai/should-respond\` — Decide if the AI should respond
3. \`ai/generate\` — Generate the response

## Rules

1. Output ONLY the JSON object — no markdown fences, no explanation
2. Every recipe MUST have a valid pipeline with at least the 3-step standard pattern
3. The uniqueId must be kebab-case, descriptive, and unique
4. responseRules should be specific and actionable — not vague platitudes
5. decisionCriteria should be questions the AI asks itself
6. feedbackLoopRules should be MANDATORY verification steps
7. If the recipe involves sentinel workflows, reference only templates from the available list above
8. roles.requires must use real capability names from the schema
9. tags should be lowercase, relevant keywords
10. version is always 1`;
  }

  private buildUserPrompt(description: string, hints?: RecipeGenerateParams['hints']): string {
    let prompt = `Generate a RecipeDefinition JSON for the following activity:\n\n${description}`;

    if (hints) {
      const hintParts: string[] = [];
      if (hints.category) hintParts.push(`Category: ${hints.category}`);
      if (hints.templates?.length) hintParts.push(`Use templates: ${hints.templates.join(', ')}`);
      if (hints.tags?.length) hintParts.push(`Tags: ${hints.tags.join(', ')}`);
      if (hints.pattern) hintParts.push(`Conversation pattern: ${hints.pattern}`);

      if (hintParts.length > 0) {
        prompt += `\n\nHints:\n${hintParts.map(h => `- ${h}`).join('\n')}`;
      }
    }

    return prompt;
  }

  private validateRecipe(recipe: RecipeDefinition): string[] {
    const errors: string[] = [];

    // Required fields
    if (!recipe.uniqueId) errors.push('Missing uniqueId');
    if (!recipe.name) errors.push('Missing name');
    if (!recipe.displayName) errors.push('Missing displayName');
    if (!recipe.description) errors.push('Missing description');
    if (recipe.version === undefined) errors.push('Missing version');

    // uniqueId format
    if (recipe.uniqueId && !/^[a-z0-9-]+$/.test(recipe.uniqueId)) {
      errors.push(`uniqueId must be kebab-case: "${recipe.uniqueId}"`);
    }

    // Pipeline
    if (!recipe.pipeline || !Array.isArray(recipe.pipeline)) {
      errors.push('Missing or invalid pipeline array');
    } else if (recipe.pipeline.length === 0) {
      errors.push('Pipeline must have at least one step');
    } else {
      for (let i = 0; i < recipe.pipeline.length; i++) {
        const step = recipe.pipeline[i];
        if (!step.command) errors.push(`Pipeline step ${i}: missing command`);
        if (!step.params || typeof step.params !== 'object') {
          errors.push(`Pipeline step ${i}: missing or invalid params`);
        }
      }
    }

    // RAG template
    if (!recipe.ragTemplate) {
      errors.push('Missing ragTemplate');
    } else if (!recipe.ragTemplate.messageHistory) {
      errors.push('Missing ragTemplate.messageHistory');
    }

    // Strategy
    if (!recipe.strategy) {
      errors.push('Missing strategy');
    } else {
      if (!recipe.strategy.conversationPattern) {
        errors.push('Missing strategy.conversationPattern');
      }
      const validPatterns = ['human-focused', 'collaborative', 'competitive', 'teaching', 'exploring', 'cooperative'];
      if (recipe.strategy.conversationPattern && !validPatterns.includes(recipe.strategy.conversationPattern)) {
        errors.push(`Invalid conversationPattern: "${recipe.strategy.conversationPattern}". Must be one of: ${validPatterns.join(', ')}`);
      }
      if (!recipe.strategy.responseRules || !Array.isArray(recipe.strategy.responseRules)) {
        errors.push('Missing strategy.responseRules array');
      }
      if (!recipe.strategy.decisionCriteria || !Array.isArray(recipe.strategy.decisionCriteria)) {
        errors.push('Missing strategy.decisionCriteria array');
      }
    }

    // Sentinel templates — must exist in registry
    if (recipe.sentinelTemplates) {
      for (const tmpl of recipe.sentinelTemplates) {
        if (!TemplateRegistry.has(tmpl)) {
          errors.push(`sentinelTemplate "${tmpl}" is not registered. Available: ${TemplateRegistry.list().map(t => t.name).join(', ')}`);
        }
      }
    }

    // Roles validation
    if (recipe.roles) {
      const validRoleTypes = ['organizational', 'perceptual', 'creative'];
      for (const role of recipe.roles) {
        if (!role.role) errors.push('Role missing "role" field');
        if (!role.type || !validRoleTypes.includes(role.type)) {
          errors.push(`Role "${role.role}": invalid type "${role.type}". Must be: ${validRoleTypes.join(', ')}`);
        }
        if (!role.requires || !Array.isArray(role.requires) || role.requires.length === 0) {
          errors.push(`Role "${role.role}": must have at least one required capability`);
        }
      }
    }

    // isPublic must be boolean
    if (recipe.isPublic === undefined) {
      errors.push('Missing isPublic (must be boolean)');
    }

    // Tags must be array
    if (!recipe.tags || !Array.isArray(recipe.tags)) {
      errors.push('Missing or invalid tags array');
    }

    // Check for collision with existing recipes
    const loader = RecipeLoader.getInstance();
    const existing = loader.getAllRecipes();
    if (existing.some(r => r.uniqueId === recipe.uniqueId)) {
      errors.push(`Recipe with uniqueId "${recipe.uniqueId}" already exists. Use a different uniqueId or specify --uniqueId.`);
    }

    return errors;
  }

  private saveRecipe(recipe: RecipeDefinition): string {
    const recipesDir = path.join(__dirname, '..', '..', '..', '..', 'system', 'recipes');
    const filePath = path.join(recipesDir, `${recipe.uniqueId}.json`);
    const json = JSON.stringify(recipe, null, 2) + '\n';
    fs.writeFileSync(filePath, json, 'utf-8');
    return filePath;
  }

  private defaultModelForProvider(provider: string): string {
    switch (provider) {
      case 'anthropic': return 'claude-sonnet-4-5-20250929';
      case 'openai': return 'gpt-4o';
      case 'groq': return 'llama-3.3-70b-versatile';
      case 'deepseek': return 'deepseek-chat';
      case 'google': return 'gemini-2.5-flash';
      case 'xai': return 'grok-3';
      default: return 'claude-sonnet-4-5-20250929';
    }
  }
}
