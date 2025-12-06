/**
 * TokenBuilder - Helper functions to generate token values for template replacement
 *
 * Provides utilities to convert user input into properly formatted token values.
 */

export interface CommandSpec {
  name: string;           // e.g., "docs/read" or "Screenshot"
  description: string;    // Human-readable description
  params: ParamSpec[];    // Parameter definitions
  results: ResultSpec[];  // Result field definitions
  examples?: ExampleSpec[];
  accessLevel?: 'ai-safe' | 'internal' | 'system' | 'dangerous';
}

export interface ParamSpec {
  name: string;
  type: string;
  optional?: boolean;
  description?: string;
}

export interface ResultSpec {
  name: string;
  type: string;
  description?: string;
}

export interface ExampleSpec {
  description: string;
  command: string;
  expectedResult?: string;
}

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
  static buildExamples(examples: ExampleSpec[]): string {
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
   * Build all tokens for a command from specification
   */
  static buildAllTokens(spec: CommandSpec): Record<string, string> {
    const className = this.toClassName(spec.name);
    const commandName = this.toCommandName(spec.name);
    const commandPath = spec.name.toLowerCase();

    return {
      COMMAND_NAME: commandName,
      DESCRIPTION: spec.description,
      CLASS_NAME: className,
      COMMAND_PATH: commandPath,
      PARAM_FIELDS: this.buildParamFields(spec.params),
      RESULT_FIELDS: this.buildResultFields(spec.results),
      PARAM_DOCS: this.buildParamDocs(spec.params),
      RESULT_DOCS: this.buildResultDocs(spec.results),
      EXAMPLES: this.buildExamples(spec.examples),
      EXAMPLE_ARGS: this.buildExampleArgs(spec.params),
      ACCESS_LEVEL: spec.accessLevel || 'internal',
      ACCESS_LEVEL_DESCRIPTION: this.getAccessLevelDescription(spec.accessLevel || 'internal')
    };
  }
}
