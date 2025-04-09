/**
 * Implementation of the adapt command
 * Transforms configuration to assistant-specific format
 */

import * as path from 'path';
import * as fs from 'fs/promises';
import chalk from 'chalk';
import { loadConfig } from '@continuum/core';
import { getAdapter } from '../adapters/index.js';

interface AdaptOptions {
  assistant: string;
  config: string;
  output?: string;
}

export async function adaptCommand(options: AdaptOptions): Promise<void> {
  console.log(chalk.blue(`Adapting configuration for ${options.assistant}...`));
  
  try {
    // Check if file exists
    const configPath = path.resolve(process.cwd(), options.config);
    
    try {
      await fs.access(configPath);
    } catch (_) {
      console.error(chalk.red(`Configuration file not found: ${configPath}`));
      process.exit(1);
    }
    
    // Get the appropriate adapter
    const adapter = getAdapter(options.assistant);
    
    if (!adapter) {
      console.error(chalk.red(`No adapter found for assistant: ${options.assistant}`));
      console.error(chalk.yellow('Available adapters: claude, gpt'));
      process.exit(1);
    }
    
    // Load the configuration
    const config = await loadConfig(configPath);
    
    // Format for the specific assistant
    const formattedConfig = adapter.formatForAssistant(config);
    
    // Determine output path
    let outputPath: string;
    if (options.output) {
      outputPath = path.resolve(process.cwd(), options.output);
    } else {
      // Default output path based on assistant type
      const defaultOutputName = getDefaultOutputName(options.assistant);
      outputPath = path.resolve(path.dirname(configPath), defaultOutputName);
    }
    
    // Write the formatted configuration
    await fs.writeFile(outputPath, formattedConfig, 'utf-8');
    
    console.log(chalk.green(`\nAdapted configuration for ${options.assistant} created at:`));
    console.log(outputPath);
    
    // Show sample usage
    console.log(chalk.yellow('\nUsage:'));
    if (options.assistant === 'claude') {
      console.log('- Copy this into your Claude conversation as a system prompt');
      console.log('- Or reference it in your repo for Claude Code');
    } else if (options.assistant === 'gpt') {
      console.log('- Use this as your system prompt for ChatGPT');
      console.log('- Or include in your OpenAI API calls');
    }
    
  } catch (error) {
    console.error(chalk.red(`Error adapting configuration: ${error}`));
    process.exit(1);
  }
}

/**
 * Get default output filename based on assistant type
 */
function getDefaultOutputName(assistant: string): string {
  switch (assistant.toLowerCase()) {
    case 'claude':
      return 'CLAUDE.md';
    case 'gpt':
      return 'GPT_SYSTEM_PROMPT.txt';
    default:
      return `${assistant.toUpperCase()}_CONFIG.txt`;
  }
}