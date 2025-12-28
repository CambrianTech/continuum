/**
 * Propose Command - Server Implementation
 *
 * Allows AI personas to generate, edit, and execute commands in their sandbox.
 * No recompile needed - uses dynamic tsx execution.
 *
 * Sandbox locations:
 * - Generated: .continuum/personas/{uniqueId}/sandbox/commands/{name}/
 * - Executed via: npx tsx {path}/server/{Name}ServerCommand.ts
 */

import { CommandBase } from '@daemons/command-daemon/shared/CommandBase';
import type { JTAGContext } from '@system/core/types/JTAGTypes';
import type { ICommandDaemon } from '@daemons/command-daemon/shared/CommandBase';
import type { ProposeCommandParams, ProposeCommandResult, CommandSpec } from '../shared/ProposeCommandTypes';
import { createProposeCommandResult } from '../shared/ProposeCommandTypes';
import { SystemPaths } from '@system/core/config/SystemPaths';
import * as fs from 'fs';
import * as path from 'path';

export class ProposeCommandServerCommand extends CommandBase<ProposeCommandParams, ProposeCommandResult> {
  constructor(context: JTAGContext, subpath: string, commander: ICommandDaemon) {
    super('development/propose-command', context, subpath, commander);
  }

  async execute(params: ProposeCommandParams): Promise<ProposeCommandResult> {
    const errors: string[] = [];
    const warnings: string[] = [];

    try {
      // Validate spec
      const validationErrors = this.validateSpec(params.spec);
      if (validationErrors.length > 0) {
        return createProposeCommandResult(this.context, this.context.uuid, {
          success: false,
          errors: validationErrors
        });
      }

      // Get caller's persona uniqueId from context
      const callerUniqueId = this.context.callerType === 'persona'
        ? (this.context as any).personaUniqueId || 'anonymous'
        : 'human';

      // Build sandbox path - SystemPaths.root is already .continuum
      const sandboxBase = path.join(
        SystemPaths.personas.dir(callerUniqueId),
        'sandbox',
        'commands',
        params.spec.name
      );

      // Check if already exists
      if (fs.existsSync(sandboxBase) && !params.force) {
        return createProposeCommandResult(this.context, this.context.uuid, {
          success: false,
          errors: [`Command already exists at ${sandboxBase}. Use force=true to overwrite.`],
          generatedPath: sandboxBase
        });
      }

      // Dry run - just validate
      if (params.dryRun) {
        return createProposeCommandResult(this.context, this.context.uuid, {
          success: true,
          commandNamespace: `persona:${callerUniqueId}/${params.spec.name}`,
          warnings: ['Dry run - no files generated'],
          nextSteps: [
            'Remove dryRun=true to generate the command',
            'After generation, use development/sandbox-execute to test'
          ]
        });
      }

      // Generate the command files
      await this.generateCommand(params.spec, sandboxBase);

      const commandNamespace = `persona:${callerUniqueId}/${params.spec.name}`;

      return createProposeCommandResult(this.context, this.context.uuid, {
        success: true,
        commandNamespace,
        generatedPath: sandboxBase,
        nextSteps: [
          `Edit the implementation: ${sandboxBase}/server/${this.toClassName(params.spec.name)}ServerCommand.ts`,
          `Test with: ./jtag development/sandbox-execute --path="${sandboxBase}" --params='{}'`,
          `When ready, ask Joel to promote to main codebase`
        ]
      });

    } catch (error) {
      const errorMessage = error instanceof Error ? error.message : String(error);
      return createProposeCommandResult(this.context, this.context.uuid, {
        success: false,
        errors: [errorMessage]
      });
    }
  }

  /**
   * Validate command spec
   */
  private validateSpec(spec: CommandSpec): string[] {
    const errors: string[] = [];

    if (!spec.name || spec.name.trim().length === 0) {
      errors.push('Command name is required');
    } else if (!/^[a-z][a-z0-9-]*$/.test(spec.name)) {
      errors.push('Command name must be lowercase alphanumeric with hyphens (e.g., "analyze-code")');
    }

    if (!spec.description || spec.description.trim().length === 0) {
      errors.push('Command description is required');
    }

    if (!Array.isArray(spec.params)) {
      errors.push('params must be an array');
    }

    if (!Array.isArray(spec.results)) {
      errors.push('results must be an array');
    }

    return errors;
  }

  /**
   * Generate command files to sandbox
   */
  private async generateCommand(spec: CommandSpec, outputDir: string): Promise<void> {
    const className = this.toClassName(spec.name);

    // Create directories
    fs.mkdirSync(path.join(outputDir, 'shared'), { recursive: true });
    fs.mkdirSync(path.join(outputDir, 'server'), { recursive: true });

    // Generate Types file
    const typesContent = this.generateTypesFile(spec, className);
    fs.writeFileSync(path.join(outputDir, 'shared', `${className}Types.ts`), typesContent);

    // Generate Server Command file
    const serverContent = this.generateServerFile(spec, className);
    fs.writeFileSync(path.join(outputDir, 'server', `${className}ServerCommand.ts`), serverContent);

    // Generate README
    const readmeContent = this.generateReadme(spec);
    fs.writeFileSync(path.join(outputDir, 'README.md'), readmeContent);

    console.log(`✅ Generated sandbox command: ${outputDir}`);
  }

  /**
   * Convert command name to class name
   */
  private toClassName(name: string): string {
    return name.split('-').map(part =>
      part.charAt(0).toUpperCase() + part.slice(1)
    ).join('');
  }

  /**
   * Generate Types file content
   */
  private generateTypesFile(spec: CommandSpec, className: string): string {
    const paramFields = spec.params.map(p =>
      `  /** ${p.description || p.name} */\n  ${p.name}${p.optional ? '?' : ''}: ${p.type};`
    ).join('\n\n');

    const resultFields = spec.results.map(r =>
      `  /** ${r.description || r.name} */\n  ${r.name}: ${r.type};`
    ).join('\n\n');

    return `/**
 * ${className} Types - Generated by AI Sandbox
 *
 * ${spec.description}
 */

export interface ${className}Params {
${paramFields}
}

export interface ${className}Result {
  success: boolean;
${resultFields}
  error?: string;
}
`;
  }

  /**
   * Generate Server Command file content
   */
  private generateServerFile(spec: CommandSpec, className: string): string {
    const notes = spec.implementationNotes
      ? `\n * Implementation Notes:\n * ${spec.implementationNotes.split('\n').join('\n * ')}\n *`
      : '';

    return `/**
 * ${className} Server Command - Generated by AI Sandbox
 *
 * ${spec.description}
 *${notes}
 * TODO: Implement the execute() method logic
 */

import type { ${className}Params, ${className}Result } from '../shared/${className}Types';

export async function execute(params: ${className}Params): Promise<${className}Result> {
  try {
    // TODO: Implement your command logic here
    console.log('🔧 ${className}: Executing with params:', params);

    // Example implementation - replace with real logic
    return {
      success: true,
      // Add your result fields here
    };

  } catch (error) {
    const errorMessage = error instanceof Error ? error.message : String(error);
    return {
      success: false,
      error: errorMessage
    };
  }
}

// Allow direct execution for testing
if (require.main === module) {
  const args = process.argv.slice(2);
  const params = args[0] ? JSON.parse(args[0]) : {};
  execute(params).then(result => {
    console.log(JSON.stringify(result, null, 2));
  });
}
`;
  }

  /**
   * Generate README content
   */
  private generateReadme(spec: CommandSpec): string {
    const examples = spec.examples?.map(e =>
      `### ${e.description}\n\`\`\`bash\n./jtag development/sandbox-execute --path="." --params='${JSON.stringify(e.params)}'\n\`\`\``
    ).join('\n\n') || 'No examples provided.';

    return `# ${spec.name}

${spec.description}

## Generated by AI Sandbox

This command was proposed by an AI persona and generated into their sandbox.

## Parameters

${spec.params.map(p => `- **${p.name}** (${p.type}${p.optional ? ', optional' : ''}): ${p.description || 'No description'}`).join('\n')}

## Results

${spec.results.map(r => `- **${r.name}** (${r.type}): ${r.description || 'No description'}`).join('\n')}

## Examples

${examples}

## Testing

\`\`\`bash
npx tsx server/${this.toClassName(spec.name)}ServerCommand.ts '{"param": "value"}'
\`\`\`
`;
  }
}
