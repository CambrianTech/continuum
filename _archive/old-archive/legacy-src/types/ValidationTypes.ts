/**
 * Validation Types - Shared validation interfaces across all Continuum modules
 */

export interface ValidationResult {
  success: boolean;
  error?: string;
  details?: Record<string, any>;
}

export interface ValidationRule {
  name: string;
  validate: (value: any) => ValidationResult;
  message?: string;
}

export interface FieldValidation {
  required?: boolean;
  type?: 'string' | 'number' | 'boolean' | 'object' | 'array';
  rules?: ValidationRule[];
  defaultValue?: any;
}

export interface ValidationSchema {
  fields: Record<string, FieldValidation>;
  strict?: boolean; // If true, reject unknown fields
}

export interface ValidationContext {
  path: string[];
  value: any;
  schema: ValidationSchema;
  root: any;
}

export class ValidationError extends Error {
  constructor(
    message: string,
    public field: string,
    public value: any,
    public rule?: string
  ) {
    super(message);
    this.name = 'ValidationError';
  }
}