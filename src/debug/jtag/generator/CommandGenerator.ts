/**
 * CommandGenerator - Generate command files using template system
 *
 * Phase 1 of generator refactoring: Integrate TemplateLoader for command generation
 * without touching the existing generate-structure.ts
 */

import { TemplateLoader } from './TemplateLoader';
import type { CommandSpec } from './CommandNaming';
import * as path from 'path';

export class CommandGenerator {
  private rootPath: string;

  constructor(rootPath: string) {
    this.rootPath = rootPath;
  }

  /**
   * Generate command files from a CommandSpec
   */
  generate(spec: CommandSpec, outputDir?: string, options?: { force?: boolean, backup?: boolean }): void {
    console.log(`📝 Generating files for command: ${spec.name}`);

    // Determine output directory
    const baseDir = outputDir ?? path.join(this.rootPath, 'commands', spec.name);

    // Check if command already exists
    const fs = require('fs');
    if (fs.existsSync(baseDir)) {
      if (options?.force) {
        console.log(`⚠️  Command already exists at: ${baseDir}`);

        if (options?.backup) {
          // Create backup before overwriting
          const backupDir = `${baseDir}.backup.${Date.now()}`;
          console.log(`📦 Creating backup: ${backupDir}`);
          fs.cpSync(baseDir, backupDir, { recursive: true });
        }

        console.log(`🔄 Overwriting existing command (--force)`);
      } else {
        console.error(`\n❌ ERROR: Command already exists at: ${baseDir}`);
        console.error(`\nOptions:`);
        console.error(`  1. Use --force to overwrite existing command`);
        console.error(`  2. Use --force --backup to backup before overwriting`);
        console.error(`  3. Specify different output directory:`);
        console.error(`     npx tsx generator/CommandGenerator.ts spec.json /tmp/output`);
        console.error(`  4. Delete existing command first:`);
        console.error(`     rm -rf ${baseDir}`);
        process.exit(1);
      }
    }

    // Render templates
    const rendered = TemplateLoader.renderCommand(spec);

    // Compute className once
    const className = spec.name.split('/').map(part =>
      part.charAt(0).toUpperCase() + part.slice(1).toLowerCase()
    ).join('');

    // Write shared types file
    const sharedDir = path.join(baseDir, 'shared');
    const sharedTypesPath = path.join(sharedDir, `${className}Types.ts`);
    TemplateLoader.writeToFile(sharedTypesPath, rendered.sharedTypes);
    console.log(`✅ Created: ${sharedTypesPath}`);

    // Write browser implementation file
    const browserDir = path.join(baseDir, 'browser');
    const browserPath = path.join(browserDir, `${className}BrowserCommand.ts`);
    TemplateLoader.writeToFile(browserPath, rendered.browser);
    console.log(`✅ Created: ${browserPath}`);

    // Write server implementation file
    const serverDir = path.join(baseDir, 'server');
    const serverPath = path.join(serverDir, `${className}ServerCommand.ts`);
    TemplateLoader.writeToFile(serverPath, rendered.server);
    console.log(`✅ Created: ${serverPath}`);

    // Write README file
    const readmePath = path.join(baseDir, 'README.md');
    TemplateLoader.writeToFile(readmePath, rendered.readme);
    console.log(`✅ Created: ${readmePath}`);

    // Write unit test file
    const unitTestDir = path.join(baseDir, 'test', 'unit');
    const unitTestPath = path.join(unitTestDir, `${className}Command.test.ts`);
    TemplateLoader.writeToFile(unitTestPath, rendered.unitTest);
    console.log(`✅ Created: ${unitTestPath}`);

    // Write integration test file
    const integrationTestDir = path.join(baseDir, 'test', 'integration');
    const integrationTestPath = path.join(integrationTestDir, `${className}Integration.test.ts`);
    TemplateLoader.writeToFile(integrationTestPath, rendered.integrationTest);
    console.log(`✅ Created: ${integrationTestPath}`);

    // Write package.json file
    const packageJsonPath = path.join(baseDir, 'package.json');
    TemplateLoader.writeToFile(packageJsonPath, rendered.packageJson);
    console.log(`✅ Created: ${packageJsonPath}`);

    // Write .npmignore file
    const npmignorePath = path.join(baseDir, '.npmignore');
    TemplateLoader.writeToFile(npmignorePath, rendered.npmignore);
    console.log(`✅ Created: ${npmignorePath}`);

    console.log(`\n🎉 Command generation complete!`);
    console.log(`📂 Files created in: ${baseDir}`);
    console.log(`\n📋 Next steps:`);
    console.log(`   1. Fill in unit tests (TDD): ${unitTestPath}`);
    console.log(`   2. Run tests: npx tsx ${unitTestPath}`);
    console.log(`   3. Implement command logic to pass tests`);
    console.log(`   4. Run integration tests after npm start`);
    console.log(`\n📦 Package commands:`);
    console.log(`   - cd ${baseDir} && npm test    (run all tests)`);
    console.log(`   - cd ${baseDir} && npm pack    (create .tgz package)`);
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

/**
 * Generate example CommandSpec JSON template
 */
function generateExampleSpec(): CommandSpec {
  return {
    name: 'example/command',
    description: 'Brief description of what this command does',
    params: [
      {
        name: 'requiredParam',
        type: 'string',
        optional: false,
        description: 'A required parameter'
      },
      {
        name: 'optionalParam',
        type: 'number',
        optional: true,
        description: 'An optional parameter'
      }
    ],
    results: [
      {
        name: 'outputValue',
        type: 'string',
        description: 'The result value returned by the command'
      },
      {
        name: 'timestamp',
        type: 'number',
        description: 'Unix timestamp of execution'
      }
    ],
    examples: [
      {
        description: 'Example usage with required parameter',
        command: './jtag example/command --requiredParam="value"',
        expectedResult: '{ outputValue: "processed", timestamp: 1234567890 }'
      }
    ],
    accessLevel: 'ai-safe'
  };
}

// CLI execution
if (require.main === module) {
  const args = process.argv.slice(2);

  if (args.length === 0) {
    console.error('Usage: npx tsx CommandGenerator.ts <spec-file.json> [output-dir] [--force] [--backup]');
    console.error('   or: npx tsx CommandGenerator.ts --test');
    console.error('   or: npx tsx CommandGenerator.ts --template');
    console.error('\nFlags:');
    console.error('  --force   Overwrite existing command');
    console.error('  --backup  Create backup before overwriting (requires --force)');
    process.exit(1);
  }

  const rootPath = path.join(__dirname, '..');
  const generator = new CommandGenerator(rootPath);

  if (args[0] === '--template') {
    // Template mode: output example JSON
    const exampleSpec = generateExampleSpec();
    console.log(JSON.stringify(exampleSpec, null, 2));
    console.log('\n📝 Copy the JSON above, edit it, save to a file, then run:');
    console.log('   npx tsx CommandGenerator.ts <your-spec-file.json>');
    process.exit(0);
  } else if (args[0] === '--test') {
    // Test mode: generate a sample command
    const testSpec: CommandSpec = {
      name: 'test/sample',
      description: 'Sample test command for generator testing',
      params: [
        {
          name: 'message',
          type: 'string',
          optional: false,
          description: 'Message to process'
        },
        {
          name: 'verbose',
          type: 'boolean',
          optional: true,
          description: 'Enable verbose output'
        }
      ],
      results: [
        {
          name: 'processedMessage',
          type: 'string',
          description: 'The processed message'
        },
        {
          name: 'timestamp',
          type: 'number',
          description: 'Unix timestamp of processing'
        }
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

    console.log('🧪 Test Mode: Generating sample command...\n');
    generator.generate(testSpec, '/tmp/generated-command-test');
  } else {
    // Normal mode: generate from spec file
    const specFile = args[0];

    // Parse flags and args
    const flagArgs = args.filter(a => a.startsWith('--'));
    const nonFlagArgs = args.filter(a => !a.startsWith('--'));

    const options = {
      force: flagArgs.includes('--force'),
      backup: flagArgs.includes('--backup')
    };

    const outputDir = nonFlagArgs[1]; // Second non-flag arg is output dir

    // Validate backup requires force
    if (options.backup && !options.force) {
      console.error('❌ ERROR: --backup requires --force');
      process.exit(1);
    }

    const fs = require('fs');
    const specJson = fs.readFileSync(specFile, 'utf-8');
    const spec: CommandSpec = JSON.parse(specJson);
    generator.generate(spec, outputDir, options);
  }
}

export { CommandSpec };
