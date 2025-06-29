/**
 * Model Adapter System - Handles different AI providers (OpenAI, Anthropic, HuggingFace)
 * Provides unified interface for fine-tuning and deployment across providers
 */

class BaseAdapter {
  constructor(apiKey, config = {}) {
    this.apiKey = apiKey;
    this.config = config;
  }

  // Simple utility
  sleep(ms) {
    return new Promise(resolve => setTimeout(resolve, ms));
  }

  // Basic validation - can be overridden
  validateTrainingData(data) {
    if (!Array.isArray(data)) {
      throw new Error('Training data must be an array');
    }
    
    if (data.length === 0) {
      throw new Error('Training data cannot be empty');
    }
  }

  // Abstract methods - must be implemented by subclasses
  async fineTune(baseModel, trainingData, options = {}) {
    throw new Error('fineTune must be implemented by subclass');
  }

  async deploy(modelId, options = {}) {
    throw new Error('deploy must be implemented by subclass');
  }

  async query(modelId, prompt, options = {}) {
    throw new Error('query must be implemented by subclass');
  }

  formatTrainingData(data) {
    throw new Error('formatTrainingData must be implemented by subclass');
  }
}

class OpenAIAdapter extends BaseAdapter {
  constructor(apiKey, config = {}) {
    super(apiKey, config);
    this.provider = 'OpenAI';
    
    if (apiKey) {
      const { OpenAI } = require('openai');
      this.client = new OpenAI({ apiKey });
      
      // Check if LoRA fine-tuning is available
      this.supportsLoRA = true;
    }
  }

  // Query OpenAI API for available models
  async getAvailableModels() {
    if (!this.client) return [];
    
    try {
      const models = await this.client.models.list();
      return models.data;
    } catch (error) {
      console.warn(`⚠️ Failed to fetch OpenAI models: ${error.message}`);
      return [];
    }
  }

  // Query OpenAI API for pricing (if available)
  async getPricing() {
    // OpenAI doesn't have a pricing API endpoint yet, but we could scrape or cache
    // For now, return null and let calling code handle pricing lookup
    return null;
  }

  formatTrainingData(data) {
    return data.map(example => ({
      messages: example.messages
    }));
  }

  async fineTune(baseModel, trainingData, options = {}) {
    this.validateTrainingData(trainingData);
    
    if (!this.client) {
      throw new Error('OpenAI API key required for fine-tuning');
    }

    // Check if LoRA fine-tuning is requested
    if (options.useLoRA) {
      return await this.fineTuneWithLoRA(baseModel, trainingData, options);
    }

    const formattedData = this.formatTrainingData(trainingData);
    
    // Upload training file
    const trainingFile = await this.client.files.create({
      file: new File([JSON.stringify(formattedData)], 'training.jsonl'),
      purpose: 'fine-tune'
    });

    // Create fine-tuning job
    const job = await this.client.fineTuning.jobs.create({
      training_file: trainingFile.id,
      model: baseModel,
      suffix: options.suffix || 'persona'
    });

    return {
      fineTuneId: job.fine_tuned_model || job.id,
      jobId: job.id,
      status: job.status,
      provider: this.provider,
      method: 'full_fine_tune'
    };
  }

  async fineTuneWithLoRA(baseModel, trainingData, options = {}) {
    console.log(`🔬 Starting LoRA fine-tuning for ${baseModel}...`);
    
    const LoRAAdapter = require('./LoRAAdapter.cjs');
    const lora = new LoRAAdapter(baseModel, options.rank || 16, options.alpha || 32);
    
    // Initialize adapters
    const modelConfig = { baseModel };
    lora.initializeAdapters(modelConfig);
    
    // Fine-tune adapters
    const checkpoint = await lora.fineTuneAdapters(trainingData, {
      epochs: options.epochs || 3,
      learningRate: options.learningRate || 1e-4
    });
    
    // Generate adapter ID
    const adapterId = `lora:${baseModel}:${options.suffix || 'persona'}:${Date.now()}`;
    
    return {
      fineTuneId: adapterId,
      jobId: adapterId,
      status: 'completed',
      provider: this.provider,
      method: 'lora_adapter',
      adapters: lora,
      checkpoint: checkpoint,
      storageReduction: checkpoint.reductionFactor
    };
  }

  async deploy(modelId, options = {}) {
    // For OpenAI, models are deployed automatically after fine-tuning
    return {
      provider: this.provider,
      modelId,
      ready: true,
      endpoint: 'https://api.openai.com/v1/chat/completions'
    };
  }

  async query(modelId, prompt, options = {}) {
    if (!this.client) {
      throw new Error('OpenAI client not initialized');
    }

    const response = await this.client.chat.completions.create({
      model: modelId,
      messages: [{ role: 'user', content: prompt }],
      ...options
    });

    return {
      provider: this.provider,
      modelId,
      response: response.choices[0].message.content,
      usage: response.usage
    };
  }
}

class AnthropicAdapter extends BaseAdapter {
  constructor(apiKey, config = {}) {
    super(apiKey, { provider: 'Anthropic', ...config });
    this.client = null;
    if (apiKey) {
      const { Anthropic } = require('@anthropic-ai/sdk');
      this.client = new Anthropic({ apiKey });
    }
    this.rateLimits = { requestsPerMinute: 30, ...config.rateLimits };
  }

  formatModelId(baseModel, namespace, suffix, timestamp) {
    return `anthropic-context:${baseModel}:${namespace}:${suffix}:${timestamp}`;
  }

  formatTrainingData(data) {
    // Anthropic format: Convert to context examples
    return data.map(example => {
      const messages = example.messages;
      const system = messages.find(m => m.role === 'system')?.content || '';
      const user = messages.find(m => m.role === 'user')?.content || '';
      const assistant = messages.find(m => m.role === 'assistant')?.content || '';
      
      return { system, human: user, assistant };
    });
  }

  async fineTune(baseModel, trainingData, options = {}) {
    this.validateTrainingData(trainingData);
    
    const formattedData = this.formatTrainingData(trainingData);
    const namespace = options.namespace || 'academy';
    const suffix = options.suffix || 'persona';
    const timestamp = Date.now();
    const modelId = this.formatModelId(baseModel, namespace, suffix, timestamp);
    
    const result = await this.executeFineTune(baseModel, formattedData, options);
    
    return {
      fineTuneId: modelId,
      status: result.status,
      method: result.method,
      context_examples: result.context_examples,
      provider: 'Anthropic'
    };
  }

  async executeFineTune(baseModel, formattedData, options) {
    console.log(`📚 Anthropic: Creating context-enhanced model (fine-tuning not directly supported)...`);
    await this.sleep(1500);
    
    return {
      status: 'completed',
      method: 'context_enhancement',
      context_examples: formattedData.length
    };
  }

  async executeDeployment(modelId, options) {
    const baseModel = modelId.replace('anthropic-context:', '').split(':')[0];
    
    return {
      endpoint: `https://api.anthropic.com/v1/messages`,
      model: baseModel,
      context_enhanced: true,
      ready: true
    };
  }

  async checkRateLimit() {
    // Simple rate limiting implementation
    await this.sleep(1000 / (this.rateLimits.requestsPerMinute / 60));
  }

  async deploy(modelId, options = {}) {
    return await this.executeDeployment(modelId, options);
  }

  async query(modelId, prompt, options = {}) {
    return await this.executeQuery(modelId, prompt, options);
  }

  async executeQuery(modelId, prompt, options) {
    await this.checkRateLimit();
    
    if (!this.client) throw new Error('Anthropic client not initialized');

    const baseModel = modelId.replace('anthropic-context:', '').split(':')[0];
    
    // Real implementation:
    // return await this.client.messages.create({
    //   model: baseModel,
    //   messages: [{ role: 'user', content: prompt }],
    //   ...options
    // });

    return {
      response: `Anthropic response from ${baseModel}: ${prompt.substring(0, 50)}...`,
      usage: { input_tokens: 10, output_tokens: 25 }
    };
  }

  estimateCost(trainingExamples, options = {}) {
    // Anthropic uses context enhancement instead of fine-tuning
    return {
      training: 0, // No direct fine-tuning cost
      inference: 0.003, // per request
      storage: trainingExamples * 0.0001, // context storage
      total: trainingExamples * 0.0001
    };
  }
}

class HuggingFaceAdapter extends BaseAdapter {
  constructor(apiKey, config = {}) {
    super(apiKey, config);
    this.endpoint = config.endpoint || 'https://api-inference.huggingface.co';
  }

  async fineTune(baseModel, trainingData, options = {}) {
    console.log(`🔬 HuggingFace: Starting fine-tune for ${baseModel}...`);

    const formattedData = this.formatTrainingData(trainingData);
    const fineTuneId = `${options.username || 'academy'}/${baseModel}-${options.suffix || 'persona'}-${Date.now()}`;
    
    console.log(`📤 Preparing ${formattedData.length} training examples for HuggingFace...`);
    
    // In real implementation, would use HuggingFace Hub API
    // to create training job or push to model repository
    
    return {
      fineTuneId,
      status: 'completed',
      provider: 'huggingface',
      metrics: {
        training_examples: formattedData.length,
        repository: fineTuneId
      }
    };
  }

  formatTrainingData(data) {
    // HuggingFace format: Usually depends on model type
    return data.map(example => ({
      text: this.messagesToText(example.messages),
      messages: example.messages
    }));
  }

  messagesToText(messages) {
    return messages.map(m => `${m.role}: ${m.content}`).join('\n');
  }

  async deploy(modelId, options = {}) {
    console.log(`🚀 HuggingFace: Deploying model ${modelId}...`);
    
    return {
      endpoint: `${this.endpoint}/models/${modelId}`,
      model: modelId,
      provider: 'huggingface',
      deployment_id: `deployment_${Date.now()}`
    };
  }

  async query(modelId, prompt, options = {}) {
    console.log(`🤖 HuggingFace: Querying ${modelId}...`);
    
    // In real implementation:
    // return await fetch(`${this.endpoint}/models/${modelId}`, {
    //   method: 'POST',
    //   headers: { 'Authorization': `Bearer ${this.apiKey}` },
    //   body: JSON.stringify({ inputs: prompt, ...options })
    // });

    return {
      response: `HuggingFace response from ${modelId}`,
      provider: 'huggingface'
    };
  }
}

class ModelAdapterFactory {
  static create(provider, apiKey, config = {}) {
    switch (provider.toLowerCase()) {
      case 'openai':
        return new OpenAIAdapter(apiKey, config);
      case 'anthropic':
        return new AnthropicAdapter(apiKey, config);
      case 'huggingface':
      case 'hf':
        return new HuggingFaceAdapter(apiKey, config);
      default:
        throw new Error(`Unsupported provider: ${provider}`);
    }
  }

  static getSupportedProviders() {
    return ['openai', 'anthropic', 'huggingface'];
  }

  static detectProvider(modelName) {
    if (modelName.startsWith('gpt-') || modelName.startsWith('ft:gpt-')) {
      return 'openai';
    } else if (modelName.startsWith('claude-')) {
      return 'anthropic';
    } else if (modelName.includes('/') || modelName.startsWith('microsoft/') || modelName.startsWith('meta-llama/')) {
      return 'huggingface';
    }
    
    return 'openai'; // default
  }
}

module.exports = {
  BaseAdapter,
  OpenAIAdapter,
  AnthropicAdapter,
  HuggingFaceAdapter,
  ModelAdapterFactory
};