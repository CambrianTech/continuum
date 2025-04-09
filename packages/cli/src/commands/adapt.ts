/**
 * Implementation of the adapt command
 */

import * as path from 'path';
import * as fs from 'fs/promises';
import chalk from 'chalk';
import { loadConfig, AIConfig } from '@continuum/core';
import { getAdapter } from '../adapters';

interface AdaptOptions {
  for: string;
  config: string;
  output?: string;
}

export async function adaptCommand(options: AdaptOptions): Promise<void> {
  console.log(chalk.blue(`Adapting configuration for ${options.for}...`));
  
  try {
    // Load the configuration
    const configPath = path.resolve(process.cwd(), options.config);
    const config = await loadConfig(configPath);
    
    // Get the adapter
    const adapter = getAdapter(options.for);
    
    if (!adapter) {
      console.error(chalk.red(`No adapter available for platform: ${options.for}`));
      console.log(chalk.yellow('Available adapters: claude, gpt'));
      process.exit(1);
    }
    
    // Format the configuration for the assistant
    const formattedConfig = adapter.formatForAssistant(config);
    
    // Output the result
    if (options.output) {
      const outputPath = path.resolve(process.cwd(), options.output);
      await fs.writeFile(outputPath, formattedConfig, 'utf-8');
      console.log(chalk.green(`\nAdapted configuration saved to ${outputPath}`));
    } else {
      console.log(chalk.yellow('\nAdapted Configuration:'));
      console.log(formattedConfig);
    }
    
  } catch (error) {
    console.error(chalk.red(`Error adapting configuration: ${error}`));
    process.exit(1);
  }
}