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

// CLI execution — when run directly, audit all types
if (require.main === module) {
  const path = require('path');
  const rootPath = path.join(__dirname, '..');
  const args = process.argv.slice(2);

  const registry = createGeneratorRegistry(rootPath);

  // Parse --type flag
  const typeArg = args.find(a => a.startsWith('--type='));
  const typeName = typeArg?.split('=')[1];

  // Parse --fix flag
  const doFix = args.includes('--fix');

  if (typeName) {
    // Audit single type
    const gen = registry.get(typeName);
    if (doFix) {
      const fixResult = gen.fixAll();
      console.log(`\nFixed ${fixResult.totalFixed} issues in ${fixResult.results.length} ${typeName} modules`);
      if (fixResult.totalRemaining > 0) {
        console.log(`${fixResult.totalRemaining} issues remaining (manual fix required)`);
      }
      for (const result of fixResult.results) {
        console.log(`  ${result.name}: ${result.issuesFixed.length} fixed, ${result.issuesRemaining.length} remaining`);
      }
    } else {
      const summary = gen.audit();
      const map = new Map([[typeName, summary]]);
      registry.printAuditReport(map);
    }
  } else {
    // Audit all types
    if (doFix) {
      const results = registry.fixAll();
      let totalFixed = 0;
      let totalRemaining = 0;
      for (const [type, fixResult] of results) {
        totalFixed += fixResult.totalFixed;
        totalRemaining += fixResult.totalRemaining;
        if (fixResult.results.length > 0) {
          console.log(`\n${type}: Fixed ${fixResult.totalFixed} issues`);
          for (const result of fixResult.results) {
            console.log(`  ${result.name}: ${result.issuesFixed.join(', ')}`);
          }
        }
      }
      console.log(`\nTotal: ${totalFixed} fixed, ${totalRemaining} remaining`);
    } else {
      const summaries = registry.auditAll();
      registry.printAuditReport(summaries);
    }
  }
}
