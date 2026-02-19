/**
 * PricingFetcher - Fetch live pricing from provider APIs
 *
 * Adapters can use this to get accurate, up-to-date pricing
 */

import { ModelPricing } from './PricingManager';

export class PricingFetcher {
  /**
   * Fetch pricing from OpenRouter API (they expose it publicly!)
   *
   * OpenRouter aggregates models from many providers and exposes pricing
   */
  static async fetchFromOpenRouter(): Promise<Map<string, ModelPricing>> {
    try {
      const response = await fetch('https://openrouter.ai/api/v1/models');
      if (!response.ok) {
        throw new Error(`HTTP ${response.status}`);
      }

      const data = await response.json();
      const pricingMap = new Map<string, ModelPricing>();

      for (const model of data.data) {
        if (!model.pricing) continue;

        const inputPer1M = parseFloat(model.pricing.prompt) * 1_000_000;
        const outputPer1M = parseFloat(model.pricing.completion) * 1_000_000;

        pricingMap.set(model.id, {
          inputPer1M,
          outputPer1M,
          currency: 'USD',
          effectiveDate: new Date().toISOString().split('T')[0],
          notes: `Fetched from OpenRouter, model: ${model.name}`
        });
      }

      console.log(`✅ PricingFetcher: Fetched pricing for ${pricingMap.size} models from OpenRouter`);
      return pricingMap;
    } catch (error) {
      console.error('❌ PricingFetcher: Failed to fetch from OpenRouter:', error);
      return new Map();
    }
  }

  /**
   * Fetch pricing for specific OpenAI models (manual scraping/hardcoded until API available)
   *
   * OpenAI doesn't expose pricing via API, so we need to either:
   * 1. Hardcode from their pricing page
   * 2. Scrape their website
   * 3. Use OpenRouter as proxy (they track OpenAI pricing)
   */
  static async fetchOpenAIPricing(): Promise<Map<string, ModelPricing>> {
    // For now, return static OpenAI pricing
    // TODO: Implement web scraping from https://openai.com/pricing
    return new Map([
      ['gpt-4o', { inputPer1M: 2.50, outputPer1M: 10.00, currency: 'USD', effectiveDate: '2024-05-13', notes: 'Static pricing' }],
      ['gpt-4', { inputPer1M: 30.00, outputPer1M: 60.00, currency: 'USD', effectiveDate: '2023-03-14', notes: 'Static pricing' }],
      ['gpt-4-turbo', { inputPer1M: 10.00, outputPer1M: 30.00, currency: 'USD', effectiveDate: '2024-04-09', notes: 'Static pricing' }],
      ['gpt-3.5-turbo', { inputPer1M: 0.50, outputPer1M: 1.50, currency: 'USD', effectiveDate: '2023-11-06', notes: 'Static pricing' }]
    ]);
  }

  /**
   * Fetch pricing for Anthropic models (manual scraping/hardcoded)
   *
   * Anthropic doesn't expose pricing via API
   */
  static async fetchAnthropicPricing(): Promise<Map<string, ModelPricing>> {
    // Static Anthropic pricing
    // TODO: Implement web scraping from https://anthropic.com/pricing
    return new Map([
      ['claude-3-opus-20240229', { inputPer1M: 15.00, outputPer1M: 75.00, currency: 'USD', effectiveDate: '2024-03-04', notes: 'Static pricing' }],
      ['claude-3-sonnet-20240229', { inputPer1M: 3.00, outputPer1M: 15.00, currency: 'USD', effectiveDate: '2024-03-04', notes: 'Static pricing' }],
      ['claude-3-5-sonnet-20241022', { inputPer1M: 3.00, outputPer1M: 15.00, currency: 'USD', effectiveDate: '2024-10-22', notes: 'Static pricing' }],
      ['claude-3-haiku-20240307', { inputPer1M: 0.25, outputPer1M: 1.25, currency: 'USD', effectiveDate: '2024-03-07', notes: 'Static pricing' }]
    ]);
  }

  /**
   * Fetch pricing for DeepSeek models
   */
  static async fetchDeepSeekPricing(): Promise<Map<string, ModelPricing>> {
    // Static DeepSeek pricing
    // TODO: Check if DeepSeek API exposes pricing
    return new Map([
      ['deepseek-chat', { inputPer1M: 0.27, outputPer1M: 1.10, currency: 'USD', effectiveDate: '2024-01-01', notes: 'Cache miss pricing' }],
      ['deepseek-reasoner', { inputPer1M: 0.55, outputPer1M: 2.19, currency: 'USD', effectiveDate: '2025-01-20', notes: 'R1 pricing' }]
    ]);
  }

  /**
   * Map OpenRouter model IDs to our internal provider/model names
   *
   * Example: "openai/gpt-4o" -> { provider: "openai", model: "gpt-4o" }
   */
  static parseOpenRouterModelId(openRouterModelId: string): { provider: string; model: string } | null {
    const parts = openRouterModelId.split('/');
    if (parts.length !== 2) return null;

    return {
      provider: parts[0],
      model: parts[1]
    };
  }
}
