#!/usr/bin/env node

// Simple CLI implementation for Continuum
// Beta version - full features coming soon

import { Command } from 'commander';
import chalk from 'chalk';
import fs from 'fs';
import path from 'path';
// No need for path and fileURLToPath in this simplified version

// Check Node.js version
const currentNodeVersion = process.versions.node;
const semver = currentNodeVersion.split('.');
const major = parseInt(semver[0], 10);

if (major < 18) {
  console.error(
    'You are running Node ' +
      currentNodeVersion +
      '.\n' +
      'The Continuum CLI requires Node 18 or higher. \n' +
      'Please update your version of Node.'
  );
  process.exit(1);
}

// Setup CLI
const program = new Command();
program
  .name('continuum')
  .description('Continuum - Designed by AI and humans for AI and humans')
  .version('0.1.0')
  .addHelpText('after', `
Example usage:
  ${chalk.cyan('$ continuum init')}               Initialize a new configuration
  ${chalk.cyan('$ continuum init --template tdd')}  Use the TDD template
  ${chalk.cyan('$ continuum validate')}           Validate configuration
  ${chalk.cyan('$ continuum adapt --assistant claude')}  Generate Claude config
  ${chalk.cyan('$ continuum adapt --assistant claude --create-link')}  Generate config with symlink at CLAUDE.md
  `);

// Helper function to create a basic config
function createBasicConfig(template = 'standard') {
  return {
    ai_protocol_version: "0.1",
    identity: {
      name: "ProjectAssistant",
      role: "Development collaborator",
      purpose: "Help maintain code quality and guide development"
    },
    behavior: {
      voice: "professional",
      autonomy: "suggest",
      verbosity: "concise",
      risk_tolerance: "medium"
    },
    capabilities: {
      allowed: ["code_review", "refactoring", "documentation", "testing"],
      restricted: ["deployment", "database_management"]
    }
  };
}

// Generate markdown content
function generateMarkdown(config) {
  // Simple YAML stringifier since we can't import yaml in ES modules easily
  function yamlStringify(obj, indent = 0) {
    const spaces = ' '.repeat(indent);
    let result = '';
    
    for (const [key, value] of Object.entries(obj)) {
      if (Array.isArray(value)) {
        result += `${spaces}${key}:\n`;
        for (const item of value) {
          result += `${spaces}  - "${item}"\n`;
        }
      } else if (typeof value === 'object' && value !== null) {
        result += `${spaces}${key}:\n${yamlStringify(value, indent + 2)}`;
      } else {
        result += `${spaces}${key}: "${value}"\n`;
      }
    }
    
    return result;
  }
  
  const yamlStr = yamlStringify(config);
  
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

// Init command
program
  .command('init')
  .description('Initialize a new AI configuration')
  .option('-t, --template <template>', 'Use a predefined template')
  .option('-o, --output <path>', 'Output path for configuration', '.continuum/default/config.md')
  .action((options) => {
    console.log(chalk.blue('\nInitializing new AI configuration...'));
    
    // Show what we'll do
    if (options.template) {
      console.log(`Template: ${chalk.green(options.template)}`);
    } else {
      console.log(`Template: ${chalk.green('standard')}`);
    }
    console.log(`Output: ${chalk.green(options.output)}`);
    
    // Create config
    const config = createBasicConfig(options.template);
    const content = generateMarkdown(config);
    
    try {
      // Create directory if it doesn't exist
      const outputDir = path.dirname(options.output);
      if (outputDir !== '.') {
        try {
          fs.mkdirSync(outputDir, { recursive: true });
          console.log(`Created directory: ${outputDir}`);
        } catch (err) {
          if (err.code !== 'EEXIST') {
            console.error(chalk.red(`\nError creating directory: ${err.message}`));
            return;
          }
        }
      }
      
      fs.writeFileSync(options.output, content);
      console.log(chalk.green(`\nConfiguration written to ${options.output}`));
    } catch (err) {
      console.error(chalk.red(`\nError writing configuration: ${err.message}`));
    }
  });

// Validate command
program
  .command('validate')
  .description('Validate an existing configuration')
  .option('-c, --config <path>', 'Path to configuration file', '.continuum/default/config.md')
  .action((options) => {
    console.log(chalk.blue('\nValidating configuration...'));
    
    try {
      if (!fs.existsSync(options.config)) {
        console.error(chalk.red(`\nError: Configuration file not found at ${options.config}`));
        return;
      }
      
      console.log(chalk.green(`\nConfiguration file exists at ${options.config}`));
      console.log(chalk.yellow('Basic validation successful. Schema validation coming soon.'));
    } catch (err) {
      console.error(chalk.red(`\nError validating configuration: ${err.message}`));
    }
  });

// Adapt command
program
  .command('adapt')
  .description('Generate assistant-specific configuration')
  .requiredOption('-a, --assistant <assistant>', 'Target assistant (claude, gpt)')
  .option('-c, --config <path>', 'Path to configuration file', '.continuum/default/config.md')
  .option('-o, --output <path>', 'Output path for adapted configuration')
  .option('-l, --create-link', 'Create a symlink in the root directory (CLAUDE.md or GPT.json)')
  .action((options) => {
    console.log(chalk.blue(`\nAdapting configuration for ${options.assistant}...`));
    
    try {
      if (!fs.existsSync(options.config)) {
        console.error(chalk.red(`\nError: Configuration file not found at ${options.config}`));
        return;
      }
      
      let outputPath = options.output;
      if (!outputPath) {
        // Default name based on assistant
        const ext = options.assistant === 'claude' ? '.md' : '.json';
        outputPath = `.continuum/${options.assistant}/config${ext}`;
      }
      
      // Read the configuration file
      const content = fs.readFileSync(options.config, 'utf-8');
      
      // Extract YAML content from markdown
      const yamlMatch = content.match(/```yaml\r?\n([\s\S]*?)```/);
      if (!yamlMatch) {
        console.error(chalk.red('\nError: Could not extract YAML configuration from file'));
        return;
      }
      
      const yamlContent = yamlMatch[1];
      let config;
      try {
        // Basic YAML parser since we don't have a full yaml library 
        const lines = yamlContent.trim().split('\n');
        config = {};
        
        let currentSection = null;
        let currentSubsection = null;
        
        for (const line of lines) {
          const trimmedLine = line.trim();
          
          // Skip empty lines and comments
          if (!trimmedLine || trimmedLine.startsWith('#')) continue;
          
          if (!trimmedLine.startsWith(' ') && !trimmedLine.startsWith('-') && trimmedLine.includes(':')) {
            // Top-level key
            const [key, value] = trimmedLine.split(':', 2);
            currentSection = key.trim();
            currentSubsection = null;
            
            if (value && value.trim()) {
              config[currentSection] = value.trim().replace(/"/g, '');
            } else {
              config[currentSection] = {};
            }
          } else if (trimmedLine.startsWith('  ') && !trimmedLine.startsWith('    ') && trimmedLine.includes(':')) {
            // Second-level key
            const [key, value] = trimmedLine.split(':', 2);
            currentSubsection = key.trim();
            
            if (!config[currentSection]) {
              config[currentSection] = {};
            }
            
            if (value && value.trim()) {
              config[currentSection][currentSubsection] = value.trim().replace(/"/g, '');
            } else {
              config[currentSection][currentSubsection] = {};
            }
          } else if (trimmedLine.startsWith('    ') && trimmedLine.includes(':')) {
            // Third-level key
            const [key, value] = trimmedLine.split(':', 2);
            const subKey = key.trim();
            
            if (!config[currentSection][currentSubsection]) {
              config[currentSection][currentSubsection] = {};
            }
            
            config[currentSection][currentSubsection][subKey] = value.trim().replace(/"/g, '');
          } else if (trimmedLine.startsWith('  - ')) {
            // List item at second level
            const value = trimmedLine.substring(4).replace(/"/g, '');
            
            if (!config[currentSection]) {
              config[currentSection] = [];
            }
            
            if (!Array.isArray(config[currentSection])) {
              config[currentSection] = [];
            }
            
            config[currentSection].push(value);
          } else if (trimmedLine.startsWith('    - ')) {
            // List item at third level
            const value = trimmedLine.substring(6).replace(/"/g, '');
            
            if (!config[currentSection][currentSubsection]) {
              config[currentSection][currentSubsection] = [];
            }
            
            if (!Array.isArray(config[currentSection][currentSubsection])) {
              config[currentSection][currentSubsection] = [];
            }
            
            config[currentSection][currentSubsection].push(value);
          }
        }
      } catch (err) {
        console.error(chalk.red(`\nError parsing YAML: ${err.message}`));
        // Fall back to simple approach
        config = { content: yamlContent };
      }
      
      // Extract markdown instructions
      const instructions = content.split('```yaml')[0].trim() + '\n\n' + 
                          content.split('```')[2].trim();
      
      let adaptedContent;
      if (options.assistant === 'claude') {
        // For Claude: Create a custom system prompt format
        adaptedContent = `# Continuum Configuration for Claude

## Role and Goal
You are ${config.identity?.name || 'ContinuumAssistant'}, a ${config.identity?.role || 'development collaborator'}. Your purpose is to ${config.identity?.purpose || 'assist with development tasks'}.

## Constraints
${config.identity?.limitations ? config.identity.limitations.map(l => `- ${l}`).join('\n') : '- Follow the project\'s best practices'}
- Maintain a ${config.behavior?.voice || 'professional'} tone
- Exercise ${config.behavior?.risk_tolerance || 'moderate'} risk tolerance

## Guidelines
${config.knowledge?.codebase ? `You are working with a ${config.knowledge.codebase.structure} using ${config.knowledge.codebase.conventions}.` : ''}
${config.knowledge?.context ? Object.entries(config.knowledge.context).map(([key, value]) => `- **${key}**: ${value}`).join('\n') : ''}

### Permitted Capabilities
${config.capabilities?.allowed ? config.capabilities.allowed.map(c => `- ${c.replace(/_/g, ' ')}`).join('\n') : '- Code assistance'}

### Restricted Capabilities
${config.capabilities?.restricted ? config.capabilities.restricted.map(c => `- ${c.replace(/_/g, ' ')}`).join('\n') : '- None specified'}

${instructions.split('##').slice(1).join('##')}`;
      } else if (options.assistant === 'gpt') {
        // For GPT: Create a JSON format for API calls
        const systemContent = `# System Instructions for ${config.identity?.name || 'Continuum Project'}

You are ${config.identity?.name || 'ContinuumAssistant'}, a ${config.identity?.role || 'development collaborator'}. Your purpose is to ${config.identity?.purpose || 'assist with development tasks'}.

## Configuration Parameters

- **Identity**: ${config.identity?.name || 'ContinuumAssistant'} (${config.identity?.role || 'Assistant'})
- **Voice**: ${config.behavior?.voice || 'Professional'}
- **Autonomy Level**: ${config.behavior?.autonomy || 'Suggest (not dictate)'}
- **Verbosity**: ${config.behavior?.verbosity || 'Concise'}
- **Risk Tolerance**: ${config.behavior?.risk_tolerance || 'Medium'}

## Technical Context

${config.knowledge ? Object.entries(config.knowledge).flatMap(([section, details]) => {
  if (typeof details === 'object') {
    return Object.entries(details).map(([key, value]) => `- **${key}**: ${value}`);
  }
  return [`- **${section}**: ${details}`];
}).join('\n') : '- No specific technical context provided'}

## Allowed Capabilities

${config.capabilities?.allowed ? config.capabilities.allowed.map(c => `- ${c.replace(/_/g, ' ')}`).join('\n') : '- General assistance'}

## Restricted Capabilities

${config.capabilities?.restricted ? config.capabilities.restricted.map(c => `- ${c.replace(/_/g, ' ')}`).join('\n') : '- None specified'}

${instructions.split('##').slice(1).join('##')}`;

        adaptedContent = JSON.stringify({
          model: "gpt-4o",
          messages: [
            {
              role: "system",
              content: systemContent
            }
          ],
          temperature: 0.7
        }, null, 2);
      } else {
        // Default approach for unknown assistants
        adaptedContent = `# Adapted for ${options.assistant.toUpperCase()}\n\n${content}`;
      }
      
      // Create directory if it doesn't exist
      const outputDir = path.dirname(outputPath);
      if (outputDir !== '.') {
        try {
          fs.mkdirSync(outputDir, { recursive: true });
          console.log(`Created directory: ${outputDir}`);
        } catch (err) {
          if (err.code !== 'EEXIST') {
            console.error(chalk.red(`\nError creating directory: ${err.message}`));
            return;
          }
        }
      }
      
      fs.writeFileSync(outputPath, adaptedContent);
      
      console.log(chalk.green(`\nAdapted configuration written to ${outputPath}`));
    } catch (err) {
      console.error(chalk.red(`\nError adapting configuration: ${err.message}`));
    }
  });

// Parse arguments
program.parse(process.argv);