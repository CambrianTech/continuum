/**
 * Implementation of the validate command
 */

import * as path from 'path';
import * as fs from 'fs/promises';
import chalk from 'chalk';
import { validateConfig, loadConfig } from '@continuum/core';

interface ValidateOptions {
  config: string;
}

export async function validateCommand(options: ValidateOptions): Promise<void> {
  console.log(chalk.blue(`Validating configuration: ${options.config}`));
  
  try {
    // Check if file exists
    const configPath = path.resolve(process.cwd(), options.config);
    
    try {
      await fs.access(configPath);
    } catch (error) {
      console.error(chalk.red(`Configuration file not found: ${configPath}`));
      process.exit(1);
    }
    
    // Load the configuration
    const config = await loadConfig(configPath);
    
    // Validate the configuration
    const result = validateConfig(config);
    
    if (result.valid) {
      console.log(chalk.green('\n✓ Configuration is valid\n'));
      
      // Display warnings if any
      if (result.warnings?.length) {
        console.log(chalk.yellow('Warnings:'));
        result.warnings.forEach(warning => {
          console.log(chalk.yellow(`  - ${warning}`));
        });
        console.log('');
      }
      
      // Display a summary of the configuration
      console.log(chalk.blue('Configuration Summary:'));
      console.log(`Assistant Name: ${config.identity?.name || 'Not specified'}`);
      console.log(`Role: ${config.identity?.role || 'Not specified'}`);
      console.log(`Behavior: ${config.behavior?.voice || 'Not specified'} voice, ${config.behavior?.autonomy || 'Not specified'} autonomy`);
      
      if (config.capabilities?.allowed?.length) {
        console.log(`Allowed Capabilities: ${config.capabilities.allowed.join(', ')}`);
      }
      
      if (config.capabilities?.restricted?.length) {
        console.log(`Restricted Capabilities: ${config.capabilities.restricted.join(', ')}`);
      }
      
    } else {
      console.error(chalk.red('\n✗ Configuration is invalid\n'));
      
      // Display errors
      if (result.errors?.length) {
        console.error(chalk.red('Errors:'));
        result.errors.forEach(error => {
          console.error(chalk.red(`  - ${error}`));
        });
        console.log('');
      }
      
      process.exit(1);
    }
    
  } catch (error) {
    console.error(chalk.red(`Error validating configuration: ${error}`));
    process.exit(1);
  }
}