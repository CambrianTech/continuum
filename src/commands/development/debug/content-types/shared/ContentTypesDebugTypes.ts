/**
 * ContentTypes Debug Command Types
 *
 * Debug command for inspecting and managing ContentType registry
 * Helps debug content type configurations and widget mappings
 */

import type { CommandParams, CommandResult, CommandInput } from '@system/core/types/JTAGTypes';
import { Commands } from '@system/core/shared/Commands';

export interface ContentTypesDebugParams extends CommandParams {
  listAll?: boolean;              // List all registered content types
  showInactive?: boolean;         // Include inactive content types
  contentType?: string;           // Inspect specific content type
  validateConfig?: boolean;       // Validate all content type configurations
  seedDefaults?: boolean;         // Seed default content types if missing
}

export interface ContentTypeInfo {
  type: string;
  displayName: string;
  category: string;
  widgetSelector: string;
  isActive: boolean;
  isBuiltIn: boolean;
  sortOrder: number;
  allowMultiple: boolean;
  autoSave: boolean;
  preloadData: boolean;
  requiredPermissions?: string[];
  minUserType?: string;
  issues?: string[];            // Configuration validation issues
}

export interface ContentTypesDebugResult extends CommandResult {
  totalTypes: number;
  activeTypes: number;
  inactiveTypes: number;
  builtInTypes: number;
  customTypes: number;
  contentTypes: ContentTypeInfo[];
  categories: {
    [category: string]: {
      count: number;
      types: string[];
    };
  };
  widgets: {
    [widgetSelector: string]: {
      count: number;
      types: string[];
    };
  };
  validationIssues: {
    type: string;
    issues: string[];
  }[];
  recommendations?: string[];
}

/**
 * DevelopmentDebugContentTypes — Type-safe command executor
 */
export const DevelopmentDebugContentTypes = {
  execute(params: CommandInput<ContentTypesDebugParams>): Promise<ContentTypesDebugResult> {
    return Commands.execute<ContentTypesDebugParams, ContentTypesDebugResult>(
      'development/debug/content-types', params as Partial<ContentTypesDebugParams>
    );
  },
  commandName: 'development/debug/content-types' as const,
} as const;
