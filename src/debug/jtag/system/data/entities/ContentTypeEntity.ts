/**
 * ContentType Registry Entity - Dynamic Content Type Management
 *
 * Defines available content types that can be opened in the UI
 * Replaces hardcoded content types with database-backed registry
 * Enables dynamic content type registration and configuration
 */


// Widget selector types for content rendering
export type WidgetSelector = 'chat-widget' | 'user-list-widget' | 'room-list-widget' | 'user-profile-widget' | 'debug-widget' | 'data-explorer-widget';

// Content category for organization
export type ContentCategory = 'communication' | 'management' | 'development' | 'configuration' | 'system';

export interface ContentTypeConfig {
  // Widget rendering configuration
  widgetSelector: WidgetSelector;
  widgetConfig?: Record<string, unknown>;

  // UI appearance
  iconClass?: string;
  colorTheme?: string;
  defaultTitle?: string;

  // Behavior configuration
  allowMultiple: boolean;      // Can user have multiple instances open?
  autoSave: boolean;          // Should state be auto-saved?
  preloadData: boolean;       // Should data be preloaded?

  // Permission requirements
  requiredPermissions?: string[];
  minUserType?: 'human' | 'agent' | 'persona' | 'system';

  // Lifecycle hooks
  onCreate?: string;          // Command to run when content is opened
  onClose?: string;           // Command to run when content is closed
  onFocus?: string;           // Command to run when content gains focus
}

import {
  TextField,
  EnumField,
  JsonField,
  BooleanField,
  NumberField,
  TEXT_LENGTH
} from '../decorators/FieldDecorators';
import { BaseEntity } from './BaseEntity';

/**
 * ContentType Registry Entity - Dynamic content type definitions
 *
 * Manages available content types and their configurations
 * Enables runtime registration of new content types without code changes
 */
export class ContentTypeEntity extends BaseEntity {
  // Single source of truth for collection name
  static readonly collection = 'ContentType';

  @TextField({ index: true })
  type: string; // Primary content type identifier (e.g., 'chat', 'user-profile')

  @TextField({ maxLength: TEXT_LENGTH.MEDIUM })
  displayName: string; // Human-readable name for UI

  @TextField({ maxLength: TEXT_LENGTH.LONG, nullable: true })
  description?: string; // Optional description

  @EnumField({ index: true })
  category: ContentCategory;

  @JsonField()
  config: ContentTypeConfig;

  @BooleanField()
  isActive: boolean; // Can this content type be used?

  @BooleanField()
  isBuiltIn: boolean; // Is this a core system content type?

  @NumberField()
  sortOrder: number; // Display order in UI

  // Index signature for compatibility
  [key: string]: unknown;

  constructor() {
    super(); // Initialize BaseEntity fields

    // Default values
    this.type = '';
    this.displayName = '';
    this.description = undefined;
    this.category = 'communication';
    this.config = {
      widgetSelector: 'chat-widget',
      allowMultiple: false,
      autoSave: true,
      preloadData: false
    };
    this.isActive = true;
    this.isBuiltIn = false;
    this.sortOrder = 0;
  }

  /**
   * Implement BaseEntity abstract method
   */
  get collection(): string {
    return ContentTypeEntity.collection;
  }

  /**
   * Implement BaseEntity abstract method - validate content type data
   */
  validate(): { success: boolean; error?: string } {
    // Required fields validation
    if (!this.type?.trim()) {
      return { success: false, error: 'ContentType type is required' };
    }

    // Type format validation (kebab-case)
    const typeRegex = /^[a-z][a-z0-9]*(-[a-z0-9]+)*$/;
    if (!typeRegex.test(this.type)) {
      return { success: false, error: 'ContentType type must be kebab-case (e.g., "chat", "user-profile")' };
    }

    if (!this.displayName?.trim()) {
      return { success: false, error: 'ContentType displayName is required' };
    }

    if (this.displayName.length > 100) {
      return { success: false, error: 'ContentType displayName must be 100 characters or less' };
    }

    // Category validation
    const validCategories: ContentCategory[] = ['communication', 'management', 'development', 'configuration', 'system'];
    if (!validCategories.includes(this.category)) {
      return { success: false, error: `ContentType category must be one of: ${validCategories.join(', ')}` };
    }

    // Config validation
    if (!this.config) {
      return { success: false, error: 'ContentType config is required' };
    }

    const validWidgetSelectors: WidgetSelector[] = [
      'chat-widget', 'user-list-widget', 'room-list-widget',
      'user-profile-widget', 'debug-widget', 'data-explorer-widget'
    ];
    if (!validWidgetSelectors.includes(this.config.widgetSelector)) {
      return { success: false, error: `ContentType config.widgetSelector must be one of: ${validWidgetSelectors.join(', ')}` };
    }

    if (typeof this.config.allowMultiple !== 'boolean') {
      return { success: false, error: 'ContentType config.allowMultiple must be a boolean' };
    }

    if (typeof this.config.autoSave !== 'boolean') {
      return { success: false, error: 'ContentType config.autoSave must be a boolean' };
    }

    if (typeof this.config.preloadData !== 'boolean') {
      return { success: false, error: 'ContentType config.preloadData must be a boolean' };
    }

    // Sort order validation
    if (typeof this.sortOrder !== 'number' || this.sortOrder < 0) {
      return { success: false, error: 'ContentType sortOrder must be a non-negative number' };
    }

    return { success: true };
  }

  /**
   * Check if user can access this content type
   */
  canUserAccess(userType: 'human' | 'agent' | 'persona' | 'system', permissions: string[]): boolean {
    if (!this.isActive) {
      return false;
    }

    // Check minimum user type requirement
    if (this.config.minUserType) {
      const userTypeHierarchy = { human: 0, agent: 1, persona: 2, system: 3 };
      const requiredLevel = userTypeHierarchy[this.config.minUserType];
      const userLevel = userTypeHierarchy[userType];

      if (userLevel < requiredLevel) {
        return false;
      }
    }

    // Check required permissions
    if (this.config.requiredPermissions) {
      const hasAllPermissions = this.config.requiredPermissions.every(permission =>
        permissions.includes(permission)
      );
      if (!hasAllPermissions) {
        return false;
      }
    }

    return true;
  }

  /**
   * Get widget configuration for rendering this content type
   */
  getWidgetConfig(): { selector: WidgetSelector; config: Record<string, unknown> } {
    return {
      selector: this.config.widgetSelector,
      config: this.config.widgetConfig || {}
    };
  }

  /**
   * Create a default content type for core system types
   */
  static createBuiltInContentType(
    type: string,
    displayName: string,
    widgetSelector: WidgetSelector,
    category: ContentCategory = 'communication',
    config: Partial<ContentTypeConfig> = {}
  ): ContentTypeEntity {
    const contentType = new ContentTypeEntity();
    contentType.type = type;
    contentType.displayName = displayName;
    contentType.category = category;
    contentType.config = {
      widgetSelector,
      allowMultiple: false,
      autoSave: true,
      preloadData: false,
      ...config
    };
    contentType.isBuiltIn = true;
    contentType.isActive = true;

    return contentType;
  }
}