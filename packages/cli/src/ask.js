/**
 * Ask functionality - Send task to configured assistant
 * This is a prototype implementation to demonstrate the concept
 */

import fs from 'fs';
import chalk from 'chalk';
import path from 'path';
import { execSync } from 'child_process';

/**
 * Send a task to the configured assistant
 */
export async function askAssistant(prompt, options = {}) {
  console.log(chalk.blue(`\n🧠 Preparing to ask assistant: "${prompt}"\n`));
  
  try {
    // First, determine which assistant to use
    const assistant = options.assistant || detectDefaultAssistant();
    
    if (!assistant) {
      console.log(chalk.yellow('No assistant specified and could not detect a default.'));
      console.log(chalk.yellow('Please use --assistant option or configure a default assistant.'));
      return;
    }
    
    console.log(chalk.dim(`Using ${assistant} assistant...`));
    
    // Check if the assistant configuration exists
    const assistantConfigPath = getAssistantConfigPath(assistant);
    
    if (!fs.existsSync(assistantConfigPath)) {
      console.log(chalk.yellow(`No configuration found for ${assistant} at ${assistantConfigPath}`));
      console.log(chalk.yellow(`Run 'continuum --assistant ${assistant}' to generate one.`));
      return;
    }
    
    // Generate the assistant prompt
    const assistantPrompt = generatePromptForAssistant(assistant, prompt);
    
    // Display what we would do (actual integration would depend on the assistant)
    console.log(chalk.green('\nSending to assistant:'));
    console.log(chalk.dim('-----------------------------------'));
    console.log(assistantPrompt);
    console.log(chalk.dim('-----------------------------------'));
    
    // Check if we can actually execute a command for this assistant
    const command = getAssistantCommand(assistant, prompt);
    
    if (command) {
      console.log(chalk.blue('\nExecuting command:'));
      console.log(chalk.dim(command));
      console.log('');
      
      try {
        // This is a simulation - in a real implementation, we'd handle this more carefully
        // This would need proper escaping and security considerations in a real implementation
        const result = execSync(command, { stdio: 'inherit' });
      } catch (error) {
        console.error(chalk.yellow(`\nCommand execution failed: ${error.message}`));
        console.log(chalk.yellow('This is just a prototype - full integration coming soon!'));
      }
    } else {
      console.log(chalk.yellow('\nThis is a prototype - full integration coming soon!'));
      console.log(chalk.yellow('In a complete implementation, this would launch or send the request to the assistant.'));
    }
    
  } catch (error) {
    console.error(chalk.red(`\nError: ${error.message}`));
  }
}

/**
 * Detect the default assistant based on available configurations
 */
function detectDefaultAssistant() {
  // Check if Claude is configured
  if (fs.existsSync('.continuum/claude/config.md')) {
    return 'claude';
  }
  
  // Check if GPT is configured
  if (fs.existsSync('.continuum/gpt/config.json')) {
    return 'gpt';
  }
  
  // No assistant configured
  return null;
}

/**
 * Get the path to the assistant configuration
 */
function getAssistantConfigPath(assistant) {
  switch (assistant.toLowerCase()) {
    case 'claude':
      return '.continuum/claude/config.md';
    case 'gpt':
      return '.continuum/gpt/config.json';
    default:
      return `.continuum/${assistant.toLowerCase()}/config.md`;
  }
}

/**
 * Generate a prompt for the specified assistant
 */
function generatePromptForAssistant(assistant, prompt) {
  const preamble = `Hello ${assistant}. Load configuration from `;
  
  switch (assistant.toLowerCase()) {
    case 'claude':
      const claudeConfig = fs.existsSync('.continuum/claude/config.md') 
        ? '.continuum/claude/config.md' 
        : (fs.existsSync('CLAUDE.md') ? 'CLAUDE.md' : null);
      
      if (!claudeConfig) {
        return `${preamble}the Continuum configuration. ${prompt}`;
      }
      
      // Get the first line from the config to extract the role
      const claudeContent = fs.readFileSync(claudeConfig, 'utf-8');
      const claudeFirstLine = claudeContent.split('\\n')[0] || '';
      const claudeRole = claudeFirstLine.includes('Configuration for') 
        ? claudeFirstLine.replace('# Continuum Configuration for ', '') 
        : 'Claude';
      
      return `${preamble}${claudeConfig}. Apply the ${claudeRole} role and proceed with this task: ${prompt}`;
      
    case 'gpt':
      const gptConfig = fs.existsSync('.continuum/gpt/config.json') 
        ? '.continuum/gpt/config.json' 
        : (fs.existsSync('GPT.json') ? 'GPT.json' : null);
      
      return `${preamble}${gptConfig || 'the Continuum configuration'}. ${prompt}`;
      
    default:
      return `${preamble}the Continuum configuration. ${prompt}`;
  }
}

/**
 * Get a command to execute for the specified assistant (if available)
 */
function getAssistantCommand(assistant, prompt) {
  switch (assistant.toLowerCase()) {
    case 'claude':
      // Check if Claude Code CLI is installed
      try {
        // We'd need to check properly and escape the prompt in a real implementation
        // This is just a prototype to show the concept
        // Note: claude isn't actually a command that exists, this is just for demonstration
        return `echo "This would run: claude ask \\"${prompt}\\""`;
      } catch (error) {
        return null;
      }
      
    case 'gpt':
      // Check if openai CLI is installed
      try {
        // Again, this is just for demonstration
        return `echo "This would run: openai api chat \\"${prompt}\\""`;
      } catch (error) {
        return null;
      }
      
    default:
      return null;
  }
}