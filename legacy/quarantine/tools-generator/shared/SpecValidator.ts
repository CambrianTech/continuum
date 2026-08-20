/**
 * Spec Validator
 *
 * Validation logic for command specifications.
 * Validates structure, required fields, and consistency.
 */

import type {
  CommandSpec,
  ParamSpec,
  ResultSpec,
  ValidationResult
} from './specs/CommandSpec';
import { CommandSpecHelper, ParamSpecHelper, ResultSpecHelper } from './specs/CommandSpec';

/**
 * Validator for command specifications
 */
export class SpecValidator {
  /**
   * Validate a complete command spec
   * @param spec The command spec to validate
   * @returns Validation result with errors and warnings
   */
  static validate(spec: CommandSpec): ValidationResult {
    return CommandSpecHelper.validate(spec);
  }

  /**
   * Validate an array of parameters
   * @param params Array of parameter specs
   * @returns Validation result
   */
  static validateParams(params: ParamSpec[]): ValidationResult {
    const errors: string[] = [];
    const warnings: string[] = [];

    if (!params) {
      errors.push('params array is required (use empty array [] if no params)');
      return { valid: false, errors, warnings };
    }

    // Validate each parameter
    params.forEach((param, index) => {
      const paramValidation = ParamSpecHelper.validate(param);
      if (!paramValidation.valid) {
        errors.push(...paramValidation.errors.map(err => `params[${index}]: ${err}`));
      }
      warnings.push(...paramValidation.warnings.map(warn => `params[${index}]: ${warn}`));
    });

    // Check for duplicate parameter names
    const paramNames = params.map(p => p.name);
    const duplicates = paramNames.filter((name, index) => paramNames.indexOf(name) !== index);
    if (duplicates.length > 0) {
      errors.push(`Duplicate parameter names: ${duplicates.join(', ')}`);
    }

    return {
      valid: errors.length === 0,
      errors,
      warnings
    };
  }

  /**
   * Validate an array of result fields
   * @param results Array of result field specs
   * @returns Validation result
   */
  static validateResults(results: ResultSpec[]): ValidationResult {
    const errors: string[] = [];
    const warnings: string[] = [];

    if (!results) {
      errors.push('results array is required (use empty array [] if no results)');
      return { valid: false, errors, warnings };
    }

    // Validate each result field
    results.forEach((result, index) => {
      const resultValidation = ResultSpecHelper.validate(result);
      if (!resultValidation.valid) {
        errors.push(...resultValidation.errors.map(err => `results[${index}]: ${err}`));
      }
      warnings.push(...resultValidation.warnings.map(warn => `results[${index}]: ${warn}`));
    });

    // Check for duplicate result field names
    const resultNames = results.map(r => r.name);
    const duplicates = resultNames.filter((name, index) => resultNames.indexOf(name) !== index);
    if (duplicates.length > 0) {
      errors.push(`Duplicate result field names: ${duplicates.join(', ')}`);
    }

    // Check if 'success' field exists (convention)
    if (!results.some(r => r.name === 'success')) {
      warnings.push('Result fields typically include a "success: boolean" field');
    }

    return {
      valid: errors.length === 0,
      errors,
      warnings
    };
  }

  /**
   * Validate that a spec is complete and ready for generation
   * @param spec The command spec to validate
   * @returns Validation result with detailed feedback
   */
  static validateForGeneration(spec: CommandSpec): ValidationResult {
    const validation = this.validate(spec);

    // Additional checks for generation readiness
    if (validation.valid) {
      // Check if command name is valid (no spaces, follows naming convention)
      if (spec.name.includes(' ')) {
        validation.errors.push('Command name cannot contain spaces');
        validation.valid = false;
      }

      // Check if description is substantial (not just a few characters)
      if (spec.description && spec.description.length < 10) {
        validation.warnings.push('Description seems very short - provide more detail for better documentation');
      }

      // Check if examples are provided
      if (!spec.examples || spec.examples.length === 0) {
        validation.warnings.push('No examples provided - examples help users understand how to use the command');
      }

      // Check if accessLevel is set
      if (!spec.accessLevel) {
        validation.warnings.push('No accessLevel specified - defaulting to "ai-safe"');
      }
    }

    return validation;
  }

  /**
   * Check if validation result has any issues (errors or warnings)
   * @param result Validation result to check
   * @returns True if there are any issues
   */
  static hasIssues(result: ValidationResult): boolean {
    return result.errors.length > 0 || result.warnings.length > 0;
  }

  /**
   * Format validation result as human-readable string
   * @param result Validation result to format
   * @returns Formatted string with errors and warnings
   */
  static formatResult(result: ValidationResult): string {
    const lines: string[] = [];

    if (result.valid) {
      lines.push('✅ Validation passed');
    } else {
      lines.push('❌ Validation failed');
    }

    if (result.errors.length > 0) {
      lines.push('');
      lines.push('Errors:');
      result.errors.forEach(error => {
        lines.push(`  - ${error}`);
      });
    }

    if (result.warnings.length > 0) {
      lines.push('');
      lines.push('Warnings:');
      result.warnings.forEach(warning => {
        lines.push(`  - ${warning}`);
      });
    }

    return lines.join('\n');
  }
}
