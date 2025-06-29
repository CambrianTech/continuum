/**
 * Model Caliber System - AI Level Abstraction
 * Instead of specific model names, use caliber levels: fast, smart, premium
 */

class ModelCaliber {
  constructor() {
    this.calibers = new Map();
    this.setupCalibers();
  }

  setupCalibers() {
    // Define caliber levels with their characteristics
    this.calibers.set('fast', {
      level: 'fast',
      description: 'Quick responses, cost-effective',
      maxCost: 0.001, // Max cost per request
      maxLatency: 2000, // Max response time in ms
      capabilities: ['simple_tasks', 'validation', 'quick_analysis'],
      fallbacks: ['smart']
    });

    this.calibers.set('smart', {
      level: 'smart', 
      description: 'Balanced performance and capability',
      maxCost: 0.01,
      maxLatency: 5000,
      capabilities: ['reasoning', 'code_analysis', 'research', 'planning'],
      fallbacks: ['premium', 'fast']
    });

    this.calibers.set('premium', {
      level: 'premium',
      description: 'Highest capability, most expensive',
      maxCost: 0.1,
      maxLatency: 15000,
      capabilities: ['complex_reasoning', 'advanced_coding', 'deep_analysis'],
      fallbacks: ['smart']
    });
  }

  /**
   * Get best available model for a caliber level
   */
  getModelForCaliber(caliber, modelRegistry) {
    const caliberSpec = this.calibers.get(caliber);
    if (!caliberSpec) {
      throw new Error(`Unknown caliber: ${caliber}`);
    }

    // Try to find best model matching this caliber
    const availableModels = modelRegistry.getAvailableModels();
    
    for (const model of availableModels) {
      if (this.modelMatchesCaliber(model, caliberSpec)) {
        return model;
      }
    }

    // Try fallback calibers
    for (const fallbackCaliber of caliberSpec.fallbacks) {
      try {
        return this.getModelForCaliber(fallbackCaliber, modelRegistry);
      } catch (error) {
        continue; // Try next fallback
      }
    }

    throw new Error(`No models available for caliber: ${caliber}`);
  }

  modelMatchesCaliber(model, caliberSpec) {
    // Check if model fits within caliber constraints
    if (model.inputRate > caliberSpec.maxCost * 1000) return false; // Rough cost check
    
    // Check capabilities overlap
    const modelCapabilities = model.capabilities || [];
    const hasRequiredCapability = caliberSpec.capabilities.some(cap => 
      modelCapabilities.includes(cap) || this.inferCapability(model, cap)
    );
    
    return hasRequiredCapability;
  }

  inferCapability(model, capability) {
    // Infer capabilities from model characteristics
    const capabilityMap = {
      'simple_tasks': model.inputRate < 1.0, // Cheap models for simple tasks
      'validation': model.inputRate < 1.0,
      'quick_analysis': model.inputRate < 1.0,
      'reasoning': model.inputRate < 10.0,
      'code_analysis': model.capabilities?.includes('coding') || model.name.includes('code'),
      'research': model.contextWindow > 50000,
      'planning': model.inputRate < 10.0,
      'complex_reasoning': model.inputRate >= 10.0,
      'advanced_coding': model.capabilities?.includes('coding'),
      'deep_analysis': model.contextWindow > 100000
    };

    return capabilityMap[capability] || false;
  }

  /**
   * Get recommended caliber for a task
   */
  getCaliber(task, context = {}) {
    const taskLower = task.toLowerCase();
    
    // Protocol validation - always use fast
    if (context.purpose === 'validation' || context.purpose === 'protocol_check') {
      return 'fast';
    }

    // Simple queries
    if (task.length < 50 && !this.isComplexTask(taskLower)) {
      return 'fast';
    }

    // Complex analysis or coding
    if (this.isComplexTask(taskLower) || task.length > 1000) {
      return 'premium';
    }

    // Default to smart for most tasks
    return 'smart';
  }

  isComplexTask(taskLower) {
    const complexKeywords = [
      'analyze', 'debug', 'optimize', 'architect', 'design', 'strategy',
      'algorithm', 'performance', 'security', 'refactor', 'implement'
    ];
    
    return complexKeywords.some(keyword => taskLower.includes(keyword));
  }

  /**
   * Get caliber information
   */
  getCalibers() {
    return Array.from(this.calibers.keys());
  }

  getCaliber(caliber) {
    return this.calibers.get(caliber);
  }

  /**
   * Check if caliber is available
   */
  isCaliberAvailable(caliber, modelRegistry) {
    try {
      this.getModelForCaliber(caliber, modelRegistry);
      return true;
    } catch (error) {
      return false;
    }
  }

  /**
   * Get status of all calibers
   */
  getCaliberStatus(modelRegistry) {
    const status = {};
    
    for (const caliber of this.getCalibers()) {
      try {
        const model = this.getModelForCaliber(caliber, modelRegistry);
        status[caliber] = {
          available: true,
          model: model.name,
          provider: model.provider,
          cost: model.inputRate
        };
      } catch (error) {
        status[caliber] = {
          available: false,
          error: error.message
        };
      }
    }
    
    return status;
  }
}

module.exports = ModelCaliber;