/**
 * Widget Factory - Creates and Compiles Widget Instances
 * Handles TypeScript compilation and widget instantiation
 */

import { 
  WidgetManifest, 
  BaseWidgetInstance, 
  WidgetFactory as IWidgetFactory,
  WidgetAssets,
  WidgetConfig,
  WidgetState,
  WidgetError,
  CompileOptions,
  WidgetCompilationError
} from '../types/WidgetTypes';

import { TypeScriptCompiler } from '../../../daemons/renderer/core/TypeScriptCompiler';

export class WidgetFactory implements IWidgetFactory {
  public readonly name = 'DefaultWidgetFactory';
  public readonly supportedTypes = ['ui', 'data', 'control', 'display'];

  private compiler: TypeScriptCompiler;
  private compilationCache: Map<string, string> = new Map();
  private cacheEnabled: boolean = true;

  constructor(options: { cacheEnabled?: boolean } = {}) {
    this.compiler = new TypeScriptCompiler();
    this.cacheEnabled = options.cacheEnabled ?? true;
  }

  /**
   * Create a widget instance from manifest
   */
  public async createWidget(manifest: WidgetManifest): Promise<BaseWidgetInstance> {
    if (!this.validateManifest(manifest)) {
      throw new WidgetError('Invalid widget manifest', manifest.config.name, 'INVALID_MANIFEST');
    }

    // Compile assets if needed
    const compiledAssets = await this.compileAssets(manifest.assets);
    
    // Create widget element
    const element = await this.createWidgetElement(manifest, compiledAssets);
    
    // Create widget instance
    const instance = new DefaultWidgetInstance(manifest.config, element);
    
    // Initialize the widget
    await instance.initialize();
    
    return instance;
  }

  /**
   * Validate widget manifest
   */
  public validateManifest(manifest: WidgetManifest): boolean {
    if (!manifest.config || !manifest.config.name || !manifest.config.version) {
      return false;
    }

    if (!this.supportedTypes.includes(manifest.config.type)) {
      return false;
    }

    if (!manifest.assets || !manifest.basePath) {
      return false;
    }

    return true;
  }

  /**
   * Compile widget assets (TypeScript, CSS, HTML)
   */
  public async compileAssets(assets: WidgetAssets): Promise<WidgetAssets> {
    const compiledAssets: WidgetAssets = {
      css: assets.css,
      html: assets.html,
      js: [...assets.js],
      ts: assets.ts
    };

    // Compile TypeScript files to JavaScript
    for (const tsFile of assets.ts) {
      try {
        const compiledJS = await this.compileTypeScript(tsFile);
        if (compiledJS) {
          // Replace .ts extension with .js
          const jsFile = tsFile.replace(/\.ts$/, '.js');
          compiledAssets.js.push(jsFile);
        }
      } catch (error) {
        throw new WidgetCompilationError(
          `Failed to compile TypeScript file: ${tsFile}`,
          'unknown',
          tsFile
        );
      }
    }

    return compiledAssets;
  }

  /**
   * Compile TypeScript to JavaScript
   */
  private async compileTypeScript(tsFile: string): Promise<string | null> {
    // Check cache first
    if (this.cacheEnabled && this.compilationCache.has(tsFile)) {
      return this.compilationCache.get(tsFile)!;
    }

    try {
      // Load TypeScript source
      const response = await fetch(tsFile);
      if (!response.ok) {
        throw new Error(`Failed to load TypeScript file: ${tsFile}`);
      }

      const source = await response.text();
      
      // Compile with TypeScript compiler
      const compileOptions: CompileOptions = {
        target: 'es2020',
        module: 'es2020',
        strict: true,
        sourceMaps: false
      };

      const compiledJS = await this.compiler.compileWidgetComponent(source, tsFile, compileOptions);
      
      // Cache the result
      if (this.cacheEnabled) {
        this.compilationCache.set(tsFile, compiledJS);
      }
      
      return compiledJS;
    } catch (error) {
      console.error(`❌ TypeScript compilation failed for ${tsFile}:`, error);
      throw new WidgetCompilationError(
        `TypeScript compilation failed: ${error}`,
        'unknown',
        tsFile
      );
    }
  }

  /**
   * Create widget HTML element
   */
  private async createWidgetElement(manifest: WidgetManifest, assets: WidgetAssets): Promise<HTMLElement> {
    // Create custom element for the widget
    const tagName = `widget-${manifest.config.name.toLowerCase()}`;
    
    // Check if element is already defined
    if (!customElements.get(tagName)) {
      // Create widget class dynamically
      const WidgetClass = await this.createWidgetClass(manifest, assets);
      customElements.define(tagName, WidgetClass);
    }

    // Create and return element instance
    return document.createElement(tagName);
  }

  /**
   * Create widget class dynamically
   */
  private async createWidgetClass(_manifest: WidgetManifest, assets: WidgetAssets): Promise<CustomElementConstructor> {
    
    // Load CSS content
    const cssContent = await this.loadAssetContent(assets.css);
    
    // Load HTML content
    const htmlContent = await this.loadAssetContent(assets.html);
    
    // Create widget class
    class DynamicWidget extends HTMLElement {
      declare shadowRoot: ShadowRoot;
      constructor() {
        super();
        this.attachShadow({ mode: 'open' });
      }
      
      connectedCallback() {
        this.render();
      }
      
      private render() {
        this.shadowRoot.innerHTML = `
          <style>
            ${cssContent}
          </style>
          ${htmlContent}
        `;
      }
    }
    
    return DynamicWidget as CustomElementConstructor;
  }

  /**
   * Load asset content from file paths
   */
  private async loadAssetContent(assetPaths: string[]): Promise<string> {
    const contents: string[] = [];
    
    for (const path of assetPaths) {
      try {
        const response = await fetch(path);
        if (response.ok) {
          contents.push(await response.text());
        } else {
          console.warn(`⚠️ Failed to load asset: ${path}`);
        }
      } catch (error) {
        console.error(`❌ Error loading asset ${path}:`, error);
      }
    }
    
    return contents.join('\n');
  }

  /**
   * Clear compilation cache
   */
  public clearCache(): void {
    this.compilationCache.clear();
  }

  /**
   * Get cache statistics
   */
  public getCacheStats(): { size: number; enabled: boolean } {
    return {
      size: this.compilationCache.size,
      enabled: this.cacheEnabled
    };
  }
}

/**
 * Default widget instance implementation
 */
class DefaultWidgetInstance implements BaseWidgetInstance {
  public readonly config: WidgetConfig;
  public readonly element: HTMLElement;
  public readonly state: WidgetState;

  private _state: WidgetState = {
    connected: false,
    initialized: false,
    collapsed: false
  };

  constructor(config: WidgetConfig, element: HTMLElement) {
    this.config = config;
    this.element = element;
    this.state = this._state;
  }

  public async initialize(): Promise<void> {
    try {
      // Attach element to DOM if not already attached
      if (!this.element.parentElement) {
        // Find appropriate container or create one
        const container = this.findOrCreateContainer();
        container.appendChild(this.element);
      }

      this._state = {
        ...this._state,
        initialized: true,
        connected: true
      };

      console.log(`🎨 Widget initialized: ${this.config.name}`);
    } catch (error) {
      this._state = {
        ...this._state,
        error: error instanceof Error ? error.message : String(error)
      };
      throw error;
    }
  }

  public async render(): Promise<void> {
    // Rendering is handled by the custom element
    // This method can be extended for additional rendering logic
  }

  public async update(data?: unknown): Promise<void> {
    // Update widget with new data
    if (data) {
      this._state = {
        ...this._state,
        data: data as Record<string, unknown>
      };
    }

    // Trigger re-render if needed
    await this.render();
  }

  public async cleanup(): Promise<void> {
    // Remove element from DOM
    if (this.element.parentElement) {
      this.element.parentElement.removeChild(this.element);
    }

    this._state = {
      ...this._state,
      connected: false,
      initialized: false
    };

    console.log(`🧹 Widget cleaned up: ${this.config.name}`);
  }

  public async sendMessage(message: any): Promise<void> {
    // Send message via continuum API
    const continuum = (window as any).continuum;
    if (continuum && continuum.isConnected()) {
      await continuum.execute('widget:message', {
        widgetId: this.config.name,
        message
      });
    }
  }

  public async executeCommand(command: any): Promise<any> {
    // Execute command via continuum API
    const continuum = (window as any).continuum;
    if (continuum && continuum.isConnected()) {
      return await continuum.execute(command.command, command.params);
    }
    throw new Error('Continuum API not available');
  }

  public addEventListener(type: string, handler: (event: Event) => void): void {
    this.element.addEventListener(type, handler);
  }

  public removeEventListener(type: string, handler: (event: Event) => void): void {
    this.element.removeEventListener(type, handler);
  }

  private findOrCreateContainer(): HTMLElement {
    // Look for existing widget container
    let container = document.querySelector('.widget-container');
    
    if (!container) {
      // Create container if it doesn't exist
      container = document.createElement('div');
      container.className = 'widget-container';
      document.body.appendChild(container);
    }
    
    return container as HTMLElement;
  }
}

// Export factory instance
export const widgetFactory = new WidgetFactory();