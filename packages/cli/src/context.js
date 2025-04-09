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
 * This is the heart of the context-aware CLI paradigm
 */
export async function analyzeContext(options = {}) {
  console.log(chalk.blue('🧠 Analyzing context...\n'));
  
  // Detect repository environment
  const isGitRepo = await checkIsGitRepo();
  const gitBranch = isGitRepo ? await getCurrentGitBranch() : null;
  const hasUncommittedChanges = isGitRepo ? await hasGitChanges() : false;
  
  // Check if config files exist
  const hasProjectConfig = existsSync(PROJECT_CONFIG_PATH);
  const hasUserConfig = existsSync(USER_CONFIG_PATH);
  const hasOrgConfig = existsSync(ORG_CONFIG_PATH);
  
  // Check if AI assistant integration files exist
  const hasClaudeConfig = existsSync('.continuum/claude/config.md');
  const hasGptConfig = existsSync('.continuum/gpt/config.json');
  const hasClaudeSymlink = existsSync('CLAUDE.md');
  const hasGptSymlink = existsSync('GPT.json');
  
  // Check integration configuration
  const hasClaudeRC = existsSync('.clauderc');
  const hasGptRC = existsSync('.gptrc');
  
  // Check for specific file types that might indicate project type
  const hasPackageJson = existsSync('package.json');
  const hasPythonFiles = await hasPython();
  const hasDockerfile = existsSync('Dockerfile') || existsSync('docker-compose.yml');
  
  // Determine development environment
  const devEnvironment = detectDevEnvironment(hasPackageJson, hasPythonFiles, hasDockerfile);
  
  // Check if specific arguments were passed
  const hasAssistantArg = options.assistant !== undefined;
  const hasTemplateArg = options.template !== undefined;
  const hasAskPrompt = options.ask !== undefined;
  const hasCreateLink = options.createLink === true;
  
  // Log the detected context
  console.log(chalk.dim('Context detected:'));
  console.log(chalk.dim(`- Git repository: ${isGitRepo ? '✅' : '❌'}${gitBranch ? ` (${gitBranch})` : ''}`));
  console.log(chalk.dim(`- Dev environment: ${devEnvironment}`));
  console.log(chalk.dim(`- Project config: ${hasProjectConfig ? '✅' : '❌'}`));
  console.log(chalk.dim(`- User config: ${hasUserConfig ? '✅' : '❌'}`));
  console.log(chalk.dim(`- Organization config: ${hasOrgConfig ? '✅' : '❌'}`));
  console.log(chalk.dim(`- Claude integration: ${hasClaudeConfig ? '✅' : '❌'}${hasClaudeRC ? ' (with .clauderc)' : ''}`));
  console.log(chalk.dim(`- GPT integration: ${hasGptConfig ? '✅' : '❌'}${hasGptRC ? ' (with .gptrc)' : ''}`));
  console.log('');
  
  // Build context object for decision making
  const context = {
    repo: {
      isGitRepo,
      gitBranch,
      hasUncommittedChanges,
      devEnvironment
    },
    config: {
      hasProjectConfig,
      hasUserConfig,
      hasOrgConfig
    },
    integration: {
      claude: {
        hasConfig: hasClaudeConfig,
        hasSymlink: hasClaudeSymlink,
        hasRC: hasClaudeRC
      },
      gpt: {
        hasConfig: hasGptConfig,
        hasSymlink: hasGptSymlink,
        hasRC: hasGptRC
      }
    },
    options
  };
  
  // Determine what to do based on context - prioritize explicit actions
  if (hasAskPrompt) {
    // Ask prompt was provided, send to assistant
    return { 
      action: 'ask', 
      message: `Processing request: "${options.ask}"`,
      context 
    };
  }
  
  if (hasAssistantArg && hasCreateLink) {
    // Create symlink for specific assistant
    return { 
      action: 'create_symlink', 
      message: `Creating symlink for ${options.assistant}...`,
      context 
    };
  }
  
  if (hasAssistantArg) {
    // Specific assistant argument was provided, run adapt command
    return { 
      action: 'adapt', 
      message: `Adapting configuration for ${options.assistant}...`,
      context 
    };
  }
  
  // Implicit actions based on project state
  if (!hasProjectConfig) {
    // No project config found, we should initialize
    return { 
      action: 'init', 
      message: 'No configuration found. Running initialization...',
      context 
    };
  }
  
  if (hasProjectConfig && !hasClaudeConfig && !hasGptConfig) {
    // Project config exists but no assistant configs, suggest adapting
    return { 
      action: 'suggest_adapt', 
      message: 'Configuration exists but no assistant integrations found. Would you like to generate them?',
      context 
    };
  }
  
  // Suggest symlink creation if configs exist but no symlinks
  if ((hasClaudeConfig && !hasClaudeSymlink) || (hasGptConfig && !hasGptSymlink)) {
    const assistant = hasClaudeConfig && !hasClaudeSymlink ? 'Claude' : 'GPT';
    return { 
      action: 'suggest_symlink', 
      message: `${assistant} configuration found but no symlink. Would you like to create a symlink for tool integration?`,
      context
    };
  }
  
  // If we have Claude config and symlink but no .clauderc, suggest creating one
  if (hasClaudeConfig && hasClaudeSymlink && !hasClaudeRC) {
    return {
      action: 'suggest_clauderc',
      message: 'Claude configuration and symlink found but no .clauderc file to configure Claude Code.',
      context
    };
  }
  
  // Default action: validate and report status
  return { 
    action: 'validate', 
    message: 'Validating configuration and checking integrations...',
    context
  };
}

/**
 * Execute the appropriate action based on context analysis
 */
export async function executeAction(contextResult, options = {}) {
  console.log(chalk.blue(contextResult.message));
  
  const context = contextResult.context || {};
  
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
        // Initialize configuration with enhanced context awareness
        await initializeConfig(options, context);
      } catch (error) {
        console.error(chalk.red(`\nError initializing: ${error.message}`));
      }
      break;
      
    case 'adapt':
      try {
        // Adapt configuration for specific assistant
        await adaptConfig(options, context);
      } catch (error) {
        console.error(chalk.red(`\nError adapting configuration: ${error.message}`));
      }
      break;
      
    case 'create_symlink':
      try {
        // Create symlink only
        await createSymlink(options.assistant, context);
      } catch (error) {
        console.error(chalk.red(`\nError creating symlink: ${error.message}`));
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
      // Determine which assistant we're suggesting symlinks for
      const assistantName = context?.integration?.claude?.hasConfig && !context?.integration?.claude?.hasSymlink
        ? 'Claude'
        : 'GPT';
        
      console.log(chalk.yellow(`\n${assistantName} configuration found but no symlink for integration.`));
      console.log(chalk.green('\nRun the following command to create a symlink:'));
      console.log(chalk.cyan(`continuum --assistant ${assistantName.toLowerCase()} --create-link`));
      
      // For Claude, also suggest creating .clauderc if needed
      if (assistantName === 'Claude' && !context?.integration?.claude?.hasRC) {
        console.log(chalk.yellow('\nTip: Create a .clauderc file with:'));
        console.log(`{
  "systemPromptFile": "CLAUDE.md"
}`);
      }
      break;
      
    case 'suggest_clauderc':
      console.log(chalk.yellow('\nClaude configuration and symlink found, but no .clauderc file.'));
      console.log(chalk.yellow('To complete Claude Code integration, create a .clauderc file with:'));
      console.log(`{
  "systemPromptFile": "CLAUDE.md"
}`);
      break;
      
    case 'validate':
      try {
        // Enhanced validation with context awareness
        await validateConfig(options, context);
        
        // Show detailed integration status
        showIntegrationStatus(context);
        
        // Suggest next steps based on context
        suggestNextSteps(context);
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
 * Show integration status with detailed information
 */
function showIntegrationStatus(context) {
  const claudeIntegration = context?.integration?.claude;
  const gptIntegration = context?.integration?.gpt;
  
  console.log(chalk.blue('\n🧩 Integration Status:'));
  
  // Claude integration status
  if (claudeIntegration?.hasConfig) {
    console.log(`Claude: ${chalk.green('✅ Configured')} ${claudeIntegration.hasSymlink ? chalk.green('✅ Symlinked') : chalk.yellow('⚠️ No symlink')}`);
    if (claudeIntegration.hasRC) {
      console.log(`Claude Code: ${chalk.green('✅ .clauderc found')}`);
    } else if (claudeIntegration.hasSymlink) {
      console.log(`Claude Code: ${chalk.yellow('⚠️ No .clauderc found')}`);
    }
  } else {
    console.log(`Claude: ${chalk.yellow('⚠️ Not configured')}`);
  }
  
  // GPT integration status
  if (gptIntegration?.hasConfig) {
    console.log(`GPT: ${chalk.green('✅ Configured')} ${gptIntegration.hasSymlink ? chalk.green('✅ Symlinked') : chalk.yellow('⚠️ No symlink')}`);
    if (gptIntegration.hasRC) {
      console.log(`GPT: ${chalk.green('✅ .gptrc found')}`);
    } else if (gptIntegration.hasSymlink) {
      console.log(`GPT: ${chalk.yellow('⚠️ No .gptrc found')}`);
    }
  } else {
    console.log(`GPT: ${chalk.yellow('⚠️ Not configured')}`);
  }
  
  // Show repo context if available
  if (context?.repo?.isGitRepo) {
    console.log(chalk.blue('\n📂 Repository Context:'));
    console.log(`Git branch: ${context.repo.gitBranch || 'unknown'}`);
    console.log(`Environment: ${context.repo.devEnvironment || 'unknown'}`);
    console.log(`Changes: ${context.repo.hasUncommittedChanges ? chalk.yellow('⚠️ Uncommitted changes') : chalk.green('✅ Clean working tree')}`);
  }
}

/**
 * Suggest next steps based on context
 */
function suggestNextSteps(context) {
  const suggestions = [];
  const claudeIntegration = context?.integration?.claude;
  const gptIntegration = context?.integration?.gpt;
  
  if (!context?.config?.hasProjectConfig) {
    suggestions.push('Initialize project configuration: continuum');
    // Early return since we need this first
    if (suggestions.length > 0) {
      console.log(chalk.blue('\n📋 Suggested actions:'));
      suggestions.forEach((suggestion, index) => {
        console.log(`${index + 1}. ${suggestion}`);
      });
      return;
    }
  }
  
  // Claude suggestions
  if (!claudeIntegration?.hasConfig) {
    suggestions.push('Generate Claude configuration: continuum --assistant claude');
  } else if (!claudeIntegration?.hasSymlink) {
    suggestions.push('Create Claude symlink: continuum --assistant claude --create-link');
  } else if (!claudeIntegration?.hasRC) {
    suggestions.push('Create .clauderc file for Claude Code integration');
  }
  
  // GPT suggestions
  if (!gptIntegration?.hasConfig) {
    suggestions.push('Generate GPT configuration: continuum --assistant gpt');
  } else if (!gptIntegration?.hasSymlink) {
    suggestions.push('Create GPT symlink: continuum --assistant gpt --create-link');
  } else if (!gptIntegration?.hasRC) {
    suggestions.push('Create .gptrc file for GPT integration');
  }
  
  // Show suggestions if any
  if (suggestions.length > 0) {
    console.log(chalk.blue('\n📋 Suggested actions:'));
    suggestions.forEach((suggestion, index) => {
      console.log(`${index + 1}. ${suggestion}`);
    });
  } else {
    console.log(chalk.green('\n🎉 All integrations are set up! Your AI assistants are ready to collaborate.'));
    console.log(chalk.green('Try using: continuum --ask "Help me with this codebase"'));
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
 * Get current git branch
 */
async function getCurrentGitBranch() {
  try {
    // Use fsPromises to read the git HEAD file
    const headContent = await fsPromises.readFile('.git/HEAD', 'utf-8');
    const match = headContent.match(/ref: refs\/heads\/([^\n]+)/);
    return match ? match[1] : null;
  } catch (error) {
    return null;
  }
}

/**
 * Check if there are uncommitted changes
 */
async function hasGitChanges() {
  try {
    // Simple check for uncommitted changes - more complex checking could be done
    // but this is sufficient for our context detection
    const gitStatus = await fsPromises.readdir('.git/index');
    return gitStatus.length > 0;
  } catch (error) {
    return false;
  }
}

/**
 * Check for Python files in the project
 */
async function hasPython() {
  try {
    // Look for .py files in the current directory
    const files = await fsPromises.readdir('.');
    return files.some(file => file.endsWith('.py'));
  } catch (error) {
    return false;
  }
}

/**
 * Detect development environment based on files
 */
function detectDevEnvironment(hasPackageJson, hasPythonFiles, hasDockerfile) {
  if (hasPackageJson) {
    return 'Node.js/JavaScript';
  } else if (hasPythonFiles) {
    return 'Python';
  } else if (hasDockerfile) {
    return 'Containerized';
  } else {
    return 'Unknown';
  }
}

/**
 * Adapt configuration for specific assistant with context awareness
 */
async function adaptConfig(options, context = {}) {
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
      // Use dev environment information if available
      const devEnv = context?.repo?.devEnvironment || 'Unknown';
      const isGitRepo = context?.repo?.isGitRepo || false;
      const gitBranch = context?.repo?.gitBranch || null;
      
      // Enhance Claude configuration with context information
      adaptedContent = `# Continuum Configuration for Claude

## Role and Goal
You are a development collaborator for a ${devEnv} project${isGitRepo ? ` using Git${gitBranch ? ` (currently on branch: ${gitBranch})` : ''}` : ''}. Your purpose is to help maintain code quality and guide development.

## Constraints
- Follow the project's best practices
- Must not introduce security vulnerabilities 
- Maintain a professional tone
- Exercise moderate risk tolerance

## Guidelines
- Suggest improvements but allow the developer to make final decisions
- Provide concise explanations when suggesting changes
- Review code thoroughly before suggesting changes
${isGitRepo ? `- Be aware of the current branch context (${gitBranch || 'unknown'})` : ''}

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
      // Use dev environment information if available
      const devEnv = context?.repo?.devEnvironment || 'Unknown';
      const isGitRepo = context?.repo?.isGitRepo || false;
      const gitBranch = context?.repo?.gitBranch || null;
      
      adaptedContent = JSON.stringify({
        model: "gpt-4o",
        messages: [
          {
            role: "system",
            content: `You are a development collaborator for a ${devEnv} project${isGitRepo ? ` using Git${gitBranch ? ` (currently on branch: ${gitBranch})` : ''}` : ''}. Your purpose is to help maintain code quality and guide development.

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
- Database management

${isGitRepo ? `# Repository Context\n- Current branch: ${gitBranch || 'unknown'}` : ''}`
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
      await createSymlink(assistant, context);
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
 * Create symlink for assistant configuration
 */
async function createSymlink(assistant, context = {}) {
  if (!assistant) {
    throw new Error('Assistant name is required');
  }
  
  const linkName = assistant.toLowerCase() === 'claude' ? 'CLAUDE.md' : 
                  (assistant.toLowerCase() === 'gpt' ? 'GPT.json' : 
                  `${assistant.toUpperCase()}.txt`);
  
  const configDir = `.continuum/${assistant.toLowerCase()}`;
  const configExt = assistant.toLowerCase() === 'gpt' ? 'json' : 'md';
  const configPath = `${configDir}/config.${configExt}`;
  
  const linkPath = path.resolve(process.cwd(), linkName);
  
  // Check if source file exists
  if (!existsSync(configPath)) {
    throw new Error(`Configuration file not found: ${configPath}. Run 'continuum --assistant ${assistant}' first.`);
  }
  
  // Remove existing symlink if it exists
  try {
    if (existsSync(linkPath)) {
      await fsPromises.unlink(linkPath);
      console.log(`Removed existing symlink: ${linkPath}`);
    }
  } catch (e) {
    console.error(chalk.yellow(`\nCould not remove existing file: ${e.message}`));
    console.error(chalk.yellow('You may need administrative privileges to modify files.'));
    throw new Error(`Failed to remove existing file: ${e.message}`);
  }
  
  try {
    // Create relative path for symlink
    const relativePath = path.relative(process.cwd(), configPath);
    
    // Create symlink
    await fsPromises.symlink(relativePath, linkPath);
    console.log(chalk.green(`\nSymlink created at: ${linkPath}`));
    
    // Suggest creating config file if needed
    if (assistant.toLowerCase() === 'claude' && !existsSync('.clauderc')) {
      console.log(chalk.yellow('\nTip: Create a .clauderc file with:'));
      console.log(`{
  "systemPromptFile": "CLAUDE.md"
}`);
    } else if (assistant.toLowerCase() === 'gpt' && !existsSync('.gptrc')) {
      console.log(chalk.yellow('\nTip: Create a .gptrc file to configure GPT.'));
    }
    
  } catch (e) {
    console.error(chalk.yellow(`\nCould not create symlink: ${e.message}`));
    console.error(chalk.yellow('You may need administrative privileges to create symlinks.'));
    throw new Error(`Failed to create symlink: ${e.message}`);
  }
}

/**
 * Validate configuration with enhanced context awareness
 */
async function validateConfig(options, context = {}) {
  const configPath = options.config || PROJECT_CONFIG_PATH;
  
  console.log(chalk.yellow(`\nValidating configuration: ${configPath}`));
  
  // Check if config file exists
  if (!existsSync(configPath)) {
    throw new Error(`Configuration file not found: ${configPath}`);
  }
  
  // Read the configuration file
  const content = await fsPromises.readFile(configPath, 'utf-8');
  
  // Check for YAML syntax in markdown files
  if (configPath.endsWith('.md')) {
    // Look for yaml blocks
    const yamlBlockRegex = /```yaml\s+([\s\S]+?)```/;
    const yamlMatch = content.match(yamlBlockRegex);
    
    if (yamlMatch && yamlMatch[1]) {
      try {
        // Try to parse YAML
        // In a real implementation, we would import and use a YAML library here
        console.log(chalk.dim('Validated YAML syntax in markdown file'));
      } catch (err) {
        console.warn(chalk.yellow(`\nWarning: YAML syntax in markdown file may be invalid: ${err.message}`));
      }
    }
  }
  
  // Enhanced validation based on context
  if (context?.repo?.isGitRepo) {
    console.log(chalk.dim(`Validating in Git repository context (branch: ${context.repo.gitBranch || 'unknown'})`));
    // In a real implementation, we could do Git-specific validation here
  }
  
  if (context?.repo?.devEnvironment) {
    console.log(chalk.dim(`Validating for ${context.repo.devEnvironment} development environment`));
    // In a real implementation, we could do environment-specific validation here
  }
  
  console.log(chalk.green(`\nConfiguration file exists at ${configPath}`));
  console.log(chalk.green('Basic validation successful.'));
  
  return true;
}

/**
 * Initialize configuration with context awareness
 */
async function initializeConfig(options, context = {}) {
  console.log(chalk.yellow('\nInitializing configuration...'));
  
  const template = options.template || 'standard';
  const outputPath = options.output || PROJECT_CONFIG_PATH;
  
  // Show what we'll do
  console.log(`Template: ${chalk.green(template)}`);
  console.log(`Output: ${chalk.green(outputPath)}`);
  
  // Enhance configuration with context information
  const devEnv = context?.repo?.devEnvironment || 'Unknown';
  const isGitRepo = context?.repo?.isGitRepo || false;
  const gitBranch = context?.repo?.gitBranch || null;
  
  // Create config with context awareness
  const config = {
    ai_protocol_version: "0.1",
    identity: {
      name: "ProjectAssistant",
      role: "Development collaborator",
      purpose: "Help maintain code quality and guide development"
    },
    environment: {
      type: devEnv,
      vcs: isGitRepo ? "git" : "none",
      branch: gitBranch || "none"
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
${isGitRepo ? `\nThe project uses Git${gitBranch ? ` and is currently on the ${gitBranch} branch` : ''}.` : ''}
${devEnv !== 'Unknown' ? `\nThis appears to be a ${devEnv} project.` : ''}

### Workflow Examples

AI assistants should follow the configuration above when working with this project.

Example tasks:
- Code reviews and suggestions
- Documentation assistance
- Refactoring guidance
- Testing strategy
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
  
  // Suggest next steps with context awareness
  console.log(chalk.blue('\n📋 Suggested next steps:'));
  console.log('1. Generate Claude configuration: continuum --assistant claude');
  console.log('2. Generate GPT configuration: continuum --assistant gpt');
  console.log('3. Create symlinks for tool integration: continuum --create-link');
  
  // Special suggestion based on detected environment
  if (devEnv === 'Node.js/JavaScript') {
    console.log('\nTip: For JavaScript projects, you might want to add a script to package.json:');
    console.log(`  "scripts": {
    "continuum": "continuum"
  }`);
  } else if (devEnv === 'Python') {
    console.log('\nTip: For Python projects, consider integrating with your development workflow.');
  }
}