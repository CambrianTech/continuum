/**
 * Implementation of the validate command
 */

import * as path from 'path';
import chalk from 'chalk';
import { loadConfig, validateConfig } from '@continuum/core';

interface ValidateOptions {
  config: string;
}

export async function validateCommand(options: ValidateOptions): Promise<void> {
  console.log(chalk.blue(`Validating configuration at ${options.config}...`));
  
  try {
    // Load the configuration
    const configPath = path.resolve(process.cwd(), options.config);
    const config = await loadConfig(configPath);
    
    // Validate the configuration
    const result = await validateConfig(config);
    
    if (result.valid) {
      console.log(chalk.green('\n✓ Configuration is valid'));
      
      // Print summary
      console.log(chalk.yellow('\nConfiguration Summary:'));
      console.log(`AI Protocol Version: ${config.ai_protocol_version}`);
      console.log(`Assistant Name: ${config.identity?.name || 'Not specified'}`);
      console.log(`Role: ${config.identity?.role || 'Not specified'}`);
      console.log(`Voice: ${config.behavior?.voice || 'Not specified'}`);
      console.log(`Autonomy: ${config.behavior?.autonomy || 'Not specified'}`);
      
      if (config.capabilities?.allowed?.length) {
        console.log(`Allowed Capabilities: ${config.capabilities.allowed.join(', ')}`);
      }
      
      if (config.capabilities?.restricted?.length) {
        console.log(`Restricted Capabilities: ${config.capabilities.restricted.join(', ')}`);
      }
      
    } else {
      console.log(chalk.red('\n✗ Configuration is invalid'));
      
      if (result.errors?.length) {
        console.log(chalk.yellow('\nErrors:'));
        result.errors.forEach(error => {
          console.log(`- ${error}`);
        });
      }
      
      if (result.warnings?.length) {
        console.log(chalk.yellow('\nWarnings:'));
        result.warnings.forEach(warning => {
          console.log(`- ${warning}`);
        });
      }
      
      process.exit(1);
    }
    
  } catch (error) {
    console.error(chalk.red(`Error validating configuration: ${error}`));
    process.exit(1);
  }
}