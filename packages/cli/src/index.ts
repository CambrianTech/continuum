#!/usr/bin/env node
/**
 * Command Line Interface for Human-AI Configuration Protocol
 */

import { program } from 'commander';
import inquirer from 'inquirer';
import * as path from 'path';
import * as fs from 'fs/promises';
import { AIConfig, loadConfig, validateConfig, writeConfigFile } from '@continuum/core';
import { initCommand } from './commands/init';
import { validateCommand } from './commands/validate';
import { adaptCommand } from './commands/adapt';

// Setup the program
program
  .name('ai-config')
  .description('Human-AI Configuration Protocol CLI')
  .version('0.1.0');

// Init command
program
  .command('init')
  .description('Initialize a new AI configuration')
  .option('-t, --template <template>', 'Use a predefined template', 'standard')
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
  .description('Generate platform-specific configuration')
  .requiredOption('--for <platform>', 'Target platform (claude, gpt)')
  .option('-c, --config <path>', 'Path to configuration file', 'AI_CONFIG.md')
  .option('-o, --output <path>', 'Output path for adapted configuration')
  .action(adaptCommand);

// Parse and execute
program.parse(process.argv);

export { program };