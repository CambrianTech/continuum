/**
 * TokenBuilder - Generic string manipulation utilities
 *
 * Provides case conversion and formatting utilities independent of domain (commands/daemons/widgets).
 */

import type { CommandSpec, ParamSpec, ResultSpec, ExampleSpec, ImportSpec } from './CommandNaming';
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
      // Empty params: callers should use `buildParamsTypeDecl` to emit a
      // type alias instead of an empty interface. Returning '' here lets
      // legacy templates still compile, but new templates use the
      // dedicated decl builder so we never ship `_noParams?: never`
      // marker fields again (the lint workaround that became a typing
      // bug — TS sees the marker and refuses structural-equivalence
      // casts).
      return '';
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
   * Build the params TYPE DECLARATION block.
   *
   * For empty-params commands: emits a type alias to CommandParams
   * (genuinely empty + structurally identical). For non-empty: emits an
   * interface extending CommandParams with the typed fields.
   *
   * Replaces the old `interface FooParams extends CommandParams { _noParams?: never }`
   * pattern that:
   *   (a) lied about emptiness via the never marker
   *   (b) made the type structurally-incompatible with CommandParams
   *       so the factory's createPayload return required `as unknown as`
   *       casts to compile — which violated Joel's typing rule (no
   *       `unknown`, no `any`, types must be true to the wire shape)
   */
  static buildParamsTypeDecl(spec: CommandSpec): string {
    const naming = new CommandNaming(spec);
    if (spec.params.length === 0) {
      return `export type ${naming.paramsType} = CommandParams;`;
    }
    return `export interface ${naming.paramsType} extends CommandParams {\n${this.buildParamFields(spec.params)}\n}`;
  }

  /**
   * Build the params FACTORY function block.
   *
   * For empty-params commands: factory takes (context, sessionId, userId)
   * — userId is REQUIRED on CommandParams; createPayload wraps it cleanly
   * so the result is structurally CommandParams with NO casts needed.
   *
   * For non-empty: factory takes (context, sessionId, userId, data) where
   * data is the typed param fields. Same no-cast guarantee.
   */
  static buildParamsFactoryDecl(spec: CommandSpec): string {
    const naming = new CommandNaming(spec);
    if (spec.params.length === 0) {
      return [
        `export const create${naming.baseName}Params = (`,
        `  context: JTAGContext,`,
        `  sessionId: UUID,`,
        `  userId: UUID,`,
        `): ${naming.paramsType} => createPayload(context, sessionId, { userId });`,
      ].join('\n');
    }
    const dataType = this.buildFactoryDataType(spec.params);
    const defaults = this.buildFactoryDefaults(spec.params);
    const defaultsBlock = defaults ? `${defaults}\n` : '';
    return [
      `export const create${naming.baseName}Params = (`,
      `  context: JTAGContext,`,
      `  sessionId: UUID,`,
      `  userId: UUID,`,
      `  data: ${dataType},`,
      `): ${naming.paramsType} => createPayload(context, sessionId, {`,
      `  userId,`,
      `${defaultsBlock}  ...data,`,
      `});`,
    ].join('\n');
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
        const optional = result.optional ? '?' : '';
        const comment = result.description ? `  // ${result.description}\n` : '';
        return `${comment}  ${result.name}${optional}: ${result.type};`;
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
      return 'Record<string, never>';
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
        const defaultValue = this.defaultValueForType(param.type);
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

    results.forEach(result => {
      const optional = result.optional ? '?' : '';
      const comment = result.description ? `    // ${result.description}\n` : '';
      fields.push(`${comment}    ${result.name}${optional}: ${result.type};`);
    });

    // error is always optional
    fields.push('    error?: JTAGError;');

    return `{\n${fields.join('\n')}\n  }`;
  }

  /**
   * Build default value assignments for result fields in factory functions
   */
  static buildResultFactoryDefaults(results: ResultSpec[]): string {
    const optionalResults = results.filter(result => result.optional);
    if (optionalResults.length === 0) {
      return '';
    }

    return optionalResults
      .map(result => {
        // Generate sensible defaults based on type
        const defaultValue = this.defaultValueForType(result.type);
        return `  ${result.name}: data.${result.name} ?? ${defaultValue},`;
      })
      .join('\n');
  }

  static buildImportStatements(imports: ImportSpec[] | undefined): string {
    if (!imports || imports.length === 0) return '';
    return imports
      .map(importSpec => {
        const typeOnly = importSpec.typeOnly ?? true;
        const prefix = typeOnly ? 'import type' : 'import';
        return `${prefix} { ${importSpec.names.join(', ')} } from '${importSpec.from}';`;
      })
      .join('\n');
  }

  /**
   * Get a sensible default value for a TypeScript type.
   * Used only for optional factory fields; required result fields are caller-owned.
   */
  static defaultValueForType(type: string): string {
    if (type === 'boolean') return 'false';
    if (type === 'number') return '0';
    if (type === 'string') return "''";
    if (type === 'object') return '{}';
    if (type.endsWith('[]') || type.startsWith('Array<')) return '[]';
    if (type.startsWith('Record<')) return '{}';
    return 'undefined';
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
      // Type-safe replacements for the legacy
      // `interface Foo extends CommandParams { _noParams: never }`
      // + cast-laden factory pattern. See buildParamsTypeDecl /
      // buildParamsFactoryDecl for the rationale.
      PARAMS_TYPE_DECL: this.buildParamsTypeDecl(spec),
      PARAMS_FACTORY_DECL: this.buildParamsFactoryDecl(spec),
      RESULT_FACTORY_DATA_TYPE: this.buildResultFactoryDataType(spec.results),
      RESULT_FACTORY_DEFAULTS: this.buildResultFactoryDefaults(spec.results),
      EXTRA_IMPORTS: this.buildImportStatements(spec.imports),
      RESULT_FIELD_EXAMPLES: this.buildResultFieldExamples(spec.results)
    };
  }
}
