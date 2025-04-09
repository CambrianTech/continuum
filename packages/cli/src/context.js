/**
 * Context detection for intelligent command execution
 * This module determines what action to take based on the current environment
 */

import * as fsPromises from 'fs/promises';
import fs, { existsSync } from 'fs';
import path from 'path';
import chalk from 'chalk';
// We'll import these dynamically to avoid path issues
// import { adaptCommand } from './commands/adapt.js';
// import { askAssistant } from './ask.js';

// Default paths
const USER_CONFIG_PATH = path.resolve(process.env.HOME || process.env.USERPROFILE, '.continuum/profile.yml');
const PROJECT_CONFIG_PATH = '.continuum/default/config.md';
const ORG_CONFIG_PATH = '.continuum/org/config.md';

/**
 * Analyzes the current context to determine what action to take
 */
export async function analyzeContext(options = {}) {
  console.log(chalk.blue('🧠 Analyzing context...\n'));
  
  // Check if we're in a git repository
  const isGitRepo = await checkIsGitRepo();
  
  // Check if config files exist
  const hasProjectConfig = existsSync(PROJECT_CONFIG_PATH);
  const hasUserConfig = existsSync(USER_CONFIG_PATH);
  const hasOrgConfig = existsSync(ORG_CONFIG_PATH);
  
  // Check if AI assistant integration files exist
  const hasClaudeConfig = existsSync('.continuum/claude/config.md');
  const hasGptConfig = existsSync('.continuum/gpt/config.json');
  const hasClaudeSymlink = existsSync('CLAUDE.md');
  const hasGptSymlink = existsSync('GPT.json');
  
  // Check if .clauderc exists
  const hasClaudeRC = existsSync('.clauderc');
  
  // Check if specific arguments were passed
  const hasAssistantArg = options.assistant !== undefined;
  const hasTemplateArg = options.template !== undefined;
  const hasAskPrompt = options.ask !== undefined;
  
  // Log the detected context
  console.log(chalk.dim('Context detected:'));
  console.log(chalk.dim(`- Git repository: ${isGitRepo ? '✅' : '❌'}`));
  console.log(chalk.dim(`- Project config: ${hasProjectConfig ? '✅' : '❌'}`));
  console.log(chalk.dim(`- User config: ${hasUserConfig ? '✅' : '❌'}`));
  console.log(chalk.dim(`- Organization config: ${hasOrgConfig ? '✅' : '❌'}`));
  console.log(chalk.dim(`- Claude integration: ${hasClaudeConfig ? '✅' : '❌'}`));
  console.log(chalk.dim(`- GPT integration: ${hasGptConfig ? '✅' : '❌'}`));
  console.log('');
  
  // Determine what to do based on context
  if (hasAskPrompt) {
    // Ask prompt was provided, send to assistant
    return { action: 'ask', message: `Processing request: "${options.ask}"` };
  }
  
  if (!hasProjectConfig) {
    // No project config found, we should initialize
    return { action: 'init', message: 'No configuration found. Running initialization...' };
  }
  
  if (hasAssistantArg) {
    // Specific assistant argument was provided, run adapt command
    return { action: 'adapt', message: `Adapting configuration for ${options.assistant}...` };
  }
  
  if (hasProjectConfig && !hasClaudeConfig && !hasGptConfig) {
    // Project config exists but no assistant configs, suggest adapting
    return { 
      action: 'suggest_adapt', 
      message: 'Configuration exists but no assistant integrations found. Would you like to generate them?' 
    };
  }
  
  if (hasClaudeConfig && !hasClaudeSymlink) {
    // Claude config exists but no symlink, suggest creating one
    return { 
      action: 'suggest_symlink', 
      message: 'Claude configuration found but no symlink. Would you like to create a CLAUDE.md symlink for Claude Code?' 
    };
  }
  
  // Default action: validate and report status
  return { 
    action: 'validate', 
    message: 'Validating configuration and checking integrations...',
    context: {
      hasProjectConfig,
      hasUserConfig,
      hasOrgConfig,
      hasClaudeConfig,
      hasGptConfig,
      hasClaudeSymlink,
      hasGptSymlink,
      hasClaudeRC
    }
  };
}

/**
 * Execute the appropriate action based on context analysis
 */
export async function executeAction(contextResult, options = {}) {
  console.log(chalk.blue(contextResult.message));
  
  switch (contextResult.action) {
    case 'ask':
      try {
        const { askAssistant } = await import('./ask.js');
        await askAssistant(options.ask, options);
      } catch (error) {
        console.error(chalk.red(`\nError processing request: ${error.message}`));
      }
      break;
      
    case 'init':
      try {
        // Simple initialization inline rather than importing
        await initializeConfig(options);
      } catch (error) {
        console.error(chalk.red(`\nError initializing: ${error.message}`));
      }
      break;
      
    case 'adapt':
      try {
        // Simplified adaptation for demo
        await adaptConfig(options);
      } catch (error) {
        console.error(chalk.red(`\nError adapting configuration: ${error.message}`));
      }
      break;
      
    case 'suggest_adapt':
      console.log(chalk.yellow('\nConfiguration exists but no assistant integrations found.'));
      console.log(chalk.yellow('Would you like to generate configurations for:'));
      console.log('1. Claude');
      console.log('2. GPT');
      console.log('3. Both');
      console.log('4. Skip');
      
      console.log(chalk.green('\nRun one of the following commands:'));
      console.log(chalk.cyan('continuum --assistant claude'));
      console.log(chalk.cyan('continuum --assistant gpt'));
      console.log(chalk.cyan('continuum --assistant claude --assistant gpt'));
      break;
      
    case 'suggest_symlink':
      console.log(chalk.yellow('\nClaude configuration found but no symlink for Claude Code integration.'));
      console.log(chalk.green('\nRun the following command to create a symlink:'));
      console.log(chalk.cyan('continuum --assistant claude --create-link'));
      
      if (!contextResult.context.hasClaudeRC) {
        console.log(chalk.yellow('\nTip: Create a .clauderc file with:'));
        console.log(`{
  "systemPromptFile": "CLAUDE.md"
}`);
      }
      break;
      
    case 'validate':
      try {
        // Simplified validation for demo
        await validateConfig(options);
        
        // Show integration status
        const ctx = contextResult.context;
        
        console.log(chalk.blue('\n🧩 Integration Status:'));
        if (ctx.hasClaudeConfig) {
          console.log(`Claude: ${chalk.green('✅ Configured')} ${ctx.hasClaudeSymlink ? chalk.green('✅ Symlinked') : chalk.yellow('⚠️ No symlink')}`);
          if (ctx.hasClaudeRC) {
            console.log(`Claude Code: ${chalk.green('✅ .clauderc found')}`);
          } else {
            console.log(`Claude Code: ${chalk.yellow('⚠️ No .clauderc found')}`);
          }
        } else {
          console.log(`Claude: ${chalk.yellow('⚠️ Not configured')}`);
        }
        
        if (ctx.hasGptConfig) {
          console.log(`GPT: ${chalk.green('✅ Configured')} ${ctx.hasGptSymlink ? chalk.green('✅ Symlinked') : chalk.yellow('⚠️ No symlink')}`);
        } else {
          console.log(`GPT: ${chalk.yellow('⚠️ Not configured')}`);
        }
        
        // Suggest next steps
        const suggestions = [];
        
        if (!ctx.hasClaudeConfig) {
          suggestions.push('Generate Claude configuration: continuum --assistant claude');
        } else if (!ctx.hasClaudeSymlink) {
          suggestions.push('Create Claude symlink: continuum --assistant claude --create-link');
        }
        
        if (!ctx.hasGptConfig) {
          suggestions.push('Generate GPT configuration: continuum --assistant gpt');
        } else if (!ctx.hasGptSymlink) {
          suggestions.push('Create GPT symlink: continuum --assistant gpt --create-link');
        }
        
        if (!ctx.hasClaudeRC && ctx.hasClaudeSymlink) {
          suggestions.push('Create .clauderc file for Claude Code integration');
        }
        
        if (suggestions.length > 0) {
          console.log(chalk.blue('\n📋 Suggested actions:'));
          suggestions.forEach((suggestion, index) => {
            console.log(`${index + 1}. ${suggestion}`);
          });
        } else {
          console.log(chalk.green('\n🎉 All integrations are set up! Your AI assistants are ready to collaborate.'));
        }
      } catch (error) {
        console.error(chalk.red(`\nError validating: ${error.message}`));
      }
      break;
      
    default:
      console.log(chalk.yellow('\nNo specific action needed based on current context.'));
      console.log(chalk.green('Your configuration is ready for use.'));
  }
}

/**
 * Check if we're in a git repository
 */
async function checkIsGitRepo() {
  try {
    // Check if .git directory exists
    return existsSync('.git');
  } catch (error) {
    return false;
  }
}

/**
 * Adapt configuration for specific assistant
 */
async function adaptConfig(options) {
  const assistant = options.assistant;
  if (!assistant) {
    throw new Error('Assistant name is required');
  }
  
  // Get input and output paths
  const configPath = options.config || PROJECT_CONFIG_PATH;
  const outputDir = `.continuum/${assistant.toLowerCase()}`;
  const outputExt = assistant.toLowerCase() === 'gpt' ? 'json' : 'md';
  const outputPath = options.output || `${outputDir}/config.${outputExt}`;
  
  console.log(chalk.yellow(`\nAdapting configuration for ${assistant}...`));
  console.log(`Input: ${chalk.green(configPath)}`);
  console.log(`Output: ${chalk.green(outputPath)}`);
  
  // Check if input file exists
  if (!existsSync(configPath)) {
    throw new Error(`Configuration file not found: ${configPath}`);
  }
  
  try {
    // Read configuration file
    const content = await fsPromises.readFile(configPath, 'utf-8');
    
    // Generate assistant-specific configuration
    let adaptedContent;
    
    if (assistant.toLowerCase() === 'claude') {
      adaptedContent = `# Continuum Configuration for Claude

## Role and Goal
You are a development collaborator. Your purpose is to help maintain code quality and guide development.

## Constraints
- Follow the project's best practices
- Must not introduce security vulnerabilities 
- Maintain a professional tone
- Exercise moderate risk tolerance

## Guidelines
- Suggest improvements but allow the developer to make final decisions
- Provide concise explanations when suggesting changes
- Review code thoroughly before suggesting changes

## Allowed Capabilities
- Code review
- Refactoring
- Documentation
- Testing

## Restricted Capabilities
- Deployment
- Database management

## Additional Instructions
This configuration was generated using Continuum - designed by AI and humans for AI and humans.`;
    } else if (assistant.toLowerCase() === 'gpt') {
      adaptedContent = JSON.stringify({
        model: "gpt-4o",
        messages: [
          {
            role: "system",
            content: `You are a development collaborator. Your purpose is to help maintain code quality and guide development.

# Configuration Parameters
- Voice: professional
- Autonomy: suggest (not dictate)
- Verbosity: concise
- Risk Tolerance: medium

# Allowed Capabilities
- Code review
- Refactoring  
- Documentation
- Testing

# Restricted Capabilities
- Deployment
- Database management`
          }
        ],
        temperature: 0.7
      }, null, 2);
    } else {
      adaptedContent = `# Continuum Configuration for ${assistant}

Adapted from project configuration.`;
    }
    
    // Create directory if it doesn't exist
    const outputDirPath = path.dirname(outputPath);
    if (outputDirPath !== '.') {
      await fsPromises.mkdir(outputDirPath, { recursive: true });
      console.log(`Created directory: ${outputDirPath}`);
    }
    
    // Write adapted content
    await fsPromises.writeFile(outputPath, adaptedContent);
    console.log(chalk.green(`\nAdapted configuration written to ${outputPath}`));
    
    // Create symlink if requested
    if (options.createLink) {
      const linkName = assistant.toLowerCase() === 'claude' ? 'CLAUDE.md' : 
                      (assistant.toLowerCase() === 'gpt' ? 'GPT.json' : 
                      `${assistant.toUpperCase()}.txt`);
                      
      const linkPath = path.resolve(process.cwd(), linkName);
      
      // Remove existing symlink if it exists
      try {
        if (existsSync(linkPath)) {
          await fsPromises.unlink(linkPath);
        }
      } catch (e) {
        // Continue even if removal fails
      }
      
      try {
        // Create relative path for symlink
        const relativePath = path.relative(process.cwd(), outputPath);
        
        // Create symlink
        await fsPromises.symlink(relativePath, linkPath);
        console.log(chalk.green(`\nSymlink created at: ${linkPath}`));
      } catch (e) {
        console.error(chalk.yellow(`\nCould not create symlink: ${e.message}`));
        console.error(chalk.yellow('You may need administrative privileges to create symlinks.'));
      }
    }
    
    // Suggest next steps
    if (assistant.toLowerCase() === 'claude' && !existsSync('.clauderc')) {
      console.log(chalk.yellow('\nTip: Create a .clauderc file with:'));
      console.log(`{
  "systemPromptFile": "CLAUDE.md"
}`);
    }
  } catch (err) {
    throw new Error(`Error adapting configuration: ${err.message}`);
  }
}

/**
 * Validate configuration
 */
async function validateConfig(options) {
  const configPath = options.config || PROJECT_CONFIG_PATH;
  
  console.log(chalk.yellow(`\nValidating configuration: ${configPath}`));
  
  // Check if config file exists
  if (!existsSync(configPath)) {
    throw new Error(`Configuration file not found: ${configPath}`);
  }
  
  // Read the configuration file
  const content = await fsPromises.readFile(configPath, 'utf-8');
  
  // You could do more validation here, such as checking YAML syntax
  
  console.log(chalk.green(`\nConfiguration file exists at ${configPath}`));
  console.log(chalk.green('Basic validation successful.'));
}

/**
 * Initialize configuration
 */
async function initializeConfig(options) {
  console.log(chalk.yellow('\nInitializing configuration...'));
  
  const template = options.template || 'standard';
  const outputPath = options.output || PROJECT_CONFIG_PATH;
  
  // Show what we'll do
  console.log(`Template: ${chalk.green(template)}`);
  console.log(`Output: ${chalk.green(outputPath)}`);
  
  // Create config
  const config = {
    ai_protocol_version: "0.1",
    identity: {
      name: "ProjectAssistant",
      role: "Development collaborator",
      purpose: "Help maintain code quality and guide development"
    },
    behavior: {
      voice: "professional",
      autonomy: "suggest",
      verbosity: "concise",
      risk_tolerance: "medium"
    },
    capabilities: {
      allowed: ["code_review", "refactoring", "documentation", "testing"],
      restricted: ["deployment", "database_management"]
    }
  };
  
  // Generate markdown - using a simple YAML stringifier
  function yamlStringify(obj, indent = 0) {
    const spaces = ' '.repeat(indent);
    let result = '';
    
    for (const [key, value] of Object.entries(obj)) {
      if (Array.isArray(value)) {
        result += `${spaces}${key}:\n`;
        for (const item of value) {
          result += `${spaces}  - "${item}"\n`;
        }
      } else if (typeof value === 'object' && value !== null) {
        result += `${spaces}${key}:\n${yamlStringify(value, indent + 2)}`;
      } else {
        result += `${spaces}${key}: "${value}"\n`;
      }
    }
    
    return result;
  }
  
  const yamlStr = yamlStringify(config);
  const content = `# Continuum Configuration

\`\`\`yaml
${yamlStr}
\`\`\`

## Additional Instructions

This configuration was generated using Continuum - designed by AI and humans for AI and humans.

### Project Context

This defines how AI assistants should interact with this project.

### Workflow Examples

AI assistants should follow the configuration above when working with this project.
`;
  
  // Create directory if it doesn't exist
  const outputDir = path.dirname(outputPath);
  if (outputDir !== '.') {
    try {
      await fsPromises.mkdir(outputDir, { recursive: true });
      console.log(`Created directory: ${outputDir}`);
    } catch (err) {
      if (err.code !== 'EEXIST') {
        console.error(chalk.red(`\nError creating directory: ${err.message}`));
        return;
      }
    }
  }
  
  await fsPromises.writeFile(outputPath, content);
  console.log(chalk.green(`\nConfiguration written to ${outputPath}`));
  
  // Suggest next steps
  console.log(chalk.blue('\n📋 Suggested next steps:'));
  console.log('1. Generate Claude configuration: continuum --assistant claude');
  console.log('2. Generate GPT configuration: continuum --assistant gpt');
  console.log('3. Create symlinks for tool integration: continuum --create-link');
}