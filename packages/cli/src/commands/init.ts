/**
 * Implementation of the init command
 */

import inquirer from 'inquirer';
import * as path from 'path';
import * as fs from 'fs/promises';
import chalk from 'chalk';
import { AIConfig, writeConfigFile } from '@continuum/core';
import { getTemplate, listTemplates } from '../templates';

interface InitOptions {
  template: string;
  output: string;
}

export async function initCommand(options: InitOptions): Promise<void> {
  console.log(chalk.blue('Initializing new AI configuration...'));
  
  try {
    // Get available templates first if none specified
    if (!options.template) {
      const templates = await listTemplates();
      const templateAnswer = await inquirer.prompt([
        {
          type: 'list',
          name: 'template',
          message: 'Which template would you like to use?',
          choices: templates
        }
      ]);
      options.template = templateAnswer.template;
    }
    
    // Load template
    const baseConfig = await getTemplate(options.template);
    
    // Ask for basic information
    const answers = await inquirer.prompt([
      {
        type: 'input',
        name: 'name',
        message: 'What name should the AI assistant use?',
        default: baseConfig.identity?.name || 'ProjectAssistant'
      },
      {
        type: 'input',
        name: 'role',
        message: 'What is the primary role of the AI assistant?',
        default: baseConfig.identity?.role || 'Development collaborator'
      },
      {
        type: 'list',
        name: 'voice',
        message: 'What communication style should the assistant use?',
        choices: ['professional', 'friendly', 'academic', 'casual', 'technical'],
        default: baseConfig.behavior?.voice || 'professional'
      },
      {
        type: 'list',
        name: 'autonomy',
        message: 'What level of autonomy should the assistant have?',
        choices: [
          { name: 'Suggest only', value: 'suggest' },
          { name: 'Execute with approval', value: 'execute_with_approval' },
          { name: 'Fully autonomous', value: 'fully_autonomous' },
          { name: 'Restricted', value: 'restricted' }
        ],
        default: baseConfig.behavior?.autonomy || 'suggest'
      },
      {
        type: 'list',
        name: 'verbosity',
        message: 'What level of detail should the assistant provide?',
        choices: [
          { name: 'Concise', value: 'concise' },
          { name: 'Detailed', value: 'detailed' },
          { name: 'Comprehensive', value: 'comprehensive' }
        ],
        default: baseConfig.behavior?.verbosity || 'concise'
      },
      {
        type: 'checkbox',
        name: 'capabilities',
        message: 'What capabilities should the assistant have?',
        choices: [
          { name: 'Code review', value: 'code_review', checked: true },
          { name: 'Refactoring', value: 'refactoring', checked: true },
          { name: 'Documentation', value: 'documentation', checked: true },
          { name: 'Testing', value: 'testing', checked: true },
          { name: 'UI design', value: 'ui_design' },
          { name: 'API design', value: 'api_design' },
          { name: 'Deployment', value: 'deployment' },
          { name: 'Database management', value: 'database_management' }
        ]
      },
      {
        type: 'checkbox',
        name: 'restricted',
        message: 'What capabilities should be restricted?',
        choices: [
          { name: 'Deployment', value: 'deployment', checked: true },
          { name: 'Database management', value: 'database_management', checked: true },
          { name: 'External API access', value: 'external_api', checked: false },
          { name: 'User data access', value: 'user_data_access', checked: true },
          { name: 'Security critical changes', value: 'security_critical_changes', checked: true }
        ]
      }
    ]);
    
    // Template-specific questions based on the selected template
    let templateExtensions = {};
    
    if (options.template === 'tdd') {
      const tddAnswers = await inquirer.prompt([
        {
          type: 'confirm',
          name: 'test_first',
          message: 'Enforce tests before implementation?',
          default: true
        },
        {
          type: 'input',
          name: 'coverage_target',
          message: 'Target test coverage percentage:',
          default: 80,
          validate: (input) => {
            const num = parseInt(input);
            return (num > 0 && num <= 100) ? true : 'Please enter a number between 1 and 100';
          },
          filter: (input) => parseInt(input)
        }
      ]);
      
      templateExtensions = {
        tdd: {
          test_first: tddAnswers.test_first,
          coverage_target: tddAnswers.coverage_target,
          frameworks: baseConfig.extensions?.tdd?.frameworks || ['jest', 'mocha', 'pytest']
        }
      };
    } else if (options.template === 'enterprise') {
      const enterpriseAnswers = await inquirer.prompt([
        {
          type: 'checkbox',
          name: 'compliance_standards',
          message: 'Which compliance standards apply?',
          choices: [
            { name: 'SOC2', value: 'SOC2' },
            { name: 'GDPR', value: 'GDPR' },
            { name: 'HIPAA', value: 'HIPAA' },
            { name: 'PCI-DSS', value: 'PCI-DSS' }
          ],
          default: baseConfig.extensions?.compliance?.standards || []
        },
        {
          type: 'confirm',
          name: 'security_first',
          message: 'Prioritize security in all development?',
          default: true
        }
      ]);
      
      templateExtensions = {
        compliance: {
          standards: enterpriseAnswers.compliance_standards,
          enforcement: 'strict'
        },
        security: {
          security_first: enterpriseAnswers.security_first,
          prevent_vulnerabilities: baseConfig.extensions?.security?.prevent_vulnerabilities || [
            'sql_injection', 
            'xss', 
            'csrf'
          ]
        }
      };
    }
    
    // Update the configuration
    const config: AIConfig = {
      ...baseConfig,
      ai_protocol_version: '0.1',
      identity: {
        ...baseConfig.identity,
        name: answers.name,
        role: answers.role
      },
      behavior: {
        ...baseConfig.behavior,
        voice: answers.voice,
        autonomy: answers.autonomy,
        verbosity: answers.verbosity
      },
      capabilities: {
        ...baseConfig.capabilities,
        allowed: answers.capabilities,
        restricted: answers.restricted
      },
      extensions: {
        ...baseConfig.extensions,
        ...templateExtensions
      }
    };
    
    // Use default output if not specified
    if (!options.output) {
      options.output = 'AI_CONFIG.md';
    }
    
    // Create the output file
    const outputPath = path.resolve(process.cwd(), options.output);
    await writeConfigFile(config, outputPath);
    
    console.log(chalk.green(`\nConfiguration successfully created at ${outputPath}`));
    console.log(chalk.yellow('\nNext steps:'));
    console.log(`- Review the configuration in ${options.output}`);
    console.log('- Add additional context in the markdown file');
    console.log('- Run ai-config validate to ensure everything is correct');
    
  } catch (error) {
    console.error(chalk.red(`Error initializing configuration: ${error}`));
    process.exit(1);
  }
}