/**
 * Types for Human-AI Configuration Protocol
 */

export interface AIConfig {
  ai_protocol_version: string;
  identity?: Identity;
  behavior?: Behavior;
  knowledge?: Knowledge;
  capabilities?: Capabilities;
  permissions?: Permissions;
  extensions?: Record<string, unknown>;
}

export interface Identity {
  name?: string;
  role: string;
  purpose?: string;
  limitations?: string[];
}

export type VoiceStyle = 'professional' | 'friendly' | 'academic' | 'casual' | 'technical' | 'custom';
export type AutonomyLevel = 'suggest' | 'execute_with_approval' | 'fully_autonomous' | 'restricted';
export type VerbosityLevel = 'concise' | 'detailed' | 'comprehensive';
export type RiskTolerance = 'low' | 'medium' | 'high';

export interface Behavior {
  voice?: VoiceStyle;
  autonomy?: AutonomyLevel;
  verbosity?: VerbosityLevel;
  risk_tolerance?: RiskTolerance;
}

export interface Knowledge {
  codebase?: {
    structure?: string;
    conventions?: string;
  };
  context?: Record<string, string>;
}

export interface Capabilities {
  allowed?: string[];
  restricted?: string[];
}

export interface RolePermissions {
  can_modify_config?: boolean;
  can_instruct_restricted?: boolean;
}

export interface Permissions {
  roles?: Record<string, RolePermissions>;
}

export interface ConfigAdapter {
  name: string;
  loadConfig(path: string): Promise<AIConfig>;
  mergeConfigs(configs: AIConfig[]): AIConfig;
  formatForAssistant(config: AIConfig): string;
}

export interface ConfigValidator {
  validateConfig(config: AIConfig): ValidationResult;
}

export interface ValidationResult {
  valid: boolean;
  errors?: string[];
  warnings?: string[];
}

export interface TemplateOptions {
  name?: string;
  role?: string;
  level?: 'minimal' | 'standard' | 'comprehensive';
}

export enum ConfigLocations {
  PERSONAL = '.continuum/default/config.md',
  PROJECT = '.continuum/project/config.md',
  ORGANIZATION = '.continuum/org/config.md'
}