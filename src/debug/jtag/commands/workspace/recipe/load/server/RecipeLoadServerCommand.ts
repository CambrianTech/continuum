/**
 * Recipe Load Server Command
 *
 * Loads recipe JSON files from system/recipes/*.json into database
 */

import { RecipeLoadCommand } from '../shared/RecipeLoadCommand';
import { DATA_COMMANDS } from '@commands/data/shared/DataCommandConstants';
import type { JTAGContext } from '@system/core/types/JTAGTypes';
import type { ICommandDaemon } from '@daemons/command-daemon/shared/CommandBase';
import type { RecipeLoadParams, RecipeLoadResult } from '../shared/RecipeLoadTypes';
import type { RecipeDefinition } from '@system/recipes/shared/RecipeTypes';
import { RecipeEntity } from '@system/data/entities/RecipeEntity';
import type { DataListResult } from '@commands/data/list/shared/DataListTypes';
import type { DataListParams } from '@commands/data/list/shared/DataListTypes';
import type { DataCreateParams, DataCreateResult } from '@commands/data/create/shared/DataCreateTypes';
import type { DataUpdateParams, DataUpdateResult } from '@commands/data/update/shared/DataUpdateTypes';
import { Commands } from '@system/core/shared/Commands';
import * as fs from 'fs';
import * as path from 'path';
import { randomUUID } from 'crypto';

// Use process.cwd() to get consistent path from project root
const RECIPES_DIR = path.join(process.cwd(), 'system/recipes');
const COLLECTION = 'recipes'; // TODO: Add to DATABASE_CONFIG.COLLECTIONS

export class RecipeLoadServerCommand extends RecipeLoadCommand {
  constructor(context: JTAGContext, subpath: string, commander: ICommandDaemon) {
    super('workspace/recipe/load', context, subpath, commander);
  }

  async execute(params: RecipeLoadParams): Promise<RecipeLoadResult> {
    try {
      const loaded: RecipeEntity[] = [];
      const skipped: string[] = [];
      const errors: Array<{ recipeId: string; error: string }> = [];
      // Determine which recipes to load
      let recipeFiles: string[] = [];

      if (params.loadAll) {
        // Load all JSON files in recipes directory
        const files = fs.readdirSync(RECIPES_DIR);
        recipeFiles = files.filter(f => f.endsWith('.json'));
      } else if (params.recipeId) {
        // Load specific recipe
        recipeFiles = [`${params.recipeId}.json`];
      } else {
        return {
          context: params.context,
          sessionId: params.sessionId,
          success: false,
          loaded: [],
          errors: [{ recipeId: 'unknown', error: 'Must specify recipeId or loadAll=true' }]
        };
      }

      // Load each recipe file
      for (const filename of recipeFiles) {
        const recipeId = filename.replace('.json', '');

        try {
          // Read JSON file
          const filePath = path.join(RECIPES_DIR, filename);
          if (!fs.existsSync(filePath)) {
            errors.push({ recipeId, error: 'File not found' });
            continue;
          }

          const fileContent = fs.readFileSync(filePath, 'utf-8');
          const definition: RecipeDefinition = JSON.parse(fileContent);

          // Check if recipe already exists
          const existingResult = await Commands.execute<DataListParams, DataListResult<RecipeEntity>>(DATA_COMMANDS.LIST, {
            collection: COLLECTION,
            filter: { uniqueId: definition.uniqueId }
          } as Partial<DataListParams>);

          const now = new Date();

          if (existingResult.items && existingResult.items.length > 0) {
            // Recipe exists
            if (params.reload) {
              // Update existing recipe
              const updateResult = await Commands.execute<DataUpdateParams, DataUpdateResult<RecipeEntity>>(DATA_COMMANDS.UPDATE, {
                backend: 'server',
                collection: COLLECTION,
                id: existingResult.items[0].id,
                data: {
                  name: definition.name,
                  displayName: definition.displayName,
                  description: definition.description,
                  layout: definition.layout,
                  pipeline: definition.pipeline,
                  ragTemplate: definition.ragTemplate,
                  strategy: definition.strategy,
                  isPublic: definition.isPublic,
                  tags: definition.tags,
                  lastUsedAt: now
                }
              });

              if (updateResult.data) {
                loaded.push(updateResult.data);
              }
              console.log(`🔄 Recipe updated: ${recipeId}`);
            } else {
              // Skip existing recipe
              skipped.push(recipeId);
              console.log(`⏭️  Recipe skipped (already exists): ${recipeId}`);
            }
          } else {
            // Create new recipe
            const entity = new RecipeEntity();
            Object.assign(entity, {
              uniqueId: definition.uniqueId,
              name: definition.name,
              displayName: definition.displayName,
              description: definition.description,
              layout: definition.layout,
              pipeline: definition.pipeline,
              ragTemplate: definition.ragTemplate,
              strategy: definition.strategy,
              isPublic: definition.isPublic,
              createdBy: randomUUID(), // TODO: Get from system user
              tags: definition.tags,
              usageCount: 0,
              lastUsedAt: now
            });

            const createResult = await Commands.execute<DataCreateParams, DataCreateResult<RecipeEntity>>(DATA_COMMANDS.CREATE, {
              backend: 'server',
              collection: COLLECTION,
              data: entity
            });

            if (createResult.data) {
              loaded.push(createResult.data);
            }
            console.log(`✅ Recipe created: ${recipeId}`);
          }
        } catch (error) {
          errors.push({
            recipeId,
            error: error instanceof Error ? error.message : String(error)
          });
          console.error(`❌ Failed to load recipe ${recipeId}:`, error);
        }
      }

      return {
        context: params.context,
        sessionId: params.sessionId,
        success: errors.length === 0,
        loaded,
        skipped: skipped.length > 0 ? skipped : undefined,
        errors: errors.length > 0 ? errors : undefined
      };
    } catch (error) {
      console.error('❌ Recipe load command failed:', error);
      return {
        context: params.context,
        sessionId: params.sessionId,
        success: false,
        loaded: [],
        errors: [{
          recipeId: 'system',
          error: error instanceof Error ? error.message : String(error)
        }]
      };
    }
  }
}
