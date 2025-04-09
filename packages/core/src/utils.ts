/**
 * Utility functions for Human-AI Configuration Protocol
 */

import * as fs from 'fs/promises';
import * as path from 'path';
import * as yaml from 'yaml';
import { AIConfig, ConfigLocations } from './types';

/**
 * Find configuration files in the project structure
 */
export async function findConfigFiles(startPath: string): Promise<string[]> {
  const configFiles: string[] = [];
  
  // Check for personal config in home directory
  const homeDir = process.env.HOME || process.env.USERPROFILE;
  if (homeDir) {
    const personalConfigPath = path.join(homeDir, ConfigLocations.PERSONAL);
    try {
      await fs.access(personalConfigPath);
      configFiles.push(personalConfigPath);
    } catch (error) {
      // Personal config doesn't exist, that's okay
    }
  }
  
  // Walk up the directory tree to find project and organization configs
  let currentDir = startPath;
  let lastDir = '';
  
  while (currentDir !== lastDir) {
    // Check for project config
    const projectConfigPath = path.join(currentDir, ConfigLocations.PROJECT);
    try {
      await fs.access(projectConfigPath);
      configFiles.push(projectConfigPath);
    } catch (error) {
      // Project config doesn't exist at this level, that's okay
    }
    
    // Check for org config
    const orgConfigPath = path.join(currentDir, ConfigLocations.ORGANIZATION);
    try {
      await fs.access(orgConfigPath);
      configFiles.push(orgConfigPath);
    } catch (error) {
      // Org config doesn't exist at this level, that's okay
    }
    
    // Move up a directory
    lastDir = currentDir;
    currentDir = path.dirname(currentDir);
  }
  
  return configFiles;
}

/**
 * Generate a markdown file from a configuration object
 */
export function generateMarkdown(config: AIConfig): string {
  const yamlStr = yaml.stringify(config);
  
  return `# Continuum Configuration

\`\`\`yaml
${yamlStr}
\`\`\`

## Additional Instructions

This configuration was generated using Continuum - designed by AI and humans for AI and humans.

### Project Context

This defines how AI assistants should interact with this project.

### Workflow Examples

AI assistants should follow the configuration above when working with this project.
`;
}

/**
 * Generate a continuum.md file at the specified location
 */
export async function writeConfigFile(config: AIConfig, outputPath: string): Promise<void> {
  const content = generateMarkdown(config);
  await fs.writeFile(outputPath, content, 'utf-8');
}