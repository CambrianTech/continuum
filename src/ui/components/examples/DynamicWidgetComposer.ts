/**
 * Dynamic Widget Composer - Runtime Widget Composition
 * ===================================================
 * Composes widgets dynamically from discovered modules
 */

import { BaseWidget } from '../shared/BaseWidget';
import { 
  DataSourceType, 
  WidgetCapabilities
} from '../../../types/shared/WidgetServerTypes';
import { moduleRegistry } from './DynamicModuleRegistry';

export interface ProcessorModule {
  process(dataSource: DataSourceType, data: unknown): unknown;
  supports(dataSource: DataSourceType): boolean;
}

export interface HandlerModule {
  handle(action: string, params: Record<string, unknown>): Promise<void>;
  getActions(): readonly string[];
}

export interface ValidatorModule {
  validate(data: unknown): boolean;
  getSchema(): Record<string, unknown>;
}

export class DynamicWidgetComposer extends BaseWidget {
  private processors = new Map<DataSourceType, ProcessorModule>();
  private handlers = new Map<string, HandlerModule>();
  private validators = new Map<string, ValidatorModule>();
  private capabilities: WidgetCapabilities | null = null;

  constructor() {
    super();
    this.widgetName = 'DynamicComposer';
    this.widgetIcon = '🧩';
    this.widgetTitle = 'Dynamic Module Composer';
  }

  protected override getWidgetCapabilities(): WidgetCapabilities {
    if (!this.capabilities) {
      // Build capabilities dynamically from loaded modules
      this.capabilities = this.buildDynamicCapabilities();
    }
    return this.capabilities;
  }

  protected override async initializeWidget(): Promise<void> {
    await super.initializeWidget();
    
    // Discover and load modules dynamically
    await this.discoverAndLoadModules();
    
    // Rebuild capabilities after loading modules
    this.capabilities = this.buildDynamicCapabilities();
    
    // Start with first available data source
    const dataSources = this.capabilities.canFetchData;
    if (dataSources.length > 0) {
      this.fetchServerData(dataSources[0], {});
    }
  }

  /**
   * Discover modules from known paths
   */
  private async discoverAndLoadModules(): Promise<void> {
    const searchPaths = [
      '/src/ui/components/processors',
      '/src/ui/components/examples',
      '/src/commands/browser/screenshot', // Handlers belong with their commands
      '/src/commands/ui',
      '/src/commands/system'
    ];

    // Discover available modules
    const discovered = await moduleRegistry.discoverModules(searchPaths);
    console.log(`🧩 Discovered ${discovered.length} modules`);

    // Load modules by type
    await this.loadModulesByType('processor');
    await this.loadModulesByType('handler');
    await this.loadModulesByType('validator');
  }

  /**
   * Load all modules of a specific type
   */
  private async loadModulesByType(type: string): Promise<void> {
    const moduleNames = moduleRegistry.getModulesByType(type as any);
    
    for (const name of moduleNames) {
      try {
        const module = await moduleRegistry.loadModule(name);
        if (module?.isLoaded) {
          this.registerModuleByType(type, module.exports);
        }
      } catch (error) {
        console.warn(`Failed to load ${type} module ${name}:`, error);
      }
    }
  }

  /**
   * Register module exports by type
   */
  private registerModuleByType(type: string, exports: unknown): void {
    switch (type) {
      case 'processor':
        this.registerProcessor(exports as ProcessorModule);
        break;
      case 'handler':
        this.registerHandler(exports as HandlerModule);
        break;
      case 'validator':
        this.registerValidator(exports as ValidatorModule);
        break;
    }
  }

  /**
   * Register a data processor module
   */
  private registerProcessor(processor: ProcessorModule): void {
    // Find which data sources this processor supports
    const dataSources: DataSourceType[] = [
      'health', 'widgets', 'daemons', 'commands', 'sessions',
      'projects', 'personas', 'logs', 'metrics'
    ];

    for (const dataSource of dataSources) {
      if (processor.supports(dataSource)) {
        this.processors.set(dataSource, processor);
        console.log(`🧩 Registered processor for ${dataSource}`);
      }
    }
  }

  /**
   * Register an action handler module
   */
  private registerHandler(handler: HandlerModule): void {
    const actions = handler.getActions();
    for (const action of actions) {
      this.handlers.set(action, handler);
      console.log(`🧩 Registered handler for ${action}`);
    }
  }

  /**
   * Register a validator module
   */
  private registerValidator(validator: ValidatorModule): void {
    // Use module name as validator key
    const key = `validator_${Date.now()}`;
    this.validators.set(key, validator);
    console.log(`🧩 Registered validator: ${key}`);
  }

  /**
   * Build capabilities dynamically from loaded modules
   */
  private buildDynamicCapabilities(): WidgetCapabilities {
    const dataSources = Array.from(this.processors.keys());
    const commands = Array.from(this.handlers.keys());

    return {
      canFetchData: dataSources,
      canExecuteCommands: commands,
      respondsToEvents: ['data:updated', 'session:created'],
      supportsExport: ['json'],
      requiresAuth: false,
      updateFrequency: 'manual'
    };
  }

  /**
   * Process data using dynamically loaded processors
   */
  protected override processServerData(dataSource: DataSourceType, data: unknown): void {
    const processor = this.processors.get(dataSource);
    
    if (processor) {
      try {
        const processedData = processor.process(dataSource, data);
        console.log(`🧩 Processed ${dataSource} data:`, processedData);
        this.update();
      } catch (error) {
        console.error(`🧩 Processor error for ${dataSource}:`, error);
      }
    } else {
      console.warn(`🧩 No processor found for ${dataSource}`);
    }
  }

  /**
   * Execute actions using dynamically loaded handlers
   */
  private async executeAction(action: string, params: Record<string, unknown> = {}): Promise<void> {
    const handler = this.handlers.get(action);
    
    if (handler) {
      try {
        await handler.handle(action, params);
        console.log(`🧩 Executed action: ${action}`);
      } catch (error) {
        console.error(`🧩 Handler error for ${action}:`, error);
      }
    } else {
      console.warn(`🧩 No handler found for action: ${action}`);
    }
  }


  renderContent(): string {
    const loadedModules = moduleRegistry.getLoadedModules();
    
    return `
      <div class="dynamic-composer">
        <div class="header">
          <span>${this.widgetIcon} ${this.widgetTitle}</span>
          <span class="module-count">${loadedModules.length} modules</span>
        </div>
        
        <div class="modules-section">
          <h3>Loaded Modules</h3>
          ${this.renderLoadedModules(loadedModules)}
        </div>
        
        <div class="capabilities-section">
          <h3>Dynamic Capabilities</h3>
          ${this.renderCapabilities()}
        </div>
        
        <div class="actions-section">
          <h3>Available Actions</h3>
          ${this.renderActions()}
        </div>
      </div>
    `;
  }

  private renderLoadedModules(modules: readonly any[]): string {
    return modules.map(module => `
      <div class="module-item">
        <span class="module-name">${module.definition.name}</span>
        <span class="module-type">${module.definition.type}</span>
        <span class="module-version">${module.definition.version}</span>
      </div>
    `).join('');
  }

  private renderCapabilities(): string {
    const caps = this.getWidgetCapabilities();
    return `
      <div class="capability-list">
        <div>Data Sources: ${caps.canFetchData.join(', ')}</div>
        <div>Commands: ${caps.canExecuteCommands.join(', ')}</div>
        <div>Events: ${caps.respondsToEvents.join(', ')}</div>
      </div>
    `;
  }

  private renderActions(): string {
    const actions = Array.from(this.handlers.keys());
    return actions.map(action => `
      <button class="action-btn" data-action="${action}">
        ${action}
      </button>
    `).join('');
  }

  setupEventListeners(): void {
    if (!this.shadowRoot) return;

    this.shadowRoot.querySelectorAll('.action-btn').forEach(btn => {
      btn.addEventListener('click', async (_e) => {
        const action = (btn as HTMLElement).dataset.action!;
        await this.executeAction(action, {});
      });
    });
  }
}

// Register the dynamic composer widget
if (!customElements.get('dynamic-composer')) {
  customElements.define('dynamic-composer', DynamicWidgetComposer);
}

export default DynamicWidgetComposer;