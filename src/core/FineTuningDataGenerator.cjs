/**
 * Fine-Tuning Data Generator
 * Creates training data for fine-tuning models on protocol validation
 */

class FineTuningDataGenerator {
  constructor() {
    this.trainingData = [];
    this.validationData = [];
  }

  /**
   * Generate fine-tuning data in OpenAI's format
   * For training a model specifically on protocol validation
   */
  generateProtocolValidationData() {
    const examples = [
      // POSITIVE examples (good responses)
      {
        messages: [
          {
            role: "system",
            content: "You are a protocol validator. Determine if an AI response violates conversation protocols. Respond with VALID or INVALID and list violations."
          },
          {
            role: "user", 
            content: `USER_QUERY: "hello"
AI_RESPONSE: "Hello! How can I help you today?"
VALIDATE_RESPONSE:`
          },
          {
            role: "assistant",
            content: "VALID: true\\nVIOLATIONS: none\\nREASON: Simple, appropriate greeting response"
          }
        ]
      },
      {
        messages: [
          {
            role: "system",
            content: "You are a protocol validator. Determine if an AI response violates conversation protocols. Respond with VALID or INVALID and list violations."
          },
          {
            role: "user",
            content: `USER_QUERY: "run tests"
AI_RESPONSE: "I'll run the tests for you. [CMD:EXEC] npm test"
VALIDATE_RESPONSE:`
          },
          {
            role: "assistant", 
            content: "VALID: true\\nVIOLATIONS: none\\nREASON: Proper command format used with [CMD:EXEC]"
          }
        ]
      },

      // NEGATIVE examples (violations to catch)
      {
        messages: [
          {
            role: "system",
            content: "You are a protocol validator. Determine if an AI response violates conversation protocols. Respond with VALID or INVALID and list violations."
          },
          {
            role: "user",
            content: `USER_QUERY: "testing"
AI_RESPONSE: "Let me check GIT_STATUS first. The repository looks good! How can I help with testing?"
VALIDATE_RESPONSE:`
          },
          {
            role: "assistant",
            content: "VALID: false\\nVIOLATIONS: command_mentioned_conversationally\\nREASON: GIT_STATUS mentioned without proper [CMD:] format"
          }
        ]
      },
      {
        messages: [
          {
            role: "system", 
            content: "You are a protocol validator. Determine if an AI response violates conversation protocols. Respond with VALID or INVALID and list violations."
          },
          {
            role: "user",
            content: `USER_QUERY: "help"
AI_RESPONSE: "I can help with FILE_READ operations, WEBFETCH: https://example.com requests, and EXEC commands."
VALIDATE_RESPONSE:`
          },
          {
            role: "assistant",
            content: "VALID: false\\nVIOLATIONS: commands_mentioned_without_format\\nREASON: Commands mentioned conversationally instead of using [CMD:] format"
          }
        ]
      },
      {
        messages: [
          {
            role: "system",
            content: "You are a protocol validator. Determine if an AI response violates conversation protocols. Respond with VALID or INVALID and list violations."
          },
          {
            role: "user",
            content: `USER_QUERY: "testing"
AI_RESPONSE: "I'm GeneralAI, here to assist you with your testing needs. As a general-purpose AI agent in the Continuum system, I have access to a wide range of tools and capabilities to help you with your testing efforts."
VALIDATE_RESPONSE:`
          },
          {
            role: "assistant",
            content: "VALID: false\\nVIOLATIONS: overly_technical_for_simple_query\\nREASON: Complex response to simple greeting, assumes testing context"
          }
        ]
      }
    ];

    return examples;
  }

  /**
   * Generate training data from Testing Droid adversarial examples
   */
  generateFromAdversarialTests(adversarialTests, validationResults) {
    const trainingExamples = [];
    
    for (let i = 0; i < adversarialTests.length; i++) {
      const test = adversarialTests[i];
      const result = validationResults[i];
      
      const example = {
        messages: [
          {
            role: "system",
            content: "You are a protocol validator. Determine if an AI response violates conversation protocols. Respond with VALID or INVALID and list violations."
          },
          {
            role: "user",
            content: `USER_QUERY: "${test.userQuery}"
AI_RESPONSE: "${test.aiResponse}"
VALIDATE_RESPONSE:`
          },
          {
            role: "assistant",
            content: `VALID: ${result.isValid}\\nVIOLATIONS: ${result.violations.join(', ') || 'none'}\\nREASON: ${test.expectedViolation}`
          }
        ]
      };
      
      trainingExamples.push(example);
    }
    
    return trainingExamples;
  }

  /**
   * Generate data for conversation context understanding
   */
  generateContextualData() {
    return [
      {
        messages: [
          {
            role: "system",
            content: "Determine appropriate response level for user queries. Simple queries need simple responses."
          },
          {
            role: "user",
            content: `USER_QUERY: "hi"
APPROPRIATE_RESPONSE_LEVEL:`
          },
          {
            role: "assistant",
            content: "LEVEL: simple_greeting\\nEXAMPLE: Hello! How can I help you?\\nAVOID: Technical explanations, system details"
          }
        ]
      },
      {
        messages: [
          {
            role: "system", 
            content: "Determine appropriate response level for user queries. Simple queries need simple responses."
          },
          {
            role: "user",
            content: `USER_QUERY: "testing"
APPROPRIATE_RESPONSE_LEVEL:`
          },
          {
            role: "assistant",
            content: "LEVEL: acknowledge_without_assumption\\nEXAMPLE: Sure! What would you like to test?\\nAVOID: Assuming complex testing scenarios"
          }
        ]
      }
    ];
  }

  /**
   * Export training data in OpenAI fine-tuning format
   */
  exportForOpenAI(filename = 'protocol_validation_training.jsonl') {
    const protocolData = this.generateProtocolValidationData();
    const contextualData = this.generateContextualData();
    const allData = [...protocolData, ...contextualData];
    
    const jsonlContent = allData.map(example => JSON.stringify(example)).join('\\n');
    
    console.log(`📁 Generated ${allData.length} training examples for fine-tuning`);
    console.log(`💾 Saved to ${filename}`);
    console.log(`🔧 Use with: openai api fine_tuning.jobs.create -t ${filename} -m gpt-3.5-turbo`);
    
    return {
      content: jsonlContent,
      filename: filename,
      examples: allData.length
    };
  }

  /**
   * Export for local model training (like with Hugging Face)
   */
  exportForLocalTraining() {
    const data = this.generateProtocolValidationData();
    
    const formatted = data.map(example => ({
      input: example.messages[1].content,
      output: example.messages[2].content,
      task: 'protocol_validation'
    }));
    
    return {
      train: formatted.slice(0, Math.floor(formatted.length * 0.8)),
      validation: formatted.slice(Math.floor(formatted.length * 0.8))
    };
  }

  /**
   * Analyze training data quality
   */
  analyzeDataQuality(trainingData) {
    const analysis = {
      totalExamples: trainingData.length,
      positiveExamples: 0,
      negativeExamples: 0,
      averageInputLength: 0,
      averageOutputLength: 0,
      violationTypes: new Set()
    };
    
    let totalInputLen = 0;
    let totalOutputLen = 0;
    
    for (const example of trainingData) {
      const assistantMessage = example.messages[2].content;
      const isValid = assistantMessage.includes('VALID: true');
      
      if (isValid) {
        analysis.positiveExamples++;
      } else {
        analysis.negativeExamples++;
        
        // Extract violation types
        const violationMatch = assistantMessage.match(/VIOLATIONS: (.+)/);
        if (violationMatch && violationMatch[1] !== 'none') {
          analysis.violationTypes.add(violationMatch[1]);
        }
      }
      
      totalInputLen += example.messages[1].content.length;
      totalOutputLen += assistantMessage.length;
    }
    
    analysis.averageInputLength = Math.round(totalInputLen / trainingData.length);
    analysis.averageOutputLength = Math.round(totalOutputLen / trainingData.length);
    analysis.violationTypes = Array.from(analysis.violationTypes);
    
    return analysis;
  }

  /**
   * Generate synthetic training data with variations
   */
  generateSyntheticVariations(baseExamples, variationCount = 5) {
    const variations = [];
    
    for (const base of baseExamples) {
      // Add original
      variations.push(base);
      
      // Generate variations by modifying user queries and AI responses
      for (let i = 0; i < variationCount; i++) {
        const variation = JSON.parse(JSON.stringify(base)); // Deep clone
        
        // Modify the user query slightly
        const userContent = variation.messages[1].content;
        const modifiedContent = this.addQueryVariation(userContent);
        variation.messages[1].content = modifiedContent;
        
        variations.push(variation);
      }
    }
    
    return variations;
  }

  addQueryVariation(originalContent) {
    // Simple variations - could be made more sophisticated
    const variations = [
      content => content.replace('"testing"', '"test"'),
      content => content.replace('"hello"', '"hi"'),
      content => content.replace('"help"', '"assistance"'),
      content => content.replace('GIT_STATUS', 'git status'),
      content => content.replace('FILE_READ', 'file read')
    ];
    
    const randomVariation = variations[Math.floor(Math.random() * variations.length)];
    return randomVariation(originalContent);
  }

  getStats() {
    return {
      trainingExamples: this.trainingData.length,
      validationExamples: this.validationData.length
    };
  }
}

module.exports = FineTuningDataGenerator;