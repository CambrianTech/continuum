/**
 * Widget Registry - Centralized Widget Discovery and Management
 * Strongly typed registry for all widget instances and manifests
 */

import { 
  WidgetManifest, 
  WidgetRegistryEntry, 
  BaseWidgetInstance, 
  WidgetError,
  WidgetEvent,
  WidgetEventHandler,
  WidgetEventType,
  isWidgetManifest,
  isWidgetInstance
} from '../types/WidgetTypes';

export class WidgetRegistry {
  private static instance: WidgetRegistry;
  private widgets: Map<string, WidgetRegistryEntry> = new Map();
  private eventHandlers: Map<WidgetEventType, Set<WidgetEventHandler>> = new Map();
  private maxWidgets: number = 100;
  private developmentMode: boolean = false;

  private constructor() {
    // Initialize event handler maps
    const eventTypes: WidgetEventType[] = ['widget:created', 'widget:updated', 'widget:destroyed', 'widget:error'];
    eventTypes.forEach(type => {
      this.eventHandlers.set(type, new Set());
    });
  }

  public static getInstance(): WidgetRegistry {
    if (!WidgetRegistry.instance) {
      WidgetRegistry.instance = new WidgetRegistry();
    }
    return WidgetRegistry.instance;
  }

  /**
   * Register a widget with its manifest
   */
  public async registerWidget(manifest: WidgetManifest): Promise<string> {
    if (!isWidgetManifest(manifest)) {
      throw new WidgetError('Invalid widget manifest', 'unknown', 'INVALID_MANIFEST');
    }

    const widgetId = this.generateWidgetId(manifest.config.name);
    
    // Check if we're at capacity
    if (this.widgets.size >= this.maxWidgets) {
      throw new WidgetError('Maximum widget limit reached', widgetId, 'CAPACITY_EXCEEDED');
    }

    // Check for name conflicts
    if (this.isWidgetRegistered(manifest.config.name)) {
      if (!this.developmentMode) {
        throw new WidgetError('Widget name already registered', widgetId, 'NAME_CONFLICT');
      }
      // In development mode, unregister the old widget
      await this.unregisterWidget(manifest.config.name);
    }

    // Validate manifest
    if (!this.validateManifest(manifest)) {
      throw new WidgetError('Widget manifest validation failed', widgetId, 'VALIDATION_FAILED');
    }

    const entry: WidgetRegistryEntry = {
      manifest,
      registered: new Date(),
      active: true
    };

    this.widgets.set(widgetId, entry);
    
    // Emit registration event
    await this.emitEvent('widget:created', widgetId, { manifest });
    
    console.log(`📦 Widget registered: ${manifest.config.name} (${widgetId})`);
    return widgetId;
  }

  /**
   * Unregister a widget by name or ID
   */
  public async unregisterWidget(nameOrId: string): Promise<boolean> {
    const widgetId = this.findWidgetId(nameOrId);
    if (!widgetId) {
      return false;
    }

    const entry = this.widgets.get(widgetId);
    if (!entry) {
      return false;
    }

    // Cleanup widget instance if it exists
    if (entry.instance) {
      try {
        await entry.instance.cleanup();
      } catch (error) {
        console.warn(`⚠️ Widget cleanup failed for ${widgetId}:`, error);
      }
    }

    this.widgets.delete(widgetId);
    
    // Emit destruction event
    await this.emitEvent('widget:destroyed', widgetId, { name: entry.manifest.config.name });
    
    console.log(`🗑️ Widget unregistered: ${entry.manifest.config.name} (${widgetId})`);
    return true;
  }

  /**
   * Get widget by name or ID
   */
  public getWidget(nameOrId: string): WidgetRegistryEntry | undefined {
    const widgetId = this.findWidgetId(nameOrId);
    return widgetId ? this.widgets.get(widgetId) : undefined;
  }

  /**
   * Get all registered widgets
   */
  public getAllWidgets(): WidgetRegistryEntry[] {
    return Array.from(this.widgets.values());
  }

  /**
   * Get active widgets only
   */
  public getActiveWidgets(): WidgetRegistryEntry[] {
    return Array.from(this.widgets.values()).filter(entry => entry.active);
  }

  /**
   * Set widget instance for a registered widget
   */
  public setWidgetInstance(nameOrId: string, instance: BaseWidgetInstance): boolean {
    if (!isWidgetInstance(instance)) {
      throw new WidgetError('Invalid widget instance', nameOrId, 'INVALID_INSTANCE');
    }

    const widgetId = this.findWidgetId(nameOrId);
    if (!widgetId) {
      return false;
    }

    const entry = this.widgets.get(widgetId);
    if (!entry) {
      return false;
    }

    // Update the entry with the instance
    const updatedEntry: WidgetRegistryEntry = {
      ...entry,
      instance
    };

    this.widgets.set(widgetId, updatedEntry);
    
    console.log(`🔗 Widget instance set: ${entry.manifest.config.name} (${widgetId})`);
    return true;
  }

  /**
   * Check if widget is registered
   */
  public isWidgetRegistered(nameOrId: string): boolean {
    return this.findWidgetId(nameOrId) !== undefined;
  }

  /**
   * Get widget count
   */
  public getWidgetCount(): number {
    return this.widgets.size;
  }

  /**
   * Get widget by type
   */
  public getWidgetsByType(type: string): WidgetRegistryEntry[] {
    return Array.from(this.widgets.values()).filter(entry => entry.manifest.config.type === type);
  }

  /**
   * Event system for widget lifecycle
   */
  public addEventListener(type: WidgetEventType, handler: WidgetEventHandler): void {
    const handlers = this.eventHandlers.get(type);
    if (handlers) {
      handlers.add(handler);
    }
  }

  public removeEventListener(type: WidgetEventType, handler: WidgetEventHandler): void {
    const handlers = this.eventHandlers.get(type);
    if (handlers) {
      handlers.delete(handler);
    }
  }

  /**
   * Configuration methods
   */
  public setMaxWidgets(max: number): void {
    if (max < 1) {
      throw new Error('Maximum widgets must be at least 1');
    }
    this.maxWidgets = max;
  }

  public setDevelopmentMode(enabled: boolean): void {
    this.developmentMode = enabled;
  }

  /**
   * Clear all widgets (for testing or reset)
   */
  public async clearAll(): Promise<void> {
    const widgets = Array.from(this.widgets.keys());
    for (const widgetId of widgets) {
      const entry = this.widgets.get(widgetId);
      if (entry) {
        await this.unregisterWidget(entry.manifest.config.name);
      }
    }
  }

  /**
   * Get registry statistics
   */
  public getStats(): {
    total: number;
    active: number;
    byType: Record<string, number>;
    withInstances: number;
  } {
    const entries = Array.from(this.widgets.values());
    const byType: Record<string, number> = {};
    
    entries.forEach(entry => {
      const type = entry.manifest.config.type;
      byType[type] = (byType[type] || 0) + 1;
    });

    return {
      total: entries.length,
      active: entries.filter(e => e.active).length,
      byType,
      withInstances: entries.filter(e => e.instance).length
    };
  }

  // Private helper methods
  private generateWidgetId(name: string): string {
    const timestamp = Date.now();
    const random = Math.random().toString(36).substr(2, 9);
    return `widget_${name}_${timestamp}_${random}`;
  }

  private findWidgetId(nameOrId: string): string | undefined {
    // Try direct ID lookup first
    if (this.widgets.has(nameOrId)) {
      return nameOrId;
    }

    // Search by widget name
    for (const [id, entry] of this.widgets) {
      if (entry.manifest.config.name === nameOrId) {
        return id;
      }
    }

    return undefined;
  }

  private validateManifest(manifest: WidgetManifest): boolean {
    // Basic validation
    if (!manifest.config.name || !manifest.config.version) {
      return false;
    }

    // Check for required assets
    if (!manifest.assets || (!manifest.assets.css && !manifest.assets.html && !manifest.assets.js && !manifest.assets.ts)) {
      return false;
    }

    // Validate base path
    if (!manifest.basePath || typeof manifest.basePath !== 'string') {
      return false;
    }

    return true;
  }

  private async emitEvent(type: WidgetEventType, widgetId: string, data?: unknown): Promise<void> {
    const handlers = this.eventHandlers.get(type);
    if (!handlers || handlers.size === 0) {
      return;
    }

    const event: WidgetEvent = {
      type,
      widgetId,
      timestamp: new Date(),
      data
    };

    // Execute all handlers
    const promises = Array.from(handlers).map(handler => {
      try {
        return handler(event);
      } catch (error) {
        console.error(`⚠️ Widget event handler error for ${type}:`, error);
        return Promise.resolve();
      }
    });

    await Promise.allSettled(promises);
  }
}

// Export singleton instance
export const widgetRegistry = WidgetRegistry.getInstance();