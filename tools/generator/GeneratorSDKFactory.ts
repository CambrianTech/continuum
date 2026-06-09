/**
 * GeneratorSDKFactory — Creates and registers all generator types
 *
 * Usage:
 *   const registry = createGeneratorRegistry('/path/to/src');
 *   const cmd = registry.get('command');
 *   cmd.audit();
 *   cmd.fixAll();
 *
 * Or for CLI/command use:
 *   const registry = createGeneratorRegistry(process.cwd());
 *   const audits = registry.auditAll();
 *   registry.printAuditReport(audits);
 */

import { GeneratorRegistry } from './GeneratorSDK';
import { CommandGeneratorType } from './types/CommandGeneratorType';
import { EntityGeneratorType } from './types/EntityGeneratorType';
import { DaemonGeneratorType } from './types/DaemonGeneratorType';
import { WidgetGeneratorType } from './types/WidgetGeneratorType';

/**
 * Create a fully-initialized GeneratorRegistry with all known types.
 *
 * @param rootPath - Path to the src/ directory
 */
export function createGeneratorRegistry(rootPath: string): GeneratorRegistry {
  // Reset singleton to allow fresh creation with new rootPath
  GeneratorRegistry.reset();
  const registry = GeneratorRegistry.instance;

  registry.register(new CommandGeneratorType(rootPath));
  registry.register(new EntityGeneratorType(rootPath));
  registry.register(new DaemonGeneratorType(rootPath));
  registry.register(new WidgetGeneratorType(rootPath));

  return registry;
}


// CLI entry point removed — was causing esbuild to execute readFileSync at bundle time.
// Run generators via: npx tsx generator/<name>.ts
