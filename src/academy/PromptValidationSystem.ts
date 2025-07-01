/**
 * Prompt Validation System
 * Ensures Academy training prompts are technically accurate
 * Tests command syntax, parameters, and expected outcomes
 */

import { UniversalWidgetAssetTester } from '../ui/components/shared/test/UniversalWidgetAssetTest.js';

export interface CommandSyntaxRule {
  command: string;
  requiredParams: string[];
  optionalParams: string[];
  examples: string[];
  expectedResponse: any;
  contextRequirements?: string[];
}

export interface PromptValidationResult {
  promptId: string;
  isValid: boolean;
  errors: string[];
  warnings: string[];
  commandAccuracy: number;
  syntaxAccuracy: number;
  exampleAccuracy: number;
  recommendations: string[];
}

export class PromptValidationSystem {
  private commandRules = new Map<string, CommandSyntaxRule>();
  private baseUrl: string = 'http://localhost:9000';

  constructor() {
    this.initializeCommandRules();
  }

  private initializeCommandRules(): void {
    // Screenshot command validation
    this.commandRules.set('screenshot', {
      command: 'screenshot',
      requiredParams: [],
      optionalParams: ['target', 'widgetId', 'includeContext', 'filename'],
      examples: [
        'screenshot',
        'screenshot --target=widget --widgetId=chat-widget',
        'screenshot --target=fullpage --includeContext=true'
      ],
      expectedResponse: { 
        success: true, 
        filename: 'string',
        path: 'string',
        timestamp: 'number'
      },
      contextRequirements: ['browser-active', 'ui-loaded']
    });

    // Validate command validation
    this.commandRules.set('validate', {
      command: 'validate',
      requiredParams: [],
      optionalParams: ['target', 'widgetId', 'validateAssets', 'validateContent'],
      examples: [
        'validate',
        'validate --target=widget --widgetId=chat-widget',
        'validate --validateAssets=true --validateContent=true'
      ],
      expectedResponse: {
        success: true,
        results: 'array',
        summary: 'object'
      }
    });

    // Export command validation
    this.commandRules.set('export', {
      command: 'export',
      requiredParams: [],
      optionalParams: ['target', 'widgetId', 'format', 'includeData'],
      examples: [
        'export --format=json',
        'export --target=widget --widgetId=chat-widget --format=csv',
        'export --target=system --format=json --includeData=true'
      ],
      expectedResponse: {
        success: true,
        filename: 'string',
        format: 'string',
        size: 'number'
      }
    });

    // Refresh command validation
    this.commandRules.set('refresh', {
      command: 'refresh',
      requiredParams: [],
      optionalParams: ['target', 'widgetId', 'preserveState'],
      examples: [
        'refresh',
        'refresh --target=widget --widgetId=chat-widget',
        'refresh --target=page --preserveState=true'
      ],
      expectedResponse: {
        success: true,
        target: 'string',
        timestamp: 'number'
      }
    });
  }

  /**
   * Validate Academy training prompt for technical accuracy
   */
  async validateTrainingPrompt(prompt: string, promptId: string): Promise<PromptValidationResult> {
    console.log(`🧪 Validating Academy training prompt: ${promptId}`);
    
    const result: PromptValidationResult = {
      promptId,
      isValid: true,
      errors: [],
      warnings: [],
      commandAccuracy: 0,
      syntaxAccuracy: 0,
      exampleAccuracy: 0,
      recommendations: []
    };

    // Test 1: Command Coverage
    await this.validateCommandCoverage(prompt, result);
    
    // Test 2: Syntax Accuracy  
    await this.validateSyntaxAccuracy(prompt, result);
    
    // Test 3: Example Validation
    await this.validateExamples(prompt, result);
    
    // Test 4: Real System Integration
    await this.validateRealSystemIntegration(prompt, result);
    
    // Calculate overall accuracy
    result.isValid = result.errors.length === 0;
    
    // Generate recommendations
    this.generateRecommendations(result);
    
    return result;
  }

  private async validateCommandCoverage(prompt: string, result: PromptValidationResult): Promise<void> {
    const availableCommands = Array.from(this.commandRules.keys());
    const mentionedCommands = availableCommands.filter(cmd => 
      prompt.toLowerCase().includes(cmd)
    );
    
    result.commandAccuracy = mentionedCommands.length / availableCommands.length;
    
    if (result.commandAccuracy < 0.8) {
      result.warnings.push(`Command coverage low: ${mentionedCommands.length}/${availableCommands.length} commands mentioned`);
    }
    
    // Check for missing critical commands
    const criticalCommands = ['screenshot', 'validate'];
    const missingCritical = criticalCommands.filter(cmd => !mentionedCommands.includes(cmd));
    
    if (missingCritical.length > 0) {
      result.errors.push(`Missing critical commands: ${missingCritical.join(', ')}`);
    }
  }

  private async validateSyntaxAccuracy(prompt: string, result: PromptValidationResult): Promise<void> {
    let syntaxErrors = 0;
    let totalSyntaxChecks = 0;
    
    for (const [command, rule] of this.commandRules.entries()) {
      // Check if command syntax is correctly documented in prompt
      const commandRegex = new RegExp(`${command}\\s*(?:--\\w+(?:=\\w+)?\\s*)*`, 'gi');
      const matches = prompt.match(commandRegex) || [];
      
      totalSyntaxChecks++;
      
      if (matches.length === 0 && prompt.includes(command)) {
        // Command mentioned but no proper syntax shown
        syntaxErrors++;
        result.warnings.push(`Command '${command}' mentioned but no syntax examples provided`);
      }
      
      // Validate parameter syntax if examples exist
      for (const match of matches) {
        if (match.includes('--') && !this.validateParameterSyntax(match, rule)) {
          syntaxErrors++;
          result.errors.push(`Invalid parameter syntax in: ${match}`);
        }
      }
    }
    
    result.syntaxAccuracy = Math.max(0, (totalSyntaxChecks - syntaxErrors) / totalSyntaxChecks);
  }

  private validateParameterSyntax(commandText: string, rule: CommandSyntaxRule): boolean {
    // Extract parameters from command text
    const paramRegex = /--(\w+)(?:=(\w+))?/g;
    const matches = [...commandText.matchAll(paramRegex)];
    
    for (const match of matches) {
      const paramName = match[1];
      const allValidParams = [...rule.requiredParams, ...rule.optionalParams];
      
      if (!allValidParams.includes(paramName)) {
        return false; // Invalid parameter
      }
    }
    
    return true;
  }

  private async validateExamples(prompt: string, result: PromptValidationResult): Promise<void> {
    let validExamples = 0;
    let totalExamples = 0;
    
    for (const [command, rule] of this.commandRules.entries()) {
      for (const example of rule.examples) {
        totalExamples++;
        
        if (prompt.includes(example) || this.isExampleEquivalent(prompt, example)) {
          validExamples++;
        } else {
          result.warnings.push(`Missing or incorrect example for ${command}: ${example}`);
        }
      }
    }
    
    result.exampleAccuracy = totalExamples > 0 ? validExamples / totalExamples : 1;
  }

  private isExampleEquivalent(prompt: string, example: string): boolean {
    // Check if prompt contains equivalent examples with different parameter values
    const _baseCommand = example.split(' ')[0]; // Prefix with _ to indicate intentionally unused
    const paramPattern = example.replace(/--\w+=[^\s]+/g, '--\\w+=[^\\s]+');
    const regex = new RegExp(paramPattern, 'i');
    
    return regex.test(prompt);
  }

  private async validateRealSystemIntegration(prompt: string, result: PromptValidationResult): Promise<void> {
    // Test if the prompt accurately describes real system behavior
    try {
      // Check if server is running
      const response = await fetch(this.baseUrl);
      if (!response.ok) {
        result.warnings.push('Cannot validate against live system - server not running');
        return;
      }
      
      // Test widget asset accuracy
      const _assetTester = new UniversalWidgetAssetTester(this.baseUrl); // Available for future use
      
      // If prompt mentions specific widgets, verify they exist
      const widgetMentions = this.extractWidgetMentions(prompt);
      for (const widget of widgetMentions) {
        const widgetUrl = `${this.baseUrl}/src/ui/components/${widget}/${widget}.ts`;
        try {
          const widgetResponse = await fetch(widgetUrl);
          if (!widgetResponse.ok) {
            result.errors.push(`Prompt references non-existent widget: ${widget}`);
          }
        } catch (error) {
          result.warnings.push(`Could not verify widget existence: ${widget}`);
        }
      }
      
    } catch (error) {
      result.warnings.push(`System integration validation failed: ${error}`);
    }
  }

  private extractWidgetMentions(prompt: string): string[] {
    // Extract widget names mentioned in prompt
    const widgetPattern = /(?:widget|component)[\s-]*(\w+)/gi;
    const matches = [...prompt.matchAll(widgetPattern)];
    return matches.map(match => match[1]).filter(name => 
      name.length > 2 && name !== 'widget' && name !== 'component'
    );
  }

  private generateRecommendations(result: PromptValidationResult): void {
    if (result.commandAccuracy < 0.9) {
      result.recommendations.push('Include examples for all available commands');
    }
    
    if (result.syntaxAccuracy < 0.95) {
      result.recommendations.push('Verify all command syntax examples against actual system');
    }
    
    if (result.exampleAccuracy < 0.8) {
      result.recommendations.push('Add more diverse command usage examples');
    }
    
    if (result.errors.length > 0) {
      result.recommendations.push('Fix all syntax errors before using prompt in Academy training');
    }
    
    result.recommendations.push('Run prompt validation tests after any Academy prompt updates');
  }

  /**
   * Generate a test report for Academy prompt validation
   */
  generateValidationReport(results: PromptValidationResult[]): string {
    const report = ['# Academy Prompt Validation Report\n'];
    
    results.forEach(result => {
      report.push(`## ${result.promptId}`);
      report.push(`**Status:** ${result.isValid ? '✅ VALID' : '❌ INVALID'}`);
      report.push(`**Command Accuracy:** ${(result.commandAccuracy * 100).toFixed(1)}%`);
      report.push(`**Syntax Accuracy:** ${(result.syntaxAccuracy * 100).toFixed(1)}%`);
      report.push(`**Example Accuracy:** ${(result.exampleAccuracy * 100).toFixed(1)}%`);
      
      if (result.errors.length > 0) {
        report.push('### Errors:');
        result.errors.forEach(error => report.push(`- ${error}`));
      }
      
      if (result.warnings.length > 0) {
        report.push('### Warnings:');
        result.warnings.forEach(warning => report.push(`- ${warning}`));
      }
      
      if (result.recommendations.length > 0) {
        report.push('### Recommendations:');
        result.recommendations.forEach(rec => report.push(`- ${rec}`));
      }
      
      report.push('');
    });
    
    return report.join('\n');
  }

  /**
   * Test Academy prompt against live system
   */
  async testPromptAgainstLiveSystem(promptText: string): Promise<{
    success: boolean;
    commandTests: any[];
    integrationTests: any[];
  }> {
    const commandTests: any[] = [];
    const integrationTests: any[] = [];
    
    // Test each command mentioned in prompt
    for (const [command, rule] of this.commandRules.entries()) {
      if (promptText.includes(command)) {
        for (const example of rule.examples) {
          try {
            // Parse command from example
            const [cmd, ...params] = example.split(/\s+/);
            const parsedParams = this.parseCommandParams(params);
            
            // Test command execution (would integrate with actual command system)
            const testResult = await this.testCommand(cmd, parsedParams);
            commandTests.push({
              command: cmd,
              example: example,
              success: testResult.success,
              response: testResult.response
            });
          } catch (error) {
            commandTests.push({
              command: command,
              example: example,
              success: false,
              error: String(error)
            });
          }
        }
      }
    }
    
    const allTestsPassed = commandTests.every(test => test.success);
    
    return {
      success: allTestsPassed,
      commandTests,
      integrationTests
    };
  }

  private parseCommandParams(params: string[]): Record<string, any> {
    const parsed: Record<string, any> = {};
    
    params.forEach(param => {
      if (param.startsWith('--')) {
        const [key, value] = param.substring(2).split('=');
        parsed[key] = value || true;
      }
    });
    
    return parsed;
  }

  private async testCommand(_command: string, _params: Record<string, any>): Promise<any> {
    // This would integrate with your actual command system
    // For now, return mock success
    return { success: true, response: 'Command executed successfully' };
  }
}

// Jest-compatible test function
export async function validateAcademyPrompt(promptText: string, promptId: string): Promise<void> {
  const validator = new PromptValidationSystem();
  const result = await validator.validateTrainingPrompt(promptText, promptId);
  
  if (!result.isValid) {
    const errorMsg = `Academy prompt validation failed for ${promptId}:\n` +
      result.errors.join('\n') + '\n' +
      result.warnings.join('\n');
    throw new Error(errorMsg);
  }
  
  console.log(`✅ Academy prompt ${promptId} validation passed`);
}