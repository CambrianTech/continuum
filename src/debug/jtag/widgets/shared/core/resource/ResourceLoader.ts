/**
 * Resource Loading Module
 * 
 * Handles loading of external widget resources (HTML templates, CSS styles)
 * via JTAG file operations with proper path resolution and caching.
 */

import type { FileLoadResult } from '../../../../commands/file/load/shared/FileLoadTypes';

export interface ResourceConfig {
  enableCaching: boolean;
  enableDebugging: boolean;
  defaultFallbacks: {
    template: string;
    styles: string;
  };
}

export interface ResourceCache {
  content: string;
  timestamp: number;
  bytesRead: number;
  ttl: number;
}

export interface ResourceOperations {
  jtagFileLoad: (filepath: string) => Promise<FileLoadResult>;
}

export class ResourceLoader {
  private config: ResourceConfig;
  private cache = new Map<string, ResourceCache>();
  private operations: ResourceOperations;
  private widgetName: string;

  constructor(
    widgetName: string,
    operations: ResourceOperations,
    config: Partial<ResourceConfig> = {}
  ) {
    this.widgetName = widgetName;
    this.operations = operations;
    
    this.config = {
      enableCaching: true,
      enableDebugging: false,
      defaultFallbacks: {
        template: '<div>Widget template not found</div>',
        styles: '/* Widget styles not found */'
      },
      ...config
    };
  }

  /**
   * Load HTML template resource
   */
  async loadTemplate(filename: string): Promise<string> {
    return this.loadResource(filename, 'template', this.config.defaultFallbacks.template);
  }

  /**
   * Load CSS styles resource
   */
  async loadStyles(filename: string): Promise<string> {
    return this.loadResource(filename, 'styles', this.config.defaultFallbacks.styles);
  }

  /**
   * Load theme CSS (uses direct path, not widget-specific resolution)
   */
  async loadTheme(themeName: string): Promise<string> {
    const themePath = `widgets/shared/themes/${themeName}.css`;
    return this.loadResourceDirect(themePath, 'theme', '/* No theme loaded */');
  }

  /**
   * Load widget-specific resource with path resolution
   */
  async loadResource(filename: string, resourceType: string, fallback: string): Promise<string> {
    const resourcePath = this.resolveResourcePath(filename);
    return this.loadResourceDirect(resourcePath, resourceType, fallback);
  }

  /**
   * Load resource from direct path (no widget-specific resolution)
   */
  async loadResourceDirect(resourcePath: string, resourceType: string, fallback: string): Promise<string> {
    const emoji = this.getResourceEmoji(resourceType);
    
    // Check cache first
    if (this.config.enableCaching) {
      const cached = this.getCachedResource(resourcePath);
      if (cached) {
        if (this.config.enableDebugging) {
          console.log(`💾 ${this.widgetName}: Using cached ${resourceType} (${cached.bytesRead} bytes)`);
        }
        return cached.content;
      }
    }
    
    console.log(`${emoji} ${this.widgetName}: Loading ${resourceType} from ${resourcePath}`);
    
    try {
      const result = await this.operations.jtagFileLoad(resourcePath);
      
      // Handle nested JTAG response structure
      const fileData = (result as any).commandResult || result;
      
      if (result.success && fileData.success && fileData.content) {
        console.log(`✅ ${this.widgetName}: ${resourceType} loaded successfully (${fileData.bytesRead} bytes)`);
        
        // Cache the result
        if (this.config.enableCaching) {
          this.cacheResource(resourcePath, fileData.content, fileData.bytesRead);
        }
        
        return fileData.content;
      } else {
        console.warn(`⚠️ ${this.widgetName}: ${resourceType} load failed: ${resourcePath}`);
        if (this.config.enableDebugging) {
          console.warn(`  Debug: result.success=${result.success}, fileData.success=${fileData.success}, has content=${!!fileData.content}`);
        }
        return fallback;
      }
    } catch (loadError) {
      console.warn(`⚠️ ${this.widgetName}: ${resourceType} load error: ${resourcePath}`, loadError);
      return fallback;
    }
  }

  /**
   * Resolve widget-specific resource path
   */
  private resolveResourcePath(filename: string): string {
    // Extract widget directory name from widget name (ChatWidget -> chat)
    const widgetDir = this.widgetName.toLowerCase().replace('widget', '');
    // Return relative path from current working directory
    return `widgets/${widgetDir}/public/${filename}`;
  }

  /**
   * Get appropriate emoji for resource type
   */
  private getResourceEmoji(resourceType: string): string {
    switch (resourceType) {
      case 'template': return '📄';
      case 'styles': return '🎨';
      case 'theme': return '🎭';
      case 'script': return '📜';
      default: return '📁';
    }
  }

  /**
   * Cache resource content with TTL
   */
  private cacheResource(path: string, content: string, bytesRead: number): void {
    const ttl = 5 * 60 * 1000; // 5 minutes for resources (they don't change often)
    
    this.cache.set(path, {
      content,
      timestamp: Date.now(),
      bytesRead,
      ttl
    });

    if (this.config.enableDebugging) {
      console.log(`💾 ${this.widgetName}: Cached resource ${path} (${bytesRead} bytes, TTL: ${ttl}ms)`);
    }
  }

  /**
   * Get cached resource if valid
   */
  private getCachedResource(path: string): ResourceCache | null {
    const cached = this.cache.get(path);
    
    if (!cached) {
      return null;
    }

    // Check if cache is still valid
    const isValid = (Date.now() - cached.timestamp) < cached.ttl;
    
    if (!isValid) {
      this.cache.delete(path);
      return null;
    }

    return cached;
  }

  /**
   * Clear resource cache (useful for development)
   */
  clearCache(): void {
    const cacheSize = this.cache.size;
    this.cache.clear();
    
    if (this.config.enableDebugging) {
      console.log(`🗑️ ${this.widgetName}: Cleared resource cache (${cacheSize} items)`);
    }
  }

  /**
   * Get cache statistics
   */
  getCacheStats(): { size: number; items: Array<{ path: string; age: number; bytes: number }> } {
    const items = Array.from(this.cache.entries()).map(([path, cache]) => ({
      path,
      age: Date.now() - cache.timestamp,
      bytes: cache.bytesRead
    }));

    return {
      size: this.cache.size,
      items
    };
  }

  /**
   * Update configuration
   */
  updateConfig(newConfig: Partial<ResourceConfig>): void {
    this.config = { ...this.config, ...newConfig };
  }
}