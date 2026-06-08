/**
 * Command Spec Abstraction Layer
 *
 * TypeScript interfaces for command specifications used by the generator.
 * Provides type safety, validation, and serialization for command specs.
 */

/**
 * Parameter specification for a command
 */
export interface ParamSpec {
  /** Parameter name (e.g., "proposalId") */
  name: string;

  /** TypeScript type (e.g., "string", "number", "UUID", "DecisionOption[]") */
  type: string;

  /** Human-readable description of what this parameter does */
  description: string;

  /** Whether this parameter is required */
  required: boolean;

  /** Optional default value */
  defaultValue?: any;
}

/**
 * Result field specification for a command
 */
export interface ResultSpec {
  /** Result field name (e.g., "success", "message") */
  name: string;

  /** TypeScript type (e.g., "boolean", "string", "UserEntity") */
  type: string;

  /** Human-readable description of what this field means */
  description: string;
}

/**
 * Complete command specification
 */
export interface CommandSpec {
  /** Command name (e.g., "hello", "decision/create") */
  name: string;

  /** Human-readable description of what the command does */
  description: string;

  /** Array of parameter specifications */
  params: ParamSpec[];

  /** Array of result field specifications */
  results: ResultSpec[];

  /** Optional array of CLI usage examples */
  examples?: string[];

  /** Access level for the command */
  accessLevel?: 'ai-safe' | 'admin-only' | 'public';
}

/**
 * Validation result with success flag and error messages
 */
export interface ValidationResult {
  /** Whether validation passed */
  valid: boolean;

  /** Array of error messages (empty if valid) */
  errors: string[];

  /** Array of warning messages (validation passed but with warnings) */
  warnings: string[];
}

/**
 * Helper methods for ParamSpec
 */
export class ParamSpecHelper {
  /**
   * Check if a parameter is required
   */
  static isRequired(param: ParamSpec): boolean {
    return param.required === true;
  }

  /**
   * Check if a parameter has a default value
   */
  static hasDefault(param: ParamSpec): boolean {
    return param.defaultValue !== undefined;
  }

  /**
   * Get display name for parameter (with required indicator)
   */
  static getDisplayName(param: ParamSpec): string {
    return param.required ? `${param.name}*` : param.name;
  }

  /**
   * Validate parameter spec has all required fields
   */
  static validate(param: ParamSpec): ValidationResult {
    const errors: string[] = [];
    const warnings: string[] = [];

    if (!param.name || param.name.trim() === '') {
      errors.push('Parameter name is required');
    }

    if (!param.type || param.type.trim() === '') {
      errors.push(`Parameter '${param.name}': type is required`);
    }

    if (!param.description || param.description.trim() === '') {
      errors.push(`Parameter '${param.name}': description is required`);
    }

    if (param.required === undefined) {
      errors.push(`Parameter '${param.name}': required field must be explicitly set (true or false)`);
    }

    // Warning: optional parameter with default value
    if (param.required === false && param.defaultValue === undefined) {
      warnings.push(`Parameter '${param.name}': optional parameter without default value`);
    }

    // Warning: required parameter with default value
    if (param.required === true && param.defaultValue !== undefined) {
      warnings.push(`Parameter '${param.name}': required parameter has default value (defaults are typically for optional params)`);
    }

    return {
      valid: errors.length === 0,
      errors,
      warnings
    };
  }
}

/**
 * Helper methods for ResultSpec
 */
export class ResultSpecHelper {
  /**
   * Validate result spec has all required fields
   */
  static validate(result: ResultSpec): ValidationResult {
    const errors: string[] = [];
    const warnings: string[] = [];

    if (!result.name || result.name.trim() === '') {
      errors.push('Result field name is required');
    }

    if (!result.type || result.type.trim() === '') {
      errors.push(`Result field '${result.name}': type is required`);
    }

    if (!result.description || result.description.trim() === '') {
      errors.push(`Result field '${result.name}': description is required`);
    }

    return {
      valid: errors.length === 0,
      errors,
      warnings
    };
  }
}

/**
 * Helper methods for CommandSpec
 */
export class CommandSpecHelper {
  /**
   * Validate command spec has all required fields and valid structure
   */
  static validate(spec: CommandSpec): ValidationResult {
    const errors: string[] = [];
    const warnings: string[] = [];

    // Validate name
    if (!spec.name || spec.name.trim() === '') {
      errors.push('Command name is required');
    }

    // Validate description
    if (!spec.description || spec.description.trim() === '') {
      errors.push('Command description is required');
    }

    // Validate params array
    if (!spec.params) {
      errors.push('params array is required (use empty array [] if no params)');
    } else {
      // Validate each parameter
      spec.params.forEach((param, index) => {
        const paramValidation = ParamSpecHelper.validate(param);
        if (!paramValidation.valid) {
          errors.push(...paramValidation.errors.map(err => `params[${index}]: ${err}`));
        }
        warnings.push(...paramValidation.warnings.map(warn => `params[${index}]: ${warn}`));
      });

      // Check for duplicate parameter names
      const paramNames = spec.params.map(p => p.name);
      const duplicates = paramNames.filter((name, index) => paramNames.indexOf(name) !== index);
      if (duplicates.length > 0) {
        errors.push(`Duplicate parameter names: ${duplicates.join(', ')}`);
      }
    }

    // Validate results array
    if (!spec.results) {
      errors.push('results array is required (use empty array [] if no results)');
    } else {
      // Validate each result field
      spec.results.forEach((result, index) => {
        const resultValidation = ResultSpecHelper.validate(result);
        if (!resultValidation.valid) {
          errors.push(...resultValidation.errors.map(err => `results[${index}]: ${err}`));
        }
        warnings.push(...resultValidation.warnings.map(warn => `results[${index}]: ${warn}`));
      });

      // Check for duplicate result field names
      const resultNames = spec.results.map(r => r.name);
      const duplicates = resultNames.filter((name, index) => resultNames.indexOf(name) !== index);
      if (duplicates.length > 0) {
        errors.push(`Duplicate result field names: ${duplicates.join(', ')}`);
      }

      // Check if 'success' field exists (convention)
      if (!spec.results.some(r => r.name === 'success')) {
        warnings.push('Result fields typically include a "success: boolean" field');
      }
    }

    // Validate examples (if provided)
    if (spec.examples && spec.examples.length > 0) {
      spec.examples.forEach((example, index) => {
        if (!example || example.trim() === '') {
          errors.push(`examples[${index}]: empty example string`);
        }
        // Check if example looks like a CLI command
        if (example && !example.includes('./jtag') && !example.includes('jtag ')) {
          warnings.push(`examples[${index}]: example doesn't look like a CLI command (missing './jtag')`);
        }
      });
    }

    // Validate accessLevel (if provided)
    if (spec.accessLevel) {
      const validLevels = ['ai-safe', 'admin-only', 'public'];
      if (!validLevels.includes(spec.accessLevel)) {
        errors.push(`accessLevel must be one of: ${validLevels.join(', ')} (got: ${spec.accessLevel})`);
      }
    }

    return {
      valid: errors.length === 0,
      errors,
      warnings
    };
  }

  /**
   * Convert spec to JSON string
   */
  static toJSON(spec: CommandSpec, pretty: boolean = false): string {
    return pretty
      ? JSON.stringify(spec, null, 2)
      : JSON.stringify(spec);
  }

  /**
   * Parse JSON string to spec
   */
  static fromJSON(json: string): CommandSpec {
    return JSON.parse(json) as CommandSpec;
  }
}
