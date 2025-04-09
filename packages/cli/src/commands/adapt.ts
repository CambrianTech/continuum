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
  createLink?: boolean;
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
    
    // Create symlink if requested
    if (options.createLink) {
      try {
        const linkName = getLinkName(options.assistant);
        const linkPath = path.resolve(process.cwd(), linkName);
        
        // Remove existing symlink if it exists
        try {
          await fs.unlink(linkPath);
        } catch (e: any) {
          // Only ignore ENOENT (file doesn't exist), rethrow other errors
          if (e.code !== 'ENOENT') {
            console.error(chalk.red(`Error removing existing symlink: ${e.message}`));
            throw e;
          }
        }
        
        // Create relative path for the symlink
        const relativePath = path.relative(process.cwd(), outputPath);
        
        // Create symlink
        await fs.symlink(relativePath, linkPath);
        console.log(chalk.green(`\nSymlink created at:`));
        console.log(linkPath);
      } catch (err: any) {
        console.error(chalk.yellow(`\nCould not create symlink: ${err.message}`));
        console.error(chalk.yellow(`You may need to run with administrative privileges or use a different method to create links.`));
      }
    }
    
    // Show sample usage
    console.log(chalk.yellow('\nUsage:'));
    if (options.assistant === 'claude') {
      console.log('- Copy this into your Claude conversation as a system prompt');
      console.log('- Or reference it in your repo for Claude Code');
      if (options.createLink) {
        console.log('- For Claude Code, add this to .clauderc: "systemPromptFile": "CLAUDE.md"');
      }
    } else if (options.assistant === 'gpt') {
      console.log('- Use this as your system prompt for ChatGPT');
      console.log('- Or include in your OpenAI API calls');
      if (options.createLink) {
        console.log('- For API calls, you can include this with: "system_prompt_file": "GPT.json"');
      }
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
      return path.join('..', 'claude', 'config.md');
    case 'gpt':
      return path.join('..', 'gpt', 'config.json');
    default:
      return path.join('..', assistant.toLowerCase(), 'config.txt');
  }
}

/**
 * Get the symlink name based on assistant type
 */
function getLinkName(assistant: string): string {
  switch (assistant.toLowerCase()) {
    case 'claude':
      return 'CLAUDE.md';
    case 'gpt':
      return 'GPT.json';
    default:
      return `${assistant.toUpperCase()}.txt`;
  }
}