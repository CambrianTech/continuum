/**
 * README Generator
 *
 * Generates comprehensive README.md from command types
 */

import * as fs from 'fs';
import * as path from 'path';

export interface CommandSchema {
  name: string;
  description?: string;
  params?: Record<string, any>;
  result?: Record<string, any>;
}

export class ReadmeGenerator {
  /**
   * Generate README content from command schema
   */
  static generate(modulePath: string, moduleType: string): string {
    const moduleName = path.basename(modulePath);
    const commandName = moduleName.replace(/^@jtag-commands\//, '');

    // Try to load schema from Types file
    const schema = this.loadSchema(modulePath);

    return `# ${this.formatTitle(commandName)}

${schema.description || `${commandName} command for JTAG system`}

## Usage

\`\`\`bash
./jtag ${commandName.replace(/-/g, '/')} [options]
\`\`\`

## Parameters

${this.generateParamsSection(schema.params)}

## Result

${this.generateResultSection(schema.result)}

## Examples

### Basic Usage

\`\`\`bash
# Example 1: Basic invocation
./jtag ${commandName.replace(/-/g, '/')}

# Example 2: With parameters
./jtag ${commandName.replace(/-/g, '/')} --param=value
\`\`\`

### Programmatic Usage

\`\`\`typescript
import { Commands } from '@system/core/shared/Commands';

const result = await Commands.execute('${commandName.replace(/-/g, '/')}', {
  // parameters here
});

console.log(result);
\`\`\`

## Testing

### Unit Tests

\`\`\`bash
npm run test:unit
\`\`\`

### Integration Tests

\`\`\`bash
npm run test:integration
\`\`\`

## Development

### Build

\`\`\`bash
npm run build
\`\`\`

### Lint

\`\`\`bash
npm run lint
\`\`\`

## Package

This command is packaged as an npm module and can be installed independently:

\`\`\`bash
npm install @jtag-commands/${commandName}
\`\`\`

## License

MIT
`;
  }

  /**
   * Load command schema from Types file
   */
  private static loadSchema(modulePath: string): CommandSchema {
    const absolutePath = path.resolve(modulePath);
    const sharedDir = path.join(absolutePath, 'shared');

    // Default schema
    const schema: CommandSchema = {
      name: path.basename(modulePath),
      description: '',
      params: {},
      result: {},
    };

    if (!fs.existsSync(sharedDir)) {
      return schema;
    }

    // Find Types.ts file
    const files = fs.readdirSync(sharedDir);
    const typesFile = files.find((f) => f.endsWith('Types.ts'));

    if (!typesFile) {
      return schema;
    }

    // Read the types file
    const typesPath = path.join(sharedDir, typesFile);
    const content = fs.readFileSync(typesPath, 'utf-8');

    // Extract description from comments
    const descMatch = content.match(/\/\*\*\s*\n\s*\*\s*(.+?)\s*\n/);
    if (descMatch) {
      schema.description = descMatch[1];
    }

    // Extract params interface
    const paramsMatch = content.match(/export interface \w+Params[^{]*{([^}]+)}/s);
    if (paramsMatch) {
      schema.params = this.parseInterface(paramsMatch[1]);
    }

    // Extract result interface
    const resultMatch = content.match(/export interface \w+Result[^{]*{([^}]+)}/s);
    if (resultMatch) {
      schema.result = this.parseInterface(resultMatch[1]);
    }

    return schema;
  }

  /**
   * Parse TypeScript interface to extract fields
   */
  private static parseInterface(interfaceBody: string): Record<string, any> {
    const fields: Record<string, any> = {};
    const lines = interfaceBody.split('\n');

    for (const line of lines) {
      const trimmed = line.trim();
      if (!trimmed || trimmed.startsWith('//') || trimmed.startsWith('/*')) {
        continue;
      }

      // Match field: type pattern
      const match = trimmed.match(/^(\w+)(\?)?:\s*([^;]+);?/);
      if (match) {
        const [, fieldName, optional, fieldType] = match;
        fields[fieldName] = {
          type: fieldType.trim(),
          optional: !!optional,
        };
      }
    }

    return fields;
  }

  /**
   * Generate parameters section
   */
  private static generateParamsSection(params?: Record<string, any>): string {
    if (!params || Object.keys(params).length === 0) {
      return 'No parameters required.';
    }

    let section = '| Parameter | Type | Required | Description |\n';
    section += '|-----------|------|----------|-------------|\n';

    for (const [name, info] of Object.entries(params)) {
      const required = info.optional ? 'No' : 'Yes';
      section += `| \`${name}\` | \`${info.type}\` | ${required} | - |\n`;
    }

    return section;
  }

  /**
   * Generate result section
   */
  private static generateResultSection(result?: Record<string, any>): string {
    if (!result || Object.keys(result).length === 0) {
      return 'Returns execution result.';
    }

    let section = '| Field | Type | Description |\n';
    section += '|-------|------|-------------|\n';

    for (const [name, info] of Object.entries(result)) {
      section += `| \`${name}\` | \`${info.type}\` | - |\n`;
    }

    return section;
  }

  /**
   * Format title from command name
   */
  private static formatTitle(commandName: string): string {
    return commandName
      .split(/[-/]/)
      .map((word) => word.charAt(0).toUpperCase() + word.slice(1))
      .join(' ') + ' Command';
  }
}
