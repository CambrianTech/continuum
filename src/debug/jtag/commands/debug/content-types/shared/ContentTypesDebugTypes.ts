/**
 * ContentTypes Debug Command Types
 *
 * Debug command for inspecting and managing ContentType registry
 * Helps debug content type configurations and widget mappings
 */

export interface ContentTypesDebugParams {
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

export interface ContentTypesDebugResult {
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