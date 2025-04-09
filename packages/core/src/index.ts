/**
 * Core functionality for Human-AI Configuration Protocol
 */

import * as fs from 'fs/promises';
import * as path from 'path';
import * as yaml from 'yaml';
import Ajv from 'ajv';

import { AIConfig, ValidationResult } from './types';

export * from './types';
export * from './utils';

/**
 * Load an AI configuration file from a specified path
 */
export async function loadConfig(configPath: string): Promise<AIConfig> {
  try {
    const content = await fs.readFile(configPath, 'utf-8');
    
    // Check if it's an .md file with YAML frontmatter
    if (configPath.endsWith('.md')) {
      return extractConfigFromMarkdown(content);
    }
    
    // Regular YAML or JSON file
    return yaml.parse(content);
  } catch (error) {
    throw new Error(`Failed to load config from ${configPath}: ${error}`);
  }
}

/**
 * Extract configuration from a markdown file with YAML code block
 */
function extractConfigFromMarkdown(content: string): AIConfig {
  const yamlMatch = content.match(/```ya?ml\n([\s\S]*?)```/);
  
  if (!yamlMatch || !yamlMatch[1]) {
    throw new Error('No valid YAML code block found in markdown file');
  }
  
  try {
    return yaml.parse(yamlMatch[1]);
  } catch (error) {
    throw new Error(`Failed to parse YAML in markdown: ${error}`);
  }
}

// Schema is loaded once and cached
let cachedSchema: any = null;

/**
 * Load the JSON schema for validation
 */
export async function loadSchema(): Promise<any> {
  if (!cachedSchema) {
    try {
      const schemaPath = path.resolve(__dirname, '../../..', 'schema', 'ai-config.schema.json');
      const schemaContent = await fs.readFile(schemaPath, 'utf-8');
      cachedSchema = JSON.parse(schemaContent);
    } catch (error) {
      throw new Error(`Failed to load schema: ${error}`);
    }
  }
  return cachedSchema;
}

/**
 * Validate an AI configuration against the schema
 * Uses cached schema if available for performance
 */
export function validateConfig(config: AIConfig): ValidationResult {
  try {
    // Initialize warnings array to collect all validation warnings
    const warnings: string[] = [];
    let valid = true;
    let errors: string[] | undefined;
    
    // Use schema directly if it's cached, otherwise use basic validation
    if (cachedSchema) {
      // Validate with schema
      const ajv = new Ajv({ allErrors: true });
      const validate = ajv.compile(cachedSchema);
      valid = validate(config);
      
      if (!valid) {
        return {
          valid: false,
          errors: validate.errors?.map(err => `${err.instancePath} ${err.message}`) || []
        };
      }
    } else {
      // Basic validation without schema
      if (!config.ai_protocol_version) {
        return {
          valid: false,
          errors: ['Missing required property: ai_protocol_version']
        };
      }
    }
    
    // Add basic validation warnings regardless of validation method
    if (!config.identity?.role) {
      warnings.push('Missing recommended field: identity.role');
    }
    
    // Additional validation logic beyond schema
    
    // Check for capabilities conflicts
    if (config.capabilities?.allowed && config.capabilities?.restricted) {
      const conflictingCapabilities = config.capabilities.allowed.filter(
        cap => config.capabilities?.restricted?.includes(cap)
      );
      
      if (conflictingCapabilities.length) {
        warnings.push(
          `Capabilities appear in both allowed and restricted: ${conflictingCapabilities.join(', ')}`
        );
      }
    }
    
    return { 
      valid,
      errors,
      warnings: warnings.length ? warnings : undefined
    };
  } catch (error) {
    return {
      valid: false,
      errors: [`Validation error: ${error}`]
    };
  }
}

/**
 * Merge multiple configurations according to protocol rules
 */
export function mergeConfigs(configs: AIConfig[]): AIConfig {
  if (configs.length === 0) {
    throw new Error('No configurations provided for merging');
  }
  
  if (configs.length === 1) {
    return configs[0];
  }
  
  // Start with the most general config
  let result = { ...configs[0] };
  
  // Apply more specific configs in order
  for (let i = 1; i < configs.length; i++) {
    const config = configs[i];
    
    // Version - use the latest version
    result.ai_protocol_version = config.ai_protocol_version || result.ai_protocol_version;
    
    // Identity - more specific overrides general
    if (config.identity) {
      result.identity = { ...result.identity, ...config.identity };
    }
    
    // Behavior - more specific overrides general
    if (config.behavior) {
      result.behavior = { ...result.behavior, ...config.behavior };
    }
    
    // Knowledge - merge additively
    if (config.knowledge) {
      result.knowledge = result.knowledge || {};
      
      // Merge codebase
      if (config.knowledge.codebase) {
        result.knowledge.codebase = { 
          ...result.knowledge.codebase, 
          ...config.knowledge.codebase 
        };
      }
      
      // Merge context
      if (config.knowledge.context) {
        result.knowledge.context = { 
          ...result.knowledge.context, 
          ...config.knowledge.context 
        };
      }
    }
    
    // Capabilities - merge with restrictions taking precedence
    if (config.capabilities) {
      result.capabilities = result.capabilities || {};
      
      // Merge allowed capabilities
      if (config.capabilities.allowed) {
        const allowed = new Set([
          ...(result.capabilities.allowed || []),
          ...config.capabilities.allowed
        ]);
        result.capabilities.allowed = Array.from(allowed);
      }
      
      // Merge restricted capabilities
      if (config.capabilities.restricted) {
        const restricted = new Set([
          ...(result.capabilities.restricted || []),
          ...config.capabilities.restricted
        ]);
        result.capabilities.restricted = Array.from(restricted);
        
        // Remove any allowed capabilities that are now restricted
        if (result.capabilities.allowed) {
          result.capabilities.allowed = result.capabilities.allowed.filter(
            cap => !result.capabilities?.restricted?.includes(cap)
          );
        }
      }
    }
    
    // Permissions - merge with most restrictive winning
    if (config.permissions?.roles) {
      result.permissions = result.permissions || { roles: {} };
      result.permissions.roles = result.permissions.roles || {};
      
      Object.entries(config.permissions.roles).forEach(([role, permissions]) => {
        const existingRole = result.permissions?.roles?.[role];
        
        if (existingRole) {
          result.permissions!.roles![role] = {
            can_modify_config: permissions.can_modify_config !== undefined 
              ? permissions.can_modify_config && existingRole.can_modify_config !== false
              : existingRole.can_modify_config,
            can_instruct_restricted: permissions.can_instruct_restricted !== undefined
              ? permissions.can_instruct_restricted && existingRole.can_instruct_restricted !== false
              : existingRole.can_instruct_restricted
          };
        } else {
          result.permissions!.roles![role] = permissions;
        }
      });
    }
    
    // Extensions - deep merge
    if (config.extensions) {
      result.extensions = result.extensions || {};
      
      Object.entries(config.extensions).forEach(([ext, value]) => {
        const existingExt = result.extensions?.[ext];
        
        if (existingExt && typeof existingExt === 'object' && typeof value === 'object') {
          result.extensions![ext] = { ...existingExt, ...value };
        } else {
          result.extensions![ext] = value;
        }
      });
    }
  }
  
  return result;
}