/**
 * Templates for AI configurations
 */

import * as path from 'path';
import * as fs from 'fs/promises';
import { AIConfig } from '@continuum/core';

// Default templates as fallbacks
const defaultTemplates: Record<string, AIConfig> = {
  minimal: {
    ai_protocol_version: '0.1',
    identity: {
      role: 'Assistant'
    }
  },
  
  standard: {
    ai_protocol_version: '0.1',
    identity: {
      name: 'ProjectAssistant',
      role: 'Development collaborator',
      purpose: 'Help maintain code quality and guide development',
      limitations: ['No direct deployment', 'No customer data access']
    },
    behavior: {
      voice: 'professional',
      autonomy: 'suggest',
      verbosity: 'concise',
      risk_tolerance: 'low'
    },
    capabilities: {
      allowed: [
        'code_review',
        'refactoring',
        'documentation',
        'testing'
      ],
      restricted: [
        'deployment',
        'customer_data_access'
      ]
    }
  }
};

// Template cache
let templatesCache: Record<string, AIConfig> | null = null;

/**
 * Load templates from the templates directory
 */
async function loadTemplates(): Promise<Record<string, AIConfig>> {
  if (templatesCache) {
    return templatesCache;
  }

  const templates = { ...defaultTemplates };
  const templatesDir = path.resolve(__dirname, '../../../templates');
  
  try {
    // Get all subdirectories in the templates directory
    const dirs = await fs.readdir(templatesDir, { withFileTypes: true });
    const templateDirs = dirs.filter(dir => dir.isDirectory());

    // Load each template
    for (const dir of templateDirs) {
      const templateName = dir.name;
      const configPath = path.join(templatesDir, templateName, 'config.json');
      
      try {
        const configData = await fs.readFile(configPath, 'utf-8');
        templates[templateName] = JSON.parse(configData);
      } catch (error) {
        console.warn(`Failed to load template ${templateName}: ${error}`);
      }
    }

    templatesCache = templates;
    return templates;
  } catch (error) {
    console.warn(`Failed to load templates: ${error}`);
    return defaultTemplates;
  }
}

/**
 * Get a list of available template names
 */
export async function listTemplates(): Promise<string[]> {
  const templates = await loadTemplates();
  return Object.keys(templates);
}

/**
 * Get a configuration template by name
 */
export async function getTemplate(templateName: string): Promise<AIConfig> {
  const templates = await loadTemplates();
  const template = templates[templateName];
  
  if (!template) {
    const availableTemplates = Object.keys(templates).join(', ');
    throw new Error(`Template "${templateName}" not found. Available templates: ${availableTemplates}`);
  }
  
  return JSON.parse(JSON.stringify(template)); // Deep clone
}