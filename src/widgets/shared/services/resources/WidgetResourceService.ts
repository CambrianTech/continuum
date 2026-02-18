/**
 * Widget Resource Service - File & Template Loading Adapter
 * 
 * Extracts all resource operations from BaseWidget god object:
 * - Template loading (HTML templates)
 * - Style loading (CSS files) 
 * - File operations (saveFile, loadResource)
 * - Screenshot operations (takeScreenshot)
 * - Resource caching and optimization
 * 
 * Uses adapter pattern for existing JTAG file system.
 */

import type { IWidgetService, WidgetServiceContext } from '../WidgetServiceRegistry';

// Resource service interface - what widgets consume
export interface IWidgetResourceService extends IWidgetService {
  // Template and style loading
  loadTemplate(templateName: string): Promise<string>;
  loadStyles(styleName: string): Promise<string>;
  loadResource(filename: string, resourceType: ResourceType): Promise<string>;
  
  // File operations
  saveFile(filename: string, content: string | Blob, options?: FileSaveOptions): Promise<SaveFileResult>;
  loadFile(filepath: string, options?: FileLoadOptions): Promise<LoadFileResult>;
  fileExists(filepath: string): Promise<boolean>;
  
  // Screenshot operations
  takeScreenshot(options?: ScreenshotOptions): Promise<ScreenshotResult>;
  
  // Resource caching
  clearResourceCache(): Promise<void>;
  preloadResources(resourceList: string[]): Promise<PreloadResult[]>;
}

// Type definitions
export type ResourceType = 'template' | 'style' | 'script' | 'image' | 'data' | 'config';

export interface FileSaveOptions {
  directory?: string;
  createDirectories?: boolean;
  overwrite?: boolean;
  encoding?: 'utf8' | 'binary' | 'base64';
}

export interface SaveFileResult {
  success: boolean;
  filepath?: string;
  bytesWritten?: number;
  error?: string;
}

export interface FileLoadOptions {
  encoding?: 'utf8' | 'binary' | 'base64';
  maxSize?: number;
}

export interface LoadFileResult {
  success: boolean;
  content?: string;
  bytesRead?: number;
  exists?: boolean;
  error?: string;
}

export interface ScreenshotOptions {
  filename?: string;
  format?: 'png' | 'jpeg' | 'webp';
  quality?: number;
  selector?: string;
  fullPage?: boolean;
  viewport?: { width: number; height: number };
}

export interface ScreenshotResult {
  success: boolean;
  filepath?: string;
  base64?: string;
  metadata?: {
    width: number;
    height: number;
    format: string;
    fileSize: number;
  };
  error?: string;
}

export interface PreloadResult {
  resource: string;
  success: boolean;
  cached: boolean;
  error?: string;
}

// Resource service implementation
export class WidgetResourceService implements IWidgetResourceService {
  public readonly serviceName = 'WidgetResourceService';
  public readonly serviceVersion = '1.0.0';
  
  private context?: WidgetServiceContext;
  private resourceCache = new Map<string, string>();
  private resourcePaths: Record<ResourceType, string> = {
    template: 'widgets/shared/templates',
    style: 'widgets/shared/styles', 
    script: 'widgets/shared/scripts',
    image: 'widgets/shared/images',
    data: 'widgets/shared/data',
    config: 'widgets/shared/config'
  };

  async initialize(context: WidgetServiceContext): Promise<void> {
    this.context = context;
    
    // Set widget-specific resource paths
    this.resourcePaths.template = `widgets/${context.widgetName.toLowerCase()}/templates`;
    this.resourcePaths.style = `widgets/${context.widgetName.toLowerCase()}/styles`;
    
    console.debug(`📁 WidgetResourceService: Initialized for widget ${context.widgetName}`);
  }

  async cleanup(): Promise<void> {
    await this.clearResourceCache();
    console.debug(`📁 WidgetResourceService: Cleaned up`);
  }

  // Template and style loading
  async loadTemplate(templateName: string): Promise<string> {
    const resourceKey = `template:${templateName}`;
    
    // Check cache first
    if (this.resourceCache.has(resourceKey)) {
      return this.resourceCache.get(resourceKey)!;
    }

    try {
      const templatePath = `${this.resourcePaths.template}/${templateName}`;
      const content = await this.loadResourceFromFile(templatePath, 'template');
      
      // Cache the template
      this.resourceCache.set(resourceKey, content);
      
      console.debug(`📄 WidgetResourceService: Loaded template '${templateName}'`);
      return content;
    } catch (error) {
      console.error(`❌ WidgetResourceService: Failed to load template '${templateName}':`, error);
      throw error;
    }
  }

  async loadStyles(styleName: string): Promise<string> {
    const resourceKey = `style:${styleName}`;
    
    // Check cache first
    if (this.resourceCache.has(resourceKey)) {
      return this.resourceCache.get(resourceKey)!;
    }

    try {
      const stylePath = `${this.resourcePaths.style}/${styleName}`;
      const content = await this.loadResourceFromFile(stylePath, 'style');
      
      // Cache the styles
      this.resourceCache.set(resourceKey, content);
      
      console.debug(`🎨 WidgetResourceService: Loaded styles '${styleName}'`);
      return content;
    } catch (error) {
      console.error(`❌ WidgetResourceService: Failed to load styles '${styleName}':`, error);
      throw error;
    }
  }

  async loadResource(filename: string, resourceType: ResourceType): Promise<string> {
    const resourceKey = `${resourceType}:${filename}`;
    
    // Check cache first
    if (this.resourceCache.has(resourceKey)) {
      return this.resourceCache.get(resourceKey)!;
    }

    try {
      const resourcePath = `${this.resourcePaths[resourceType]}/${filename}`;
      const content = await this.loadResourceFromFile(resourcePath, resourceType);
      
      // Cache the resource
      this.resourceCache.set(resourceKey, content);
      
      console.debug(`📦 WidgetResourceService: Loaded ${resourceType} '${filename}'`);
      return content;
    } catch (error) {
      console.error(`❌ WidgetResourceService: Failed to load ${resourceType} '${filename}':`, error);
      throw error;
    }
  }

  // File operations (adapter for JTAG file system)
  async saveFile(filename: string, content: string | Blob, options: FileSaveOptions = {}): Promise<SaveFileResult> {
    try {
      const directory = options.directory || 'widgets/shared/files';
      const filepath = `${directory}/${filename}`;
      
      // Convert Blob to string if needed
      let fileContent: string;
      if (content instanceof Blob) {
        fileContent = await this.blobToBase64(content);
      } else {
        fileContent = content;
      }

      // Use JTAG file/save command (adapter)
      const result = await this.executeFileCommand('file/save', {
        filepath,
        content: fileContent,
        createDirectories: options.createDirectories !== false,
        encoding: options.encoding || 'utf8'
      });

      if (result.success) {
        console.debug(`💾 WidgetResourceService: Saved file '${filename}' (${result.bytesWritten} bytes)`);
        return {
          success: true,
          filepath: result.filepath,
          bytesWritten: result.bytesWritten
        };
      } else {
        return {
          success: false,
          error: result.error || 'Unknown save error'
        };
      }
    } catch (error) {
      console.error(`❌ WidgetResourceService: Failed to save file '${filename}':`, error);
      return {
        success: false,
        error: error instanceof Error ? error.message : 'Unknown error'
      };
    }
  }

  async loadFile(filepath: string, options: FileLoadOptions = {}): Promise<LoadFileResult> {
    try {
      // Use JTAG file/load command (adapter)
      const result = await this.executeFileCommand('file/load', {
        filepath,
        encoding: options.encoding || 'utf8'
      });

      if (result.success) {
        console.debug(`📂 WidgetResourceService: Loaded file '${filepath}' (${result.bytesRead} bytes)`);
        return {
          success: true,
          content: result.content,
          bytesRead: result.bytesRead,
          exists: result.exists
        };
      } else {
        return {
          success: false,
          exists: false,
          error: result.error || 'Unknown load error'
        };
      }
    } catch (error) {
      console.error(`❌ WidgetResourceService: Failed to load file '${filepath}':`, error);
      return {
        success: false,
        exists: false,
        error: error instanceof Error ? error.message : 'Unknown error'
      };
    }
  }

  async fileExists(filepath: string): Promise<boolean> {
    try {
      const result = await this.loadFile(filepath);
      return result.exists || false;
    } catch (error) {
      console.error(`❌ WidgetResourceService: Failed to check file existence '${filepath}':`, error);
      return false;
    }
  }

  // Screenshot operations (adapter for JTAG screenshot system)
  async takeScreenshot(options: ScreenshotOptions = {}): Promise<ScreenshotResult> {
    try {
      // Use JTAG screenshot command (adapter)
      const result = await this.executeScreenshotCommand('screenshot', {
        filename: options.filename,
        format: options.format || 'png',
        quality: options.quality,
        querySelector: options.selector,
        fullPage: options.fullPage,
        viewport: options.viewport
      });

      if (result.success) {
        console.debug(`📸 WidgetResourceService: Screenshot taken '${result.filename}'`);
        return {
          success: true,
          filepath: result.filepath,
          base64: result.base64,
          metadata: {
            width: result.width || 0,
            height: result.height || 0,
            format: result.format || 'png',
            fileSize: result.fileSize || 0
          }
        };
      } else {
        return {
          success: false,
          error: result.error || 'Unknown screenshot error'
        };
      }
    } catch (error) {
      console.error(`❌ WidgetResourceService: Failed to take screenshot:`, error);
      return {
        success: false,
        error: error instanceof Error ? error.message : 'Unknown error'
      };
    }
  }

  // Resource caching operations
  async clearResourceCache(): Promise<void> {
    this.resourceCache.clear();
    console.debug(`🗑️ WidgetResourceService: Resource cache cleared`);
  }

  async preloadResources(resourceList: string[]): Promise<PreloadResult[]> {
    const results: PreloadResult[] = [];
    
    for (const resource of resourceList) {
      try {
        // Determine resource type from extension or path
        const resourceType = this.detectResourceType(resource);
        await this.loadResource(resource, resourceType);
        
        results.push({
          resource,
          success: true,
          cached: true
        });
      } catch (error) {
        results.push({
          resource,
          success: false,
          cached: false,
          error: error instanceof Error ? error.message : 'Unknown error'
        });
      }
    }
    
    console.debug(`🚀 WidgetResourceService: Preloaded ${results.filter(r => r.success).length}/${resourceList.length} resources`);
    return results;
  }

  // Private helper methods (adapters to JTAG system)
  private async loadResourceFromFile(filepath: string, resourceType: ResourceType): Promise<string> {
    const result = await this.loadFile(filepath);
    
    if (result.success && result.content) {
      return result.content;
    } else {
      throw new Error(`Failed to load ${resourceType} from '${filepath}': ${result.error}`);
    }
  }

  private async executeFileCommand(command: string, params: any): Promise<any> {
    // This will be replaced with proper JTAG client injection
    console.debug(`📁 WidgetResourceService: Executing file command '${command}'`);
    
    // Simulate JTAG file operations for now
    return {
      success: true,
      filepath: params.filepath,
      content: params.content || 'mock file content',
      bytesRead: params.content?.length || 100,
      bytesWritten: params.content?.length || 100,
      exists: true
    };
  }

  private async executeScreenshotCommand(command: string, params: any): Promise<any> {
    // This will be replaced with proper JTAG client injection
    console.debug(`📸 WidgetResourceService: Executing screenshot command '${command}'`);
    
    // Simulate JTAG screenshot operations for now
    return {
      success: true,
      filepath: params.filename || 'screenshot.png',
      filename: params.filename || 'screenshot.png',
      width: 1200,
      height: 800,
      format: params.format || 'png',
      fileSize: 50000
    };
  }

  private async blobToBase64(blob: Blob): Promise<string> {
    return new Promise((resolve, reject) => {
      const reader = new FileReader();
      reader.onload = () => resolve(reader.result as string);
      reader.onerror = reject;
      reader.readAsDataURL(blob);
    });
  }

  private detectResourceType(resource: string): ResourceType {
    const extension = resource.split('.').pop()?.toLowerCase();
    
    switch (extension) {
      case 'html':
      case 'htm':
        return 'template';
      case 'css':
        return 'style';
      case 'js':
      case 'ts':
        return 'script';
      case 'png':
      case 'jpg':
      case 'jpeg':
      case 'gif':
      case 'svg':
        return 'image';
      case 'json':
      case 'xml':
      case 'yaml':
        return 'data';
      default:
        return 'config';
    }
  }
}