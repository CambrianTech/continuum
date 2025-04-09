#!/usr/bin/env node
/**
 * Command Line Interface for Continuum - AI Configuration Protocol
 */

import { program } from 'commander';
import chalk from 'chalk';
import { initCommand } from './commands/init.js';
import { validateCommand } from './commands/validate.js';
import { adaptCommand } from './commands/adapt.js';
import fs from 'fs';
import path from 'path';
import { fileURLToPath } from 'url';

const __filename = fileURLToPath(import.meta.url);
const __dirname = path.dirname(__filename);
const packageJsonPath = path.resolve(__dirname, '../package.json');
const packageJson = JSON.parse(fs.readFileSync(packageJsonPath, 'utf8'));

/**
 * Setup the program CLI
 */
function setupCLI() {
  // Setup the program
  program
    .name('continuum')
    .description('Continuum - DevOps for Cognitive Systems')
    .version(packageJson.version || '0.1.0')
    .addHelpText('after', `
Example usage:
  ${chalk.cyan('$ continuum init')}               Initialize with interactive prompts
  ${chalk.cyan('$ continuum init --template tdd')}  Use the TDD template
  ${chalk.cyan('$ continuum validate')}           Validate current configuration
  ${chalk.cyan('$ continuum adapt --assistant claude')}  Generate Claude-specific config
    `);

  // Init command
  program
    .command('init')
    .description('Initialize a new AI configuration')
    .option('-t, --template <template>', 'Use a predefined template')
    .option('-o, --output <path>', 'Output path for configuration', 'AI_CONFIG.md')
    .action(initCommand);

  // Validate command
  program
    .command('validate')
    .description('Validate an existing configuration')
    .option('-c, --config <path>', 'Path to configuration file', 'AI_CONFIG.md')
    .action(validateCommand);

  // Adapt command
  program
    .command('adapt')
    .description('Generate assistant-specific configuration')
    .requiredOption('-a, --assistant <assistant>', 'Target assistant (claude, gpt)')
    .option('-c, --config <path>', 'Path to configuration file', 'AI_CONFIG.md')
    .option('-o, --output <path>', 'Output path for adapted configuration')
    .action(adaptCommand);

  return program;
}

// Setup and run the CLI
const cli = setupCLI();

// Only parse arguments when this file is run directly (not when imported in tests)
if (import.meta.url === new URL(process.argv[1], 'file://').href) {
  cli.parse(process.argv);
}

export { cli as program };