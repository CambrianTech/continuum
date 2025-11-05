/**
 * Widget Manager - Integrates with RendererDaemon for Widget System
 * Bridges widget registry and factory with the renderer daemon
 */

import { 
  WidgetManifest, 
  BaseWidgetInstance, 
  WidgetConfig,
  WidgetAssets,
  WidgetError,
  WidgetSystemConfig
} from '../types/WidgetTypes';

import { widgetRegistry } from './WidgetRegistry';
import { widgetFactory } from './WidgetFactory';

export class WidgetManager {
  private static instance: WidgetManager;
  private config: WidgetSystemConfig;
  private initialized: boolean = false;

  private constructor() {
    this.config = {
      baseUrl: 'http://localhost:9000',
      assetsPath: '/src/ui/components',
      compilationEnabled: true,
      cacheEnabled: true,
      developmentMode: true,
      maxWidgets: 50,
      permissions: []
    };
  }

  public static getInstance(): WidgetManager {
    if (!WidgetManager.instance) {
      WidgetManager.instance = new WidgetManager();
    }
    return WidgetManager.instance;
  }

  /**
   * Initialize widget system
   */
  public async initialize(config?: Partial<WidgetSystemConfig>): Promise<void> {
    if (this.initialized) {
      console.log('🎨 Widget system already initialized');
      return;
    }

    console.log('🎨 Initializing widget system...');

    // Merge configuration
    this.config = { ...this.config, ...config };
    console.log('🔧 Widget system config:', this.config);

    // Configure registry
    widgetRegistry.setMaxWidgets(this.config.maxWidgets);
    widgetRegistry.setDevelopmentMode(this.config.developmentMode);
    console.log('📦 Widget registry configured');

    // Discover and register widgets
    try {
      const manifests = await this.discoverWidgets();
      console.log(`📦 Successfully discovered ${manifests.length} widgets`);
    } catch (error) {
      console.error('❌ Widget discovery failed:', error);
    }

    this.initialized = true;
    console.log('🎨 Widget system initialized successfully');
  }

  /**
   * Discover widgets from the components directory
   */
  public async discoverWidgets(): Promise<WidgetManifest[]> {
    const manifests: WidgetManifest[] = [];
    
    try {
      console.log('🔍 Starting widget discovery...');
      
      // Get list of widget directories
      const widgetDirs = await this.getWidgetDirectories();
      console.log(`📁 Found ${widgetDirs.length} widget directories:`, widgetDirs);
      
      for (const dir of widgetDirs) {
        try {
          console.log(`🔍 Processing widget directory: ${dir}`);
          const manifest = await this.loadWidgetManifest(dir);
          if (manifest) {
            console.log(`✅ Loaded manifest for ${dir}:`, manifest.config.name);
            manifests.push(manifest);
            await widgetRegistry.registerWidget(manifest);
          } else {
            console.warn(`⚠️ No manifest found for ${dir}`);
          }
        } catch (error) {
          console.warn(`⚠️ Failed to load widget from ${dir}:`, error);
        }
      }
      
      console.log(`📦 Discovered ${manifests.length} widgets`);
      return manifests;
    } catch (error) {
      console.error('❌ Widget discovery failed:', error);
      return [];
    }
  }

  /**
   * Create widget instance
   */
  public async createWidget(nameOrManifest: string | WidgetManifest): Promise<BaseWidgetInstance> {
    let manifest: WidgetManifest;
    
    if (typeof nameOrManifest === 'string') {
      const entry = widgetRegistry.getWidget(nameOrManifest);
      if (!entry) {
        throw new WidgetError('Widget not found', nameOrManifest, 'NOT_FOUND');
      }
      manifest = entry.manifest;
    } else {
      manifest = nameOrManifest;
    }

    // Create widget instance using factory
    const instance = await widgetFactory.createWidget(manifest);
    
    // Register instance with registry
    widgetRegistry.setWidgetInstance(manifest.config.name, instance);
    
    return instance;
  }

  /**
   * Get widget HTML for server-side rendering
   */
  public async renderWidget(widgetName: string): Promise<string> {
    const entry = widgetRegistry.getWidget(widgetName);
    if (!entry) {
      throw new WidgetError('Widget not found', widgetName, 'NOT_FOUND');
    }

    // Create widget instance if not exists
    let instance = entry.instance;
    if (!instance) {
      instance = await this.createWidget(entry.manifest);
    }

    // Render widget HTML
    return await this.renderWidgetInstance(instance);
  }

  /**
   * Get widget assets for HTTP serving
   */
  public async getWidgetAssets(widgetName: string): Promise<WidgetAssets> {
    const entry = widgetRegistry.getWidget(widgetName);
    if (!entry) {
      throw new WidgetError('Widget not found', widgetName, 'NOT_FOUND');
    }

    return entry.manifest.assets;
  }

  /**
   * Get widget JavaScript code (compiled from TypeScript)
   */
  public async getWidgetScript(widgetName: string): Promise<string> {
    const entry = widgetRegistry.getWidget(widgetName);
    if (!entry) {
      throw new WidgetError('Widget not found', widgetName, 'NOT_FOUND');
    }

    const { assets } = entry.manifest;
    const scripts: string[] = [];

    // Compile TypeScript files
    for (const tsFile of assets.ts) {
      const compiled = await this.compileWidgetScript(tsFile);
      scripts.push(compiled);
    }

    // Include existing JavaScript files
    for (const jsFile of assets.js) {
      const content = await this.loadAssetContent(jsFile);
      scripts.push(content);
    }

    return scripts.join('\n');
  }

  /**
   * Get widget CSS styles
   */
  public async getWidgetStyles(widgetName: string): Promise<string> {
    const entry = widgetRegistry.getWidget(widgetName);
    if (!entry) {
      throw new WidgetError('Widget not found', widgetName, 'NOT_FOUND');
    }

    const { assets } = entry.manifest;
    const styles: string[] = [];

    // Load CSS files
    for (const cssFile of assets.css) {
      const content = await this.loadAssetContent(cssFile);
      styles.push(content);
    }

    return styles.join('\n');
  }

  /**
   * Get all widget names
   */
  public getWidgetNames(): string[] {
    return widgetRegistry.getAllWidgets().map(entry => entry.manifest.config.name);
  }

  /**
   * Get widget system statistics
   */
  public getStats(): {
    total: number;
    active: number;
    byType: Record<string, number>;
    withInstances: number;
  } {
    return widgetRegistry.getStats();
  }

  /**
   * Update widget system configuration
   */
  public updateConfig(config: Partial<WidgetSystemConfig>): void {
    this.config = { ...this.config, ...config };
    
    // Update registry configuration
    widgetRegistry.setMaxWidgets(this.config.maxWidgets);
    widgetRegistry.setDevelopmentMode(this.config.developmentMode);
  }

  // Private helper methods
  private async getWidgetDirectories(): Promise<string[]> {
    // This would typically read from filesystem, but for browser we'll use known directories
    return [
      'Academy',
      'ActiveProjects', 
      'Chat',
      'ChatRoom',
      'Continuon',
      'Persona',
      'SavedPersonas',
      'Sidebar',
      'SidebarHeader',
      'SidebarPanelWidget',
      'SidebarTabs',
      'UserSelector',
      'Version'
    ];
  }

  private async loadWidgetManifest(directory: string): Promise<WidgetManifest | null> {
    try {
      const packagePath = `${this.config.assetsPath}/${directory}/package.json`;
      const response = await fetch(packagePath);
      
      if (!response.ok) {
        return null;
      }

      const packageData = await response.json();
      
      // Create manifest from package.json
      const config: WidgetConfig = {
        name: packageData.name || directory,
        version: packageData.version || '1.0.0',
        type: packageData.continuum?.type || 'ui',
        dependencies: packageData.dependencies ? Object.keys(packageData.dependencies) : [],
        permissions: packageData.continuum?.permissions || []
      };

      const assets: WidgetAssets = {
        css: (packageData.files || []).filter((f: string) => f.endsWith('.css')),
        html: (packageData.files || []).filter((f: string) => f.endsWith('.html')),
        js: (packageData.files || []).filter((f: string) => f.endsWith('.js')),
        ts: (packageData.files || []).filter((f: string) => f.endsWith('.ts')),
        other: (packageData.files || []).filter((f: string) => !f.match(/\.(css|html|js|ts)$/))
      };

      return {
        config,
        assets,
        basePath: `${this.config.assetsPath}/${directory}`,
        compiled: false
      };
    } catch (error) {
      console.warn(`⚠️ Failed to load manifest for ${directory}:`, error);
      return null;
    }
  }

  private async renderWidgetInstance(instance: BaseWidgetInstance): Promise<string> {
    // For server-side rendering, we need to serialize the widget
    return `
      <div class="widget-container" data-widget="${instance.config.name}">
        <div class="widget-header">
          <h3>${instance.config.name}</h3>
          <span class="widget-version">v${instance.config.version}</span>
        </div>
        <div class="widget-content">
          <!-- Widget content will be rendered by client-side JavaScript -->
        </div>
      </div>
    `;
  }

  private async compileWidgetScript(tsFile: string): Promise<string> {
    // Use the factory's compilation method
    return await widgetFactory.compileAssets({ css: [], html: [], js: [], ts: [tsFile] }).then(assets => {
      return assets.js.join('\n');
    });
  }

  private async loadAssetContent(assetPath: string): Promise<string> {
    try {
      const response = await fetch(assetPath);
      if (response.ok) {
        return await response.text();
      }
      return '';
    } catch (error) {
      console.error(`❌ Failed to load asset: ${assetPath}`, error);
      return '';
    }
  }
}

// Export singleton instance
export const widgetManager = WidgetManager.getInstance();