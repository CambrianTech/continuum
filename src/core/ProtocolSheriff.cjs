/**
 * Protocol Sheriff - Uses Claude Haiku to validate LLM inputs/outputs
 * Guards against command leakage, protocol violations, and user confusion
 */

const { Anthropic } = require('@anthropic-ai/sdk');

class ProtocolSheriff {
  constructor(modelRegistry = null, modelCaliber = null) {
    this.modelRegistry = modelRegistry;
    this.modelCaliber = modelCaliber;
    this.validationCache = new Map(); // Cache decisions to avoid re-validating same patterns
  }

  /**
   * Validate outgoing response before sending to user
   * Returns: { isValid: boolean, correctedResponse?: string, violations: string[] }
   */
  async validateResponse(aiResponse, userQuery, agentRole) {
    if (!this.modelCaliber || !this.modelCaliber.isCaliberAvailable('fast', this.modelRegistry)) {
      console.log('⚠️ Protocol Sheriff: No fast AI caliber available, skipping validation');
      return { isValid: true, violations: [] };
    }

    console.log('🤖 Protocol Sheriff: Validating response...');
    
    // Handle non-string responses (like bus command results)
    if (typeof aiResponse !== 'string') {
      console.log('🤖 Protocol Sheriff: Non-string response (bus command result), skipping validation');
      return { isValid: true, violations: [] };
    }
    
    // Handle non-string queries
    if (typeof userQuery !== 'string') {
      console.log('🤖 Protocol Sheriff: Non-string userQuery, skipping validation');
      return { isValid: true, violations: [] };
    }
    
    // Quick cache check for exact matches
    const cacheKey = `${aiResponse.substring(0, 100)}:${userQuery.substring(0, 50)}`;
    if (this.validationCache.has(cacheKey)) {
      return this.validationCache.get(cacheKey);
    }

    try {
      const model = this.modelCaliber.getModelForCaliber('fast', this.modelRegistry);
      const apiClient = this.modelRegistry.getAPIClient(model.provider);
      
      // If no API client available, fail open
      if (!apiClient || !apiClient.messages) {
        console.warn('⚠️ Protocol Sheriff: No API client available, failing open');
        return { isValid: true, violations: ['no_api_client'] };
      }
      
      const validation = await apiClient.messages.create({
        model: model.name,
        max_tokens: 500,
        messages: [{
          role: "user", 
          content: this.buildValidationPrompt(aiResponse, userQuery, agentRole)
        }]
      });

      // Handle different response structures
      let validationText;
      if (validation.content && validation.content[0] && validation.content[0].text) {
        validationText = validation.content[0].text;
      } else if (validation.text) {
        validationText = validation.text;
      } else if (typeof validation === 'string') {
        validationText = validation;
      } else {
        console.warn('❌ Unexpected validation response structure:', validation);
        return { isValid: true, violations: ['unexpected_response_structure'] };
      }
      
      const result = this.parseValidationResponse(validationText);
      
      // Cache the result
      this.validationCache.set(cacheKey, result);
      
      console.log(`🚨 Protocol Sheriff: ${result.isValid ? 'VALID' : 'VIOLATION DETECTED'}`);
      if (!result.isValid) {
        console.log(`🚨 Violations: ${result.violations.join(', ')}`);
      }
      
      return result;
    } catch (error) {
      console.error('❌ Protocol Sheriff validation failed:', error.message);
      return { isValid: true, violations: ['validation_failed'] }; // Fail open
    }
  }

  buildValidationPrompt(aiResponse, userQuery, agentRole) {
    return `You are a Protocol Sheriff. Your job is to validate AI responses before they reach users.

USER QUERY: "${userQuery}"
AGENT ROLE: ${agentRole}
AI RESPONSE TO VALIDATE:
---
${aiResponse}
---

VALIDATION RULES:
1. Commands (GIT_STATUS, FILE_READ, etc.) should ONLY appear in proper [CMD:COMMAND] format, never as conversational text
2. Response should be conversational and helpful, not technical jargon about "checking repository context"
3. AI should not assume complex tasks from simple queries (e.g., "testing" ≠ "I need testing help")
4. Tool execution details should be hidden from user unless explicitly requested
5. Response should directly address the user's actual query

VIOLATIONS TO CHECK FOR:
- Command names mentioned conversationally (not in [CMD:] format)
- Overly technical/robotic language for simple queries
- Assuming complex intent from simple words
- Exposing internal tool execution to user
- Not actually answering the user's question

Respond with:
VALID: true/false
VIOLATIONS: [list any violations found]
CORRECTION: [if invalid, provide a better response]

Be strict but fair. Simple queries deserve simple responses.`;
  }

  parseValidationResponse(response) {
    const lines = response.split('\n');
    let isValid = true;
    let violations = [];
    let correctedResponse = null;

    for (const line of lines) {
      const trimmed = line.trim();
      
      if (trimmed.startsWith('VALID:')) {
        isValid = trimmed.toLowerCase().includes('true');
      }
      
      if (trimmed.startsWith('VIOLATIONS:')) {
        const violationText = trimmed.replace('VIOLATIONS:', '').trim();
        if (violationText !== '[]' && violationText !== 'none') {
          violations = [violationText];
        }
      }
      
      if (trimmed.startsWith('CORRECTION:')) {
        correctedResponse = trimmed.replace('CORRECTION:', '').trim();
      }
    }

    return { isValid, violations, correctedResponse };
  }

  /**
   * Validate incoming user query for potential issues
   * Returns: { isValid: boolean, sanitizedQuery?: string, warnings: string[] }
   */
  async validateUserQuery(userQuery) {
    if (!this.anthropic) {
      return { isValid: true, warnings: [] };
    }

    // Quick checks for obvious issues
    const warnings = [];
    
    // Check for potential command injection attempts
    if (userQuery.includes('[CMD:') || userQuery.includes('[STATUS]')) {
      warnings.push('user_attempting_command_injection');
    }
    
    // Check for excessively long queries (potential prompt injection)
    if (userQuery.length > 2000) {
      warnings.push('excessively_long_query');
    }

    return { isValid: warnings.length === 0, warnings, sanitizedQuery: userQuery };
  }

  /**
   * Validate that tool execution results are properly formatted
   */
  validateToolResults(toolResults) {
    const issues = [];
    
    for (const result of toolResults) {
      if (!result.tool || !result.result) {
        issues.push('missing_required_fields');
      }
      
      // Check for sensitive data exposure
      if (result.result.includes('api_key') || result.result.includes('password')) {
        issues.push('potential_sensitive_data_exposure');
      }
    }
    
    return {
      isValid: issues.length === 0,
      issues
    };
  }

  /**
   * Get validation statistics
   */
  getStats() {
    return {
      cacheSize: this.validationCache.size,
      hasAPI: !!this.anthropic
    };
  }

  /**
   * Clear validation cache
   */
  clearCache() {
    this.validationCache.clear();
    console.log('🧹 Protocol Sheriff: Cache cleared');
  }
}

module.exports = ProtocolSheriff;