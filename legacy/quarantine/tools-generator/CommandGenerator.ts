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
    const className = this.currentSpec.name.split(/[\/-]/).map(part =>
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

// CLI entry point moved to generator/cli.ts to prevent esbuild from
// bundling it into cli-bundle.js. Run generator via:
//   npx tsx generator/CommandGenerator.ts <spec.json>
// which is aliased in package.json.

export { CommandSpec };
