#!/usr/bin/env node

/**
 * Continuum - A contextually aware AI configuration protocol
 * 
 * This CLI is designed to be simple and automatically do the right thing
 * based on the context of your project, user profile, and organization.
 */

import { Command } from 'commander';
import chalk from 'chalk';
import { analyzeContext, executeAction } from '../src/context.js';

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

// Setup CLI with a simpler interface
const program = new Command();
program
  .name('continuum')
  .description('Continuum - Designed by AI and humans for AI and humans')
  .version('0.1.0')
  .option('-a, --assistant <assistant>', 'Target assistant to configure (claude, gpt)')
  .option('-t, --template <template>', 'Template to use for initialization')
  .option('-o, --output <path>', 'Output path for configuration')
  .option('-c, --config <path>', 'Path to configuration file', '.continuum/default/config.md')
  .option('-l, --create-link', 'Create a symlink for tool integration')
  .option('--ask <prompt>', 'Ask AI assistant to perform a task based on your configuration')
  .addHelpText('after', `
Example usage:
  ${chalk.cyan('$ continuum')}                    Detect context and do the right thing
  ${chalk.cyan('$ continuum --template tdd')}     Initialize with TDD template
  ${chalk.cyan('$ continuum --assistant claude')} Generate Claude configuration
  ${chalk.cyan('$ continuum --create-link')}      Create symlinks for tool integration
  ${chalk.cyan('$ continuum --ask "Review this PR"')} Send task to configured assistant
  `);

// Parse arguments
program.parse(process.argv);

// Get options
const options = program.opts();

// Check for specific commands using positional arguments
const args = program.args;
const command = args[0];

// Special case for legacy command format (continuum init, continuum adapt, etc.)
if (command) {
  console.log(chalk.yellow(`Note: The command format 'continuum ${command}' is supported for backward compatibility.`));
  console.log(chalk.yellow(`In the future, you can simply use 'continuum' with appropriate options.`));
  console.log('');
  
  // Map legacy commands to options
  switch (command) {
    case 'init':
      // Handle init command - options are already parsed correctly
      break;
      
    case 'adapt':
      // The adapt command usually requires an assistant option
      if (!options.assistant && args[1]) {
        if (args[1] === '--assistant' || args[1] === '-a') {
          options.assistant = args[2];
        }
      }
      break;
      
    case 'validate':
      // Validation will happen by default
      break;
      
    default:
      console.log(chalk.yellow(`Unknown command: ${command}`));
      // We'll continue with contextual detection
  }
}

async function main() {
  try {
    // Analyze the current context to determine what to do
    const contextResult = await analyzeContext(options);
    
    // Execute the appropriate action based on the context
    await executeAction(contextResult, options);
  } catch (error) {
    console.error(chalk.red(`\nError: ${error.message}`));
    process.exit(1);
  }
}

// Run the main function
main();