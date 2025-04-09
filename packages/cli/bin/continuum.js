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
    // We just delegate to the adapted command implementation
    import('../src/commands/adapt.js')
      .then(module => {
        return module.adaptCommand(options);
      })
      .catch(error => {
        console.error(chalk.red(`\nError: ${error.message}`));
        process.exit(1);
      });
  });

// Parse arguments
program.parse(process.argv);