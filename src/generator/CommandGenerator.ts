/**
 * CommandGenerator - Generate command files using template system
 *
 * Phase 2 of generator refactoring: Extend ModuleGenerator base class
 * to eliminate duplicate code and enable daemon/widget generation
 */

import { ModuleGenerator, type GenerateOptions } from './ModuleGenerator';
import { TemplateLoader } from './TemplateLoader';
import type { CommandSpec } from './CommandNaming';
import * as path from 'path';

export class CommandGenerator extends ModuleGenerator<CommandSpec> {
  private currentSpec?: CommandSpec; // Temporary storage for writeFiles

  /**
   * Get module type (for logging)
   */
  protected getModuleType(): string {
    return 'command';
  }

  /**
   * Get module name from spec (for logging)
   */
  protected getModuleName(spec: CommandSpec): string {
    return spec.name;
  }

  /**
   * Get output directory for this command
   */
  protected getOutputDir(spec: CommandSpec): string {
    return path.join(this.rootPath, 'commands', spec.name);
  }

  /**
   * Render all templates for this command
   * Returns rendered content for each file
   */
  protected renderTemplates(spec: CommandSpec): Record<string, string> {
    this.currentSpec = spec; // Store for writeFiles
    const rendered = TemplateLoader.renderCommand(spec);

    // Remove tokens property - we don't need it in the base class return type
    const { tokens, ...templates } = rendered;
    return templates;
  }

  /**
   * Write rendered templates to disk
   */
  protected writeFiles(baseDir: string, rendered: Record<string, string>): void {
    if (!this.currentSpec) {
      throw new Error('currentSpec not set - renderTemplates must be called first');
    }

    // Compute className once
    const className = this.currentSpec.name.split('/').map(part =>
      part.charAt(0).toUpperCase() + part.slice(1).toLowerCase()
    ).join('');

    // Write shared types file
    const sharedTypesPath = path.join(baseDir, 'shared', `${className}Types.ts`);
    this.writeFile(sharedTypesPath, rendered.sharedTypes);

    // Write browser implementation file
    const browserPath = path.join(baseDir, 'browser', `${className}BrowserCommand.ts`);
    this.writeFile(browserPath, rendered.browser);

    // Write server implementation file
    const serverPath = path.join(baseDir, 'server', `${className}ServerCommand.ts`);
    this.writeFile(serverPath, rendered.server);

    // Write README file
    const readmePath = path.join(baseDir, 'README.md');
    this.writeFile(readmePath, rendered.readme);

    // Write unit test file
    const unitTestPath = path.join(baseDir, 'test', 'unit', `${className}Command.test.ts`);
    this.writeFile(unitTestPath, rendered.unitTest);

    // Write integration test file
    const integrationTestPath = path.join(baseDir, 'test', 'integration', `${className}Integration.test.ts`);
    this.writeFile(integrationTestPath, rendered.integrationTest);

    // Write package.json file
    const packageJsonPath = path.join(baseDir, 'package.json');
    this.writeFile(packageJsonPath, rendered.packageJson);

    // Write .npmignore file
    const npmignorePath = path.join(baseDir, '.npmignore');
    this.writeFile(npmignorePath, rendered.npmignore);

    // Print next steps
    console.log(`\n📋 Next steps:`);
    console.log(`   1. Fill in unit tests (TDD): ${unitTestPath}`);
    console.log(`   2. Run tests: npx tsx ${unitTestPath}`);
    console.log(`   3. Implement command logic to pass tests`);
    console.log(`   4. Run integration tests after npm start`);
    console.log(`\n📦 Package commands:`);
    console.log(`   - cd ${baseDir} && npm test    (run all tests)`);
    console.log(`   - cd ${baseDir} && npm pack    (create .tgz package)`);

    // Clean up
    this.currentSpec = undefined;
  }

  /**
   * Generate from a JSON file containing CommandSpec
   */
  generateFromFile(specFilePath: string, outputDir?: string): void {
    const fs = require('fs');
    const specJson = fs.readFileSync(specFilePath, 'utf-8');
    const spec: CommandSpec = JSON.parse(specJson);
    this.generate(spec, outputDir);
  }
}

// CLI execution - only runs when invoked directly as CommandGenerator, not when bundled
// The __filename check ensures this doesn't run in bundled contexts
const isDirectExecution = require.main === module &&
  __filename.includes('CommandGenerator') &&
  !process.argv[1]?.includes('cli-bundle') &&
  !process.argv[1]?.includes('cli.js');

if (isDirectExecution) {
  // Lazy imports for CLI-only dependencies (not needed when imported as library)
  const { HelpFormatter } = require('./HelpFormatter');
  const { CommandAuditor } = require('./CommandAuditor');

  const args = process.argv.slice(2);
  const rootPath = path.join(__dirname, '..');

  // ── No arguments → short help ─────────────────────────────────
  if (args.length === 0) {
    console.error(HelpFormatter.shortHelp());
    process.exit(1);
  }

  const firstArg = args[0];

  // ── --help or --help=<topic> ──────────────────────────────────
  if (firstArg === '--help') {
    console.log(HelpFormatter.fullHelp());
    process.exit(0);
  }
  if (firstArg.startsWith('--help=')) {
    const topic = firstArg.split('=')[1];
    console.log(HelpFormatter.topicHelp(topic));
    process.exit(0);
  }

  // ── --template or --template=<type> ───────────────────────────
  if (firstArg === '--template' || firstArg.startsWith('--template=')) {
    const type = firstArg.includes('=') ? firstArg.split('=')[1] : 'standard';
    const spec = HelpFormatter.templateSpec(type);
    console.log(JSON.stringify(spec, null, 2));
    console.log(`\n  Save to a file, then generate:`);
    console.log(`  npx tsx generator/CommandGenerator.ts generator/specs/<name>.json`);
    console.log(`\n  Template types: minimal, standard (default), rust-ipc, browser-only`);
    process.exit(0);
  }

  // ── --audit ───────────────────────────────────────────────────
  if (firstArg === '--audit') {
    const auditor = new CommandAuditor(rootPath);
    auditor.printAudit();
    process.exit(0);
  }

  // ── --reverse <command-dir> ───────────────────────────────────
  if (firstArg === '--reverse') {
    const commandDir = args[1];
    if (!commandDir) {
      console.error('Usage: npx tsx generator/CommandGenerator.ts --reverse <command-dir>');
      console.error('Example: npx tsx generator/CommandGenerator.ts --reverse commands/ping');
      process.exit(1);
    }
    const auditor = new CommandAuditor(rootPath);
    const spec = auditor.reverseEngineer(commandDir);
    if (spec) {
      console.log(JSON.stringify(spec, null, 2));
      console.log(`\n  Save to generator/specs/<name>.json, then regenerate:`);
      console.log(`  npx tsx generator/CommandGenerator.ts generator/specs/<name>.json --force`);
    } else {
      process.exit(1);
    }
    process.exit(0);
  }

  // ── --test (smoke test) ───────────────────────────────────────
  if (firstArg === '--test') {
    const testSpec: CommandSpec = {
      name: 'test/sample',
      description: 'Sample test command for generator testing',
      params: [
        { name: 'message', type: 'string', optional: false, description: 'Message to process' },
        { name: 'verbose', type: 'boolean', optional: true, description: 'Enable verbose output' }
      ],
      results: [
        { name: 'processedMessage', type: 'string', description: 'The processed message' },
        { name: 'timestamp', type: 'number', description: 'Unix timestamp of processing' }
      ],
      examples: [
        {
          description: 'Process a simple message',
          command: './jtag test/sample --message="Hello World"',
          expectedResult: '{ processedMessage: "HELLO WORLD", timestamp: 1234567890 }'
        },
        {
          description: 'Process with verbose output',
          command: './jtag test/sample --message="Test" --verbose=true'
        }
      ],
      accessLevel: 'ai-safe'
    };

    console.log('  Test Mode: Generating sample command...\n');
    const generator = new CommandGenerator(rootPath);
    generator.generate(testSpec, '/tmp/generated-command-test');
    process.exit(0);
  }

  // ── Normal mode: generate from spec file ──────────────────────
  const specFile = firstArg;

  // Parse flags and positional args
  const flagArgs = args.filter(a => a.startsWith('--'));
  const nonFlagArgs = args.filter(a => !a.startsWith('--'));

  const options = {
    force: flagArgs.includes('--force'),
    backup: flagArgs.includes('--backup')
  };

  const outputDir = nonFlagArgs[1]; // Second non-flag arg is output dir

  // Validate backup requires force
  if (options.backup && !options.force) {
    console.error('  --backup requires --force');
    process.exit(1);
  }

  const fs = require('fs');
  if (!fs.existsSync(specFile)) {
    console.error(`Spec file not found: ${specFile}`);
    console.error(`\nRun --help for usage, or --template to generate a starter spec.`);
    process.exit(1);
  }

  const specJson = fs.readFileSync(specFile, 'utf-8');
  const spec: CommandSpec = JSON.parse(specJson);
  const generator = new CommandGenerator(rootPath);
  generator.generate(spec, outputDir, options);
}

export { CommandSpec };
