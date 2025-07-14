/**
 * Module Bootstrap - Dynamic Module Registration
 * =============================================
 * Registers module loaders for dynamic discovery and loading
 */

import { moduleRegistry } from './DynamicModuleRegistry';

/**
 * Bootstrap dynamic modules by registering their loaders
 * This runs once at startup, not in loops
 */
export function bootstrapDynamicModules(): void {
  console.log('🚀 Bootstrapping dynamic modules...');

  // Register processor modules
  moduleRegistry.registerModuleLoader('health-processor', async () => {
    const module = await import('../processors/HealthDataProcessor');
    return new module.HealthDataProcessor();
  });

  // Register handler modules  
  moduleRegistry.registerModuleLoader('screenshot-handler', async () => {
    const module = await import('../../../commands/browser/screenshot/ScreenshotCommand');
    return module.ScreenshotCommand;
  });

  // Register widget modules
  moduleRegistry.registerModuleLoader('simple-pattern', async () => {
    const module = await import('./SimplePatternWidget');
    return module.SimplePatternWidget;
  });

  console.log('✅ Dynamic module loaders registered');
}

/**
 * Auto-bootstrap when imported
 */
if (typeof window !== 'undefined') {
  // Only bootstrap in browser environment
  bootstrapDynamicModules();
}