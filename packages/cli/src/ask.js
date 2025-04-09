/**
 * Ask functionality - Send task to configured assistant
 * Enhanced with context awareness for intelligent assistant interactions
 */

import fs from 'fs';
import chalk from 'chalk';
import path from 'path';
import { exec } from 'child_process';
import { promisify } from 'util';

// Helper for async execution
const execAsync = promisify(exec);

/**
 * Send a task to the configured assistant with context awareness
 */
export async function askAssistant(prompt, options = {}) {
  console.log(chalk.blue(`\n🧠 Preparing to ask assistant: "${prompt}"\n`));
  
  try {
    // Gather context information
    const contextInfo = await gatherContextInfo();
    
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
    
    // Generate the assistant prompt with context awareness
    const assistantPrompt = generatePromptForAssistant(assistant, prompt, contextInfo);
    
    // Display what we would do (actual integration would depend on the assistant)
    console.log(chalk.green('\nSending to assistant:'));
    console.log(chalk.dim('-----------------------------------'));
    console.log(assistantPrompt);
    console.log(chalk.dim('-----------------------------------'));
    
    // Check if we can actually execute a command for this assistant
    const command = getAssistantCommand(assistant, prompt, contextInfo);
    
    if (command) {
      console.log(chalk.blue('\nExecuting command:'));
      console.log(chalk.dim(command));
      console.log('');
      
      try {
        // This is a simulation - in a real implementation, we'd handle this more carefully
        // This would need proper escaping and security considerations in a real implementation
        await execAsync(command);
        console.log(chalk.green('\nCommand executed successfully.'));
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
 * Gather context information about the current environment
 */
async function gatherContextInfo() {
  const contextInfo = {
    projectInfo: {},
    repoInfo: {},
    configInfo: {}
  };
  
  try {
    // Check for package.json
    if (fs.existsSync('package.json')) {
      try {
        const packageJson = JSON.parse(fs.readFileSync('package.json', 'utf-8'));
        contextInfo.projectInfo = {
          name: packageJson.name,
          version: packageJson.version,
          description: packageJson.description,
          type: 'Node.js/JavaScript'
        };
      } catch (e) {
        // Ignore parsing errors
      }
    }
    
    // Check for Git repo
    if (fs.existsSync('.git')) {
      try {
        // Get branch info
        const headContent = fs.readFileSync('.git/HEAD', 'utf-8');
        const branchMatch = headContent.match(/ref: refs\/heads\/([^\n]+)/);
        const branch = branchMatch ? branchMatch[1] : 'unknown';
        
        // Get status info (simplified)
        const { stdout: statusOutput } = await execAsync('git status --porcelain', { timeout: 1000 });
        const hasChanges = statusOutput.trim().length > 0;
        
        contextInfo.repoInfo = {
          type: 'git',
          branch,
          hasChanges
        };
      } catch (e) {
        // Fallback to minimal info
        contextInfo.repoInfo = { type: 'git' };
      }
    }
    
    // Check for Continuum configurations
    contextInfo.configInfo = {
      hasProjectConfig: fs.existsSync('.continuum/default/config.md'),
      hasClaudeConfig: fs.existsSync('.continuum/claude/config.md'),
      hasGptConfig: fs.existsSync('.continuum/gpt/config.json'),
      hasClaudeSymlink: fs.existsSync('CLAUDE.md'),
      hasGptSymlink: fs.existsSync('GPT.json'),
      hasClaudeRC: fs.existsSync('.clauderc')
    };
    
  } catch (error) {
    // Continue with partial context info
    console.log(chalk.dim('Note: Could not gather full context information'));
  }
  
  return contextInfo;
}

/**
 * Detect the default assistant based on available configurations
 */
function detectDefaultAssistant() {
  // Check if Claude is configured and has symlink
  if (fs.existsSync('CLAUDE.md') || fs.existsSync('.continuum/claude/config.md')) {
    return 'claude';
  }
  
  // Check if GPT is configured and has symlink
  if (fs.existsSync('GPT.json') || fs.existsSync('.continuum/gpt/config.json')) {
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
      // Prefer symlink if it exists
      return fs.existsSync('CLAUDE.md') 
        ? 'CLAUDE.md' 
        : '.continuum/claude/config.md';
    case 'gpt':
      // Prefer symlink if it exists
      return fs.existsSync('GPT.json') 
        ? 'GPT.json' 
        : '.continuum/gpt/config.json';
    default:
      // Check for specific assistant symlink
      const customSymlink = `${assistant.toUpperCase()}.md`;
      if (fs.existsSync(customSymlink)) {
        return customSymlink;
      }
      return `.continuum/${assistant.toLowerCase()}/config.md`;
  }
}

/**
 * Generate a prompt for the specified assistant with context awareness
 */
function generatePromptForAssistant(assistant, prompt, contextInfo = {}) {
  const preamble = `Hello ${assistant}. Load configuration from `;
  
  // Common context information for any assistant
  let contextStr = '';
  const { projectInfo, repoInfo } = contextInfo;
  
  if (projectInfo.name) {
    contextStr += `\nProject Context: ${projectInfo.name}`;
    if (projectInfo.description) contextStr += ` - ${projectInfo.description}`;
    if (projectInfo.type) contextStr += ` (${projectInfo.type})`;
  }
  
  if (repoInfo.type) {
    contextStr += `\nRepository: ${repoInfo.type}`;
    if (repoInfo.branch) contextStr += `, branch: ${repoInfo.branch}`;
    if (repoInfo.hasChanges) contextStr += ` (has uncommitted changes)`;
  }
  
  switch (assistant.toLowerCase()) {
    case 'claude':
      const claudeConfig = getAssistantConfigPath('claude');
      
      if (!fs.existsSync(claudeConfig)) {
        return `${preamble}the Continuum configuration.${contextStr}\n\nTask: ${prompt}`;
      }
      
      // Get the first line from the config to extract the role
      const claudeContent = fs.readFileSync(claudeConfig, 'utf-8');
      const claudeFirstLine = claudeContent.split('\n')[0] || '';
      const claudeRole = claudeFirstLine.includes('Configuration for') 
        ? claudeFirstLine.replace('# Continuum Configuration for ', '') 
        : 'Claude';
      
      return `${preamble}${claudeConfig}. Apply the ${claudeRole} role.${contextStr}\n\nTask: ${prompt}`;
      
    case 'gpt':
      const gptConfig = getAssistantConfigPath('gpt');
      
      return `${preamble}${gptConfig}.${contextStr}\n\nTask: ${prompt}`;
      
    default:
      return `${preamble}the Continuum configuration.${contextStr}\n\nTask: ${prompt}`;
  }
}

/**
 * Get a command to execute for the specified assistant (if available)
 * Enhanced with context awareness
 */
function getAssistantCommand(assistant, prompt, contextInfo = {}) {
  // Escape the prompt for shell
  const escapedPrompt = prompt.replace(/"/g, '\\"');
  
  switch (assistant.toLowerCase()) {
    case 'claude':
      // Check for Claude Code CLI 
      const claudeConfigPath = getAssistantConfigPath('claude');
      const hasClaudeRC = contextInfo?.configInfo?.hasClaudeRC || fs.existsSync('.clauderc');
      
      // Use real Claude command if we know Claude CLI exists
      // This is a demo command, real implementation would vary
      if (hasClaudeRC) {
        return `echo "This would run: claude ask \\"${escapedPrompt}\\""`;
      } else {
        return `echo "This would run: claude --system-prompt ${claudeConfigPath} ask \\"${escapedPrompt}\\""`;
      }
      
    case 'gpt':
      // Check for GPT CLI
      const gptConfigPath = getAssistantConfigPath('gpt');
      
      // Use real GPT command if we know OpenAI CLI exists
      // This is a demo command, real implementation would vary
      return `echo "This would run: openai api chat --config ${gptConfigPath} \\"${escapedPrompt}\\""`;
      
    default:
      return `echo "No command configured for assistant: ${assistant}"`;
  }
}