/**
 * ContentTypes Debug Command Types
 *
 * Debug command for inspecting and managing ContentType registry
 * Helps debug content type configurations and widget mappings
 */

import type { CommandParams, CommandResult, CommandInput } from '@system/core/types/JTAGTypes';
import { Commands } from '@system/core/shared/Commands';
import { createPayload, transformPayload } from '@system/core/types/JTAGTypes';
import type { JTAGContext } from '@system/core/types/JTAGTypes';
import { SYSTEM_SCOPES } from '@system/core/types/SystemScopes';
import type { UUID } from '@system/core/types/CrossPlatformUUID';

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

/**
 * Factory function for creating DevelopmentDebugContentTypesParams
 */
export const createDevelopmentDebugContentTypesParams = (
  context: JTAGContext,
  sessionId: UUID,
  data: Omit<ContentTypesDebugParams, 'context' | 'sessionId' | 'userId'>
): ContentTypesDebugParams => createPayload(context, sessionId, {
  userId: SYSTEM_SCOPES.SYSTEM,
  ...data
});

/**
 * Factory function for creating DevelopmentDebugContentTypesResult with defaults
 */
export const createDevelopmentDebugContentTypesResult = (
  context: JTAGContext,
  sessionId: UUID,
  data: Omit<ContentTypesDebugResult, 'context' | 'sessionId' | 'userId'>
): ContentTypesDebugResult => createPayload(context, sessionId, {
  ...data
});

/**
 * Smart development/debug/content-types-specific inheritance from params
 * Auto-inherits context and sessionId from params
 * Must provide all required result fields
 */
export const createDevelopmentDebugContentTypesResultFromParams = (
  params: ContentTypesDebugParams,
  differences: Omit<ContentTypesDebugResult, 'context' | 'sessionId' | 'userId'>
): ContentTypesDebugResult => transformPayload(params, differences);

