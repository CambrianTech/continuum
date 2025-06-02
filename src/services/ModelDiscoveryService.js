/**
 * Model Discovery Service
 * Handles dynamic discovery of available AI models
 */

const { Anthropic } = require('@anthropic-ai/sdk');
const { OpenAI } = require('openai');

class ModelDiscoveryService {
  constructor() {
    this.availableModels = { 
      anthropic: [], 
      openai: [] 
    };
  }

  async discover() {
    console.log('🔍 Discovering available AI models...');
    
    await Promise.all([
      this.discoverAnthropicModels(),
      this.discoverOpenAIModels()
    ]);
  }

  async discoverAnthropicModels() {
    if (!process.env.ANTHROPIC_API_KEY) return;
    
    try {
      // Anthropic doesn't have a public models endpoint yet
      this.availableModels.anthropic = [
        'claude-3-5-sonnet-20241022',
        'claude-3-5-haiku-20241022', 
        'claude-3-opus-20240229',
        'claude-3-sonnet-20240229',
        'claude-3-haiku-20240307'
      ];
      console.log(`✅ Anthropic models: ${this.availableModels.anthropic.length} found`);
    } catch (error) {
      console.log(`⚠️ Anthropic model discovery failed: ${error.message}`);
    }
  }

  async discoverOpenAIModels() {
    if (!process.env.OPENAI_API_KEY) return;
    
    try {
      const openai = new OpenAI({ apiKey: process.env.OPENAI_API_KEY });
      const models = await openai.models.list();
      this.availableModels.openai = models.data
        .filter(model => model.id.includes('gpt'))
        .map(model => model.id)
        .sort();
      console.log(`✅ OpenAI models: ${this.availableModels.openai.length} found`);
    } catch (error) {
      console.log(`⚠️ OpenAI model discovery failed: ${error.message}`);
    }
  }

  getBestModel(provider = 'anthropic') {
    const models = this.availableModels[provider] || [];
    
    if (models.length === 0) {
      return provider === 'anthropic' ? 'claude-3-5-sonnet-20241022' : 'gpt-4';
    }
    
    const preferences = {
      anthropic: ['claude-3-5-sonnet-20241022', 'claude-3-opus-20240229', 'claude-3-sonnet-20240229'],
      openai: ['gpt-4o', 'gpt-4-turbo', 'gpt-4', 'gpt-3.5-turbo']
    };
    
    for (const model of preferences[provider]) {
      if (models.includes(model)) return model;
    }
    
    return models[0];
  }

  getAvailableModels() {
    return this.availableModels;
  }
}

module.exports = ModelDiscoveryService;