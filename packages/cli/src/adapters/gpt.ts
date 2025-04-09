/**
 * Adapter for OpenAI GPT assistant
 */

import { AIConfig, ConfigAdapter } from '@continuum/core';

export class GPTAdapter implements ConfigAdapter {
  name = 'gpt';
  
  async loadConfig(_path: string): Promise<AIConfig> {
    throw new Error('Method not implemented.');
  }
  
  mergeConfigs(_configs: AIConfig[]): AIConfig {
    throw new Error('Method not implemented.');
  }
  
  formatForAssistant(config: AIConfig): string {
    // Convert the configuration to GPT's system prompt format
    return this.generateSystemPrompt(config);
  }
  
  private generateSystemPrompt(config: AIConfig): string {
    let prompt = '';
    
    // Identity and purpose
    if (config.identity) {
      prompt += `You are ${config.identity.name || 'an AI assistant'}`;
      
      if (config.identity.role) {
        prompt += `, acting as ${config.identity.role}`;
      }
      
      prompt += '.\n';
      
      if (config.identity.purpose) {
        prompt += `Your purpose is to ${config.identity.purpose}.\n`;
      }
    }
    
    // Limitations
    if (config.identity?.limitations?.length) {
      prompt += '\nYou have the following limitations:\n';
      config.identity.limitations.forEach((limitation: string) => {
        prompt += `- ${limitation}\n`;
      });
    }
    
    // Behavior
    if (config.behavior) {
      prompt += '\nBehavior guidelines:\n';
      
      if (config.behavior.voice) {
        prompt += `- Communication style: ${this.describeVoice(config.behavior.voice)}\n`;
      }
      
      if (config.behavior.autonomy) {
        prompt += `- Autonomy level: ${this.describeAutonomy(config.behavior.autonomy)}\n`;
      }
      
      if (config.behavior.verbosity) {
        prompt += `- Verbosity: ${this.describeVerbosity(config.behavior.verbosity)}\n`;
      }
      
      if (config.behavior.risk_tolerance) {
        prompt += `- Risk tolerance: ${this.describeRiskTolerance(config.behavior.risk_tolerance)}\n`;
      }
    }
    
    // Capabilities
    if (config.capabilities) {
      if (config.capabilities.allowed?.length) {
        prompt += '\nYou are allowed to:\n';
        config.capabilities.allowed.forEach((capability: string) => {
          prompt += `- ${this.formatCapability(capability)}\n`;
        });
      }
      
      if (config.capabilities.restricted?.length) {
        prompt += '\nYou are NOT allowed to:\n';
        config.capabilities.restricted.forEach((capability: string) => {
          prompt += `- ${this.formatCapability(capability)}\n`;
        });
      }
    }
    
    // Knowledge
    if (config.knowledge) {
      prompt += '\nContext information:\n';
      
      if (config.knowledge.codebase) {
        if (config.knowledge.codebase.structure) {
          prompt += `- Project structure is documented at: ${config.knowledge.codebase.structure}\n`;
        }
        
        if (config.knowledge.codebase.conventions) {
          prompt += `- Coding conventions are documented at: ${config.knowledge.codebase.conventions}\n`;
        }
      }
      
      if (config.knowledge.context) {
        Object.entries(config.knowledge.context).forEach(([key, value]) => {
          prompt += `- ${key}: ${value}\n`;
        });
      }
    }
    
    // Extensions
    if (config.extensions) {
      // Handle TDD extension
      if (config.extensions.tdd) {
        prompt += '\nTest-Driven Development guidelines:\n';
        // eslint-disable-next-line @typescript-eslint/no-explicit-any
        const tdd = config.extensions.tdd as Record<string, unknown>;
        
        if (tdd.test_first) {
          prompt += '- You should always suggest writing tests before implementation code\n';
        }
        
        if (tdd.frameworks?.length) {
          prompt += `- Preferred testing frameworks: ${tdd.frameworks.join(', ')}\n`;
        }
        
        if (tdd.coverage_target) {
          prompt += `- Target test coverage: ${tdd.coverage_target}%\n`;
        }
      }
      
      // Handle compliance extension
      if (config.extensions.compliance) {
        prompt += '\nCompliance requirements:\n';
        // eslint-disable-next-line @typescript-eslint/no-explicit-any
        const compliance = config.extensions.compliance as Record<string, unknown>;
        
        if (compliance.standards?.length) {
          prompt += `- Compliance standards: ${compliance.standards.join(', ')}\n`;
        }
        
        if (compliance.enforcement) {
          prompt += `- Enforcement level: ${compliance.enforcement}\n`;
        }
      }
    }
    
    // Final instructions
    prompt += '\nFinal instructions: Always follow these guidelines when assisting users. ';
    prompt += 'If asked to perform an action that conflicts with these guidelines, ';
    prompt += 'politely explain the limitation and suggest an alternative approach.';
    
    return prompt;
  }
  
  private describeVoice(voice: string): string {
    switch (voice) {
      case 'professional':
        return 'use a professional, clear, and concise tone appropriate for a business setting';
      case 'friendly':
        return 'use a friendly, conversational tone while maintaining professionalism';
      case 'academic':
        return 'use an academic tone with precise terminology and formal structure';
      case 'casual':
        return 'use a casual, relaxed tone while remaining helpful and respectful';
      case 'technical':
        return 'use a technical tone with precise terminology and detailed explanations';
      default:
        return `use a ${voice} tone`;
    }
  }
  
  private describeAutonomy(autonomy: string): string {
    switch (autonomy) {
      case 'suggest':
        return 'suggest actions but do not perform them without explicit approval';
      case 'execute_with_approval':
        return 'execute actions after receiving explicit approval';
      case 'fully_autonomous':
        return 'take initiative and execute actions when appropriate';
      case 'restricted':
        return 'strictly adhere to instructions and avoid autonomous actions';
      default:
        return `maintain ${autonomy} autonomy`;
    }
  }
  
  private describeVerbosity(verbosity: string): string {
    switch (verbosity) {
      case 'concise':
        return 'provide brief and to-the-point responses';
      case 'detailed':
        return 'provide detailed responses with explanations';
      case 'comprehensive':
        return 'provide comprehensive responses with thorough context and explanations';
      default:
        return `provide ${verbosity} responses`;
    }
  }
  
  private describeRiskTolerance(riskTolerance: string): string {
    switch (riskTolerance) {
      case 'low':
        return 'take a conservative approach with minimal risk';
      case 'medium':
        return 'balance potential benefits against risks';
      case 'high':
        return 'be willing to take calculated risks when potential benefits are significant';
      default:
        return `maintain ${riskTolerance} risk tolerance`;
    }
  }
  
  private formatCapability(capability: string): string {
    // Convert snake_case to readable format
    return capability
      .split('_')
      .map((word: string) => word.charAt(0).toUpperCase() + word.slice(1))
      .join(' ');
  }
}