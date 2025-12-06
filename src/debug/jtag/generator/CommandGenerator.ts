/**
 * CommandGenerator - Generate command files using template system
 *
 * Phase 1 of generator refactoring: Integrate TemplateLoader for command generation
 * without touching the existing generate-structure.ts
 */

import { TemplateLoader } from './TemplateLoader';
import type { CommandSpec } from './TokenBuilder';
import * as path from 'path';

export class CommandGenerator {
  private rootPath: string;

  constructor(rootPath: string) {
    this.rootPath = rootPath;
  }

  /**
   * Generate command files from a CommandSpec
   */
  generate(spec: CommandSpec, outputDir?: string): void {
    console.log(`📝 Generating files for command: ${spec.name}`);

    // Render templates
    const rendered = TemplateLoader.renderCommand(spec);

    // Determine output directory
    const baseDir = outputDir ?? path.join(this.rootPath, 'commands', spec.name);

    // Write shared types file
    const sharedDir = path.join(baseDir, 'shared');
    const className = spec.name.split('/').map(part =>
      part.charAt(0).toUpperCase() + part.slice(1).toLowerCase()
    ).join('');

    const sharedTypesPath = path.join(sharedDir, `${className}Types.ts`);
    TemplateLoader.writeToFile(sharedTypesPath, rendered.sharedTypes);
    console.log(`✅ Created: ${sharedTypesPath}`);

    // Write README file
    const readmePath = path.join(baseDir, 'README.md');
    TemplateLoader.writeToFile(readmePath, rendered.readme);
    console.log(`✅ Created: ${readmePath}`);

    console.log(`\n🎉 Command generation complete!`);
    console.log(`📂 Files created in: ${baseDir}`);
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

// CLI execution
if (require.main === module) {
  const args = process.argv.slice(2);

  if (args.length === 0) {
    console.error('Usage: npx tsx CommandGenerator.ts <spec-file.json> [output-dir]');
    console.error('   or: npx tsx CommandGenerator.ts --test');
    process.exit(1);
  }

  const rootPath = path.join(__dirname, '..');
  const generator = new CommandGenerator(rootPath);

  if (args[0] === '--test') {
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
    const outputDir = args[1];
    generator.generateFromFile(specFile, outputDir);
  }
}

export { CommandSpec };
