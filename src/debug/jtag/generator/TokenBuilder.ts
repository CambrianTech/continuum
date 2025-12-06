/**
 * TokenBuilder - Generic string manipulation utilities
 *
 * Provides case conversion and formatting utilities independent of domain (commands/daemons/widgets).
 */

import type { CommandSpec, ParamSpec, ResultSpec, ExampleSpec } from './CommandNaming';
import { CommandNaming } from './CommandNaming';

export class TokenBuilder {
  /**
   * Convert command name to PascalCase class name
   *
   * @example
   * toClassName("docs/read") => "DocsRead"
   * toClassName("screenshot") => "Screenshot"
   * toClassName("ai-mute") => "AiMute"
   */
  static toClassName(commandName: string): string {
    return commandName
      .split(/[\/\-_]/)
      .map(part => part.charAt(0).toUpperCase() + part.slice(1).toLowerCase())
      .join('');
  }

  /**
   * Convert command name to human-readable format
   *
   * @example
   * toCommandName("docs/read") => "Docs Read"
   * toCommandName("screenshot") => "Screenshot"
   */
  static toCommandName(commandName: string): string {
    return commandName
      .split(/[\/\-_]/)
      .map(part => part.charAt(0).toUpperCase() + part.slice(1).toLowerCase())
      .join(' ');
  }

  /**
   * Build parameter fields for interface definition
   *
   * @example
   * buildParamFields([
   *   { name: 'command', type: 'string', optional: false },
   *   { name: 'format', type: 'string', optional: true }
   * ])
   * => "  command: string;\n  format?: string;"
   */
  static buildParamFields(params: ParamSpec[]): string {
    if (params.length === 0) {
      return '  // No additional parameters';
    }

    return params
      .map(param => {
        const optional = param.optional ? '?' : '';
        const comment = param.description ? `  // ${param.description}\n` : '';
        return `${comment}  ${param.name}${optional}: ${param.type};`;
      })
      .join('\n');
  }

  /**
   * Build result fields for interface definition
   */
  static buildResultFields(results: ResultSpec[]): string {
    if (results.length === 0) {
      return '  // No additional result fields';
    }

    return results
      .map(result => {
        const comment = result.description ? `  // ${result.description}\n` : '';
        return `${comment}  ${result.name}: ${result.type};`;
      })
      .join('\n');
  }

  /**
   * Build parameter documentation for README
   */
  static buildParamDocs(params: ParamSpec[]): string {
    if (params.length === 0) {
      return 'No parameters required.';
    }

    return params
      .map(param => {
        const required = param.optional ? '(optional)' : '(required)';
        const desc = param.description || 'No description';
        return `- **${param.name}** ${required}: \`${param.type}\` - ${desc}`;
      })
      .join('\n');
  }

  /**
   * Build result documentation for README
   */
  static buildResultDocs(results: ResultSpec[]): string {
    if (results.length === 0) {
      return 'Returns standard CommandResult with success flag.';
    }

    const fields = results
      .map(result => {
        const desc = result.description || 'No description';
        return `- **${result.name}**: \`${result.type}\` - ${desc}`;
      })
      .join('\n');

    return `Returns CommandResult with:\n${fields}`;
  }

  /**
   * Build example usage for README
   */
  static buildExamples(examples: ExampleSpec[] | undefined): string {
    if (!examples || examples.length === 0) {
      return '```bash\n./jtag command-name\n```';
    }

    return examples
      .map(ex => {
        const result = ex.expectedResult
          ? `\n\n**Expected result:**\n${ex.expectedResult}`
          : '';
        return `### ${ex.description}\n\n\`\`\`bash\n${ex.command}\n\`\`\`${result}`;
      })
      .join('\n\n');
  }

  /**
   * Get access level description
   */
  static getAccessLevelDescription(level: string): string {
    const descriptions: Record<string, string> = {
      'ai-safe': 'Safe for AI personas to call autonomously',
      'internal': 'Internal use only, not exposed to AI personas',
      'system': 'System-level command, requires elevated permissions',
      'dangerous': 'Potentially destructive, requires explicit user confirmation'
    };
    return descriptions[level] || 'Unknown access level';
  }

  /**
   * Build example arguments string for README usage section
   */
  static buildExampleArgs(params: ParamSpec[]): string {
    if (params.length === 0) {
      return '';
    }

    const required = params.filter(p => !p.optional);
    if (required.length === 0) {
      return '[options]';
    }

    return required
      .map(p => `--${p.name}=<value>`)
      .join(' ');
  }

  /**
   * Build factory function data parameter type for createParams
   * Explicitly types required fields without ? and optional fields with ?
   *
   * @example
   * buildFactoryDataType([
   *   { name: 'selector', type: 'string', optional: false },
   *   { name: 'timeout', type: 'number', optional: true }
   * ])
   * => "{\n    selector: string;\n    timeout?: number;\n  }"
   */
  static buildFactoryDataType(params: ParamSpec[]): string {
    if (params.length === 0) {
      return '{}';
    }

    const fields = params
      .map(param => {
        const optional = param.optional ? '?' : '';
        const comment = param.description ? `    // ${param.description}\n` : '';
        return `${comment}    ${param.name}${optional}: ${param.type};`;
      })
      .join('\n');

    return `{\n${fields}\n  }`;
  }

  /**
   * Build default value assignments for optional parameters in factory functions
   *
   * @example
   * buildFactoryDefaults([
   *   { name: 'timeout', type: 'number', optional: true },
   *   { name: 'button', type: "'left' | 'right'", optional: true }
   * ])
   * => "  timeout: data.timeout ?? 30000,\n  button: data.button ?? 'left',"
   */
  static buildFactoryDefaults(params: ParamSpec[]): string {
    const optionalParams = params.filter(p => p.optional);

    if (optionalParams.length === 0) {
      return '';
    }

    return optionalParams
      .map(param => {
        // Generate sensible defaults based on type
        let defaultValue: string;
        if (param.type === 'boolean') {
          defaultValue = 'false';
        } else if (param.type === 'number') {
          defaultValue = '0';
        } else if (param.type === 'string') {
          defaultValue = "''";
        } else {
          defaultValue = 'undefined';
        }

        return `  ${param.name}: data.${param.name} ?? ${defaultValue},`;
      })
      .join('\n');
  }

  /**
   * Build factory function data parameter type for createResult
   * Result fields are typically more flexible (success required, most others optional)
   */
  static buildResultFactoryDataType(results: ResultSpec[]): string {
    // success is always required in result factories
    const fields = ['    success: boolean;'];

    // All other result fields are typically optional (for error cases)
    results.forEach(result => {
      const comment = result.description ? `    // ${result.description}\n` : '';
      fields.push(`${comment}    ${result.name}?: ${result.type};`);
    });

    // error is always optional
    fields.push('    error?: JTAGError;');

    return `{\n${fields.join('\n')}\n  }`;
  }

  /**
   * Build default value assignments for result fields in factory functions
   */
  static buildResultFactoryDefaults(results: ResultSpec[]): string {
    if (results.length === 0) {
      return '';
    }

    return results
      .map(result => {
        // Generate sensible defaults based on type
        let defaultValue: string;
        if (result.type === 'boolean') {
          defaultValue = 'false';
        } else if (result.type === 'number') {
          defaultValue = '0';
        } else if (result.type === 'string') {
          defaultValue = "''";
        } else {
          defaultValue = 'undefined';
        }

        return `  ${result.name}: data.${result.name} ?? ${defaultValue},`;
      })
      .join('\n');
  }

  /**
   * Build example result field assignments for server template
   * Generates placeholder values based on result field types
   */
  static buildResultFieldExamples(results: ResultSpec[]): string {
    if (results.length === 0) {
      return '// No additional result fields needed';
    }

    return results
      .map(result => {
        let exampleValue: string;

        if (result.type === 'string') {
          exampleValue = `'TODO: ${result.description || result.name}'`;
        } else if (result.type === 'number') {
          exampleValue = result.name.toLowerCase().includes('time') ? 'Date.now()' : '0';
        } else if (result.type === 'boolean') {
          exampleValue = 'true';
        } else {
          exampleValue = `{} /* TODO: ${result.type} */`;
        }

        const comment = result.description ? ` // ${result.description}` : '';
        return `      ${result.name}: ${exampleValue},${comment}`;
      })
      .join('\n');
  }

  /**
   * Build all tokens for a command from specification
   * Uses CommandNaming for command-specific naming conventions
   */
  static buildAllTokens(spec: CommandSpec): Record<string, string> {
    const naming = new CommandNaming(spec);
    const commandName = this.toCommandName(spec.name);

    return {
      COMMAND_NAME: commandName,
      DESCRIPTION: spec.description,
      CLASS_NAME: naming.baseName,
      PARAMS_TYPE: naming.paramsType,
      RESULT_TYPE: naming.resultType,
      SERVER_CLASS: naming.serverClass,
      BROWSER_CLASS: naming.browserClass,
      COMMAND_PATH: spec.name.toLowerCase(),
      PARAM_FIELDS: this.buildParamFields(spec.params),
      RESULT_FIELDS: this.buildResultFields(spec.results),
      PARAM_DOCS: this.buildParamDocs(spec.params),
      RESULT_DOCS: this.buildResultDocs(spec.results),
      EXAMPLES: this.buildExamples(spec.examples),
      EXAMPLE_ARGS: this.buildExampleArgs(spec.params),
      ACCESS_LEVEL: spec.accessLevel || 'internal',
      ACCESS_LEVEL_DESCRIPTION: this.getAccessLevelDescription(spec.accessLevel || 'internal'),
      IMPLEMENTATION: naming.implementation,
      FACTORY_DATA_TYPE: this.buildFactoryDataType(spec.params),
      FACTORY_DEFAULTS: this.buildFactoryDefaults(spec.params),
      RESULT_FACTORY_DATA_TYPE: this.buildResultFactoryDataType(spec.results),
      RESULT_FACTORY_DEFAULTS: this.buildResultFactoryDefaults(spec.results),
      RESULT_FIELD_EXAMPLES: this.buildResultFieldExamples(spec.results)
    };
  }
}
