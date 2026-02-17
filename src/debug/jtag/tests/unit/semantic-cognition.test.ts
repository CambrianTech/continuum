/**
 * Semantic Cognition Unit Tests
 *
 * Tests for the semantic cognition infrastructure:
 * - IEmbeddable interface utilities
 * - ModelContextWindows configuration
 * - RAGBudgetManager allocation
 */

import { describe, it, expect, beforeEach } from 'vitest';
import { isEmbeddable, needsEmbedding, type IEmbeddable } from '../../system/data/interfaces/IEmbeddable';
import {
  getContextWindow,
  getInferenceSpeed,
  isLargeContextModel,
  isSlowLocalModel,
  getRecommendedMaxOutputTokens,
  MODEL_CONTEXT_WINDOWS,
  DEFAULT_CONTEXT_WINDOW
} from '../../system/shared/ModelContextWindows';
import { ModelRegistry } from '../../system/shared/ModelRegistry';
import {
  QuantFormat,
  WeightFormat,
  AdapterMethod,
  AdapterTarget,
  InferenceRuntime,
  Accelerator,
  isFineTunable,
  supportsLoRA,
  supportsAdapterStacking,
  estimateAdapterVramMB,
  fitsInVram,
  type ModelAdapterProfile
} from '../../system/shared/ModelCapabilities';
import {
  RAGBudgetManager,
  allocateChatBudget,
  type RAGSourceBudget
} from '../../system/rag/shared/RAGBudgetManager';
import { buildLoRATrainingPipeline, type LoRATrainingConfig } from '../../system/sentinel/pipelines/LoRATrainingPipeline';
import { AdapterPackage } from '../../system/genome/server/AdapterPackage';
import type { AdapterPackageManifest } from '../../system/genome/shared/AdapterPackageTypes';
import { GenomeLayerEntity } from '../../system/genome/entities/GenomeLayerEntity';
import type { UUID } from '../../system/core/types/CrossPlatformUUID';
import * as fs from 'fs';
import * as path from 'path';
import * as os from 'os';

describe('IEmbeddable', () => {
  describe('isEmbeddable', () => {
    it('should return true for objects with getEmbeddableContent method', () => {
      const embeddable = {
        getEmbeddableContent: () => 'test content'
      };
      expect(isEmbeddable(embeddable)).toBe(true);
    });

    it('should return false for objects without getEmbeddableContent', () => {
      expect(isEmbeddable({})).toBe(false);
      expect(isEmbeddable({ content: 'test' })).toBe(false);
    });

    it('should return false for null and undefined', () => {
      expect(isEmbeddable(null)).toBe(false);
      expect(isEmbeddable(undefined)).toBe(false);
    });

    it('should return false for primitives', () => {
      expect(isEmbeddable('string')).toBe(false);
      expect(isEmbeddable(123)).toBe(false);
    });
  });

  describe('needsEmbedding', () => {
    it('should return true for entity without embedding', () => {
      const entity: IEmbeddable = {
        getEmbeddableContent: () => 'test'
      };
      expect(needsEmbedding(entity)).toBe(true);
    });

    it('should return true for entity with empty embedding', () => {
      const entity: IEmbeddable = {
        getEmbeddableContent: () => 'test',
        embedding: []
      };
      expect(needsEmbedding(entity)).toBe(true);
    });

    it('should return false for entity with valid embedding', () => {
      const entity: IEmbeddable = {
        getEmbeddableContent: () => 'test',
        embedding: [0.1, 0.2, 0.3],
        embeddedAt: new Date().toISOString() as any
      };
      expect(needsEmbedding(entity)).toBe(false);
    });

    it('should return true for stale embedding', () => {
      const staleDate = new Date(Date.now() - 10 * 24 * 60 * 60 * 1000); // 10 days ago
      const entity: IEmbeddable = {
        getEmbeddableContent: () => 'test',
        embedding: [0.1, 0.2, 0.3],
        embeddedAt: staleDate.toISOString() as any
      };
      expect(needsEmbedding(entity, 7 * 24 * 60 * 60 * 1000)).toBe(true); // 7 day max age
    });
  });
});

describe('ModelContextWindows', () => {
  describe('getContextWindow', () => {
    it('should return correct context window for known models', () => {
      expect(getContextWindow('gpt-4')).toBe(8192);
      expect(getContextWindow('gpt-4o')).toBe(128000);
      expect(getContextWindow('claude-3-opus')).toBe(200000);
      expect(getContextWindow('llama3.2:3b')).toBe(128000);
    });

    it('should return default for unknown models', () => {
      expect(getContextWindow('unknown-model')).toBe(DEFAULT_CONTEXT_WINDOW);
    });

    it('should handle versioned model names', () => {
      // Base model lookup when version not in table
      expect(getContextWindow('llama3.2:7b')).toBe(128000); // Should find llama3.2
    });

    it('should have reasonable values for all models', () => {
      for (const [model, size] of Object.entries(MODEL_CONTEXT_WINDOWS)) {
        expect(size).toBeGreaterThan(0);
        expect(size).toBeLessThanOrEqual(1100000); // Max ~1M tokens (Gemini 2.0 Flash is 1048576)
      }
    });
  });

  describe('isLargeContextModel', () => {
    it('should return true for large context models', () => {
      expect(isLargeContextModel('gpt-4o')).toBe(true); // 128K
      expect(isLargeContextModel('claude-3-opus')).toBe(true); // 200K
    });

    it('should return false for small context models', () => {
      expect(isLargeContextModel('gpt-4')).toBe(false); // 8K
    });
  });

  describe('getRecommendedMaxOutputTokens', () => {
    it('should return reasonable output tokens for small models', () => {
      const tokens = getRecommendedMaxOutputTokens('gpt-4');
      expect(tokens).toBeGreaterThan(0);
      expect(tokens).toBeLessThanOrEqual(8192 * 0.25); // Max 25% of context
    });

    it('should cap output tokens for large models', () => {
      const tokens = getRecommendedMaxOutputTokens('claude-3-opus');
      expect(tokens).toBeLessThanOrEqual(4096); // Capped at 4K
    });
  });
});

describe('ModelRegistry - Provider-Scoped Lookups', () => {
  let registry: ModelRegistry;

  beforeEach(() => {
    registry = ModelRegistry.sharedInstance();
    registry.clear();
  });

  it('should store same model under different providers without collision', () => {
    registry.register({
      modelId: 'meta-llama/Llama-3.1-8B-Instruct',
      contextWindow: 1400,
      provider: 'candle',
      discoveredAt: Date.now()
    });
    registry.register({
      modelId: 'meta-llama/Llama-3.1-8B-Instruct',
      contextWindow: 131072,
      provider: 'together',
      discoveredAt: Date.now()
    });

    // Scoped lookups return correct values
    expect(registry.contextWindow('meta-llama/Llama-3.1-8B-Instruct', 'candle')).toBe(1400);
    expect(registry.contextWindow('meta-llama/Llama-3.1-8B-Instruct', 'together')).toBe(131072);
  });

  it('should return largest context window for unscoped lookup with multiple providers', () => {
    registry.register({
      modelId: 'meta-llama/Llama-3.1-8B-Instruct',
      contextWindow: 1400,
      provider: 'candle',
      discoveredAt: Date.now()
    });
    registry.register({
      modelId: 'meta-llama/Llama-3.1-8B-Instruct',
      contextWindow: 131072,
      provider: 'together',
      discoveredAt: Date.now()
    });

    // Unscoped returns largest (cloud wins for backward compat)
    expect(registry.contextWindow('meta-llama/Llama-3.1-8B-Instruct')).toBe(131072);
  });

  it('should return single provider entry for unscoped lookup with one provider', () => {
    registry.register({
      modelId: 'claude-sonnet-4-5-20250929',
      contextWindow: 200000,
      provider: 'anthropic',
      discoveredAt: Date.now()
    });

    expect(registry.contextWindow('claude-sonnet-4-5-20250929')).toBe(200000);
  });

  it('should return undefined for unknown provider', () => {
    registry.register({
      modelId: 'gpt-4o',
      contextWindow: 128000,
      provider: 'openai',
      discoveredAt: Date.now()
    });

    expect(registry.contextWindow('gpt-4o', 'candle')).toBeUndefined();
  });

  it('getAll should return all providers for a model', () => {
    registry.register({
      modelId: 'meta-llama/Llama-3.1-8B-Instruct',
      contextWindow: 1400,
      provider: 'candle',
      discoveredAt: Date.now()
    });
    registry.register({
      modelId: 'meta-llama/Llama-3.1-8B-Instruct',
      contextWindow: 131072,
      provider: 'together',
      discoveredAt: Date.now()
    });

    const all = registry.getAll('meta-llama/Llama-3.1-8B-Instruct');
    expect(all.length).toBe(2);
    const providers = all.map(m => m.provider).sort();
    expect(providers).toEqual(['candle', 'together']);
  });

  it('should apply date-suffix normalization within provider scope', () => {
    registry.register({
      modelId: 'claude-sonnet-4-5',
      contextWindow: 200000,
      provider: 'anthropic',
      discoveredAt: Date.now()
    });

    // Date-suffix stripped lookup should find it
    expect(registry.contextWindow('claude-sonnet-4-5-20250929', 'anthropic')).toBe(200000);
  });

  it('discoveredCount should reflect provider-scoped entries', () => {
    registry.register({
      modelId: 'llama-8b',
      contextWindow: 1400,
      provider: 'candle',
      discoveredAt: Date.now()
    });
    registry.register({
      modelId: 'llama-8b',
      contextWindow: 131072,
      provider: 'together',
      discoveredAt: Date.now()
    });

    // Two entries (one per provider), not one
    expect(registry.discoveredCount).toBe(2);
  });
});

describe('Provider-Scoped ModelContextWindows', () => {
  beforeEach(() => {
    ModelRegistry.sharedInstance().clear();
  });

  it('getContextWindow should return provider-scoped value from registry', () => {
    const registry = ModelRegistry.sharedInstance();
    registry.register({
      modelId: 'meta-llama/Llama-3.1-8B-Instruct',
      contextWindow: 1400,
      provider: 'candle',
      discoveredAt: Date.now()
    });
    registry.register({
      modelId: 'meta-llama/Llama-3.1-8B-Instruct',
      contextWindow: 131072,
      provider: 'together',
      discoveredAt: Date.now()
    });

    expect(getContextWindow('meta-llama/Llama-3.1-8B-Instruct', 'candle')).toBe(1400);
    expect(getContextWindow('meta-llama/Llama-3.1-8B-Instruct', 'together')).toBe(131072);
  });

  it('getContextWindow should fall back to static map when provider not in registry', () => {
    // No registry entries — should use static map
    expect(getContextWindow('meta-llama/Llama-3.1-8B-Instruct', 'candle')).toBe(1400);
  });

  it('getInferenceSpeed should return local TPS for local provider in registry', () => {
    const registry = ModelRegistry.sharedInstance();
    registry.register({
      modelId: 'meta-llama/Llama-3.1-8B-Instruct',
      contextWindow: 1400,
      provider: 'candle',
      discoveredAt: Date.now()
    });

    // Bug fix verification: should return 40 TPS (static map), not 1000 TPS (cloud assumption)
    const speed = getInferenceSpeed('meta-llama/Llama-3.1-8B-Instruct', 'candle');
    expect(speed).toBe(40);  // From MODEL_INFERENCE_SPEEDS static map
  });

  it('getInferenceSpeed should return 1000 TPS for cloud provider in registry', () => {
    const registry = ModelRegistry.sharedInstance();
    registry.register({
      modelId: 'meta-llama/Llama-3.1-8B-Instruct',
      contextWindow: 131072,
      provider: 'together',
      discoveredAt: Date.now()
    });

    const speed = getInferenceSpeed('meta-llama/Llama-3.1-8B-Instruct', 'together');
    expect(speed).toBe(1000);  // Cloud API speed
  });

  it('isSlowLocalModel should be true for candle models', () => {
    const registry = ModelRegistry.sharedInstance();
    registry.register({
      modelId: 'meta-llama/Llama-3.1-8B-Instruct',
      contextWindow: 1400,
      provider: 'candle',
      discoveredAt: Date.now()
    });

    expect(isSlowLocalModel('meta-llama/Llama-3.1-8B-Instruct', 'candle')).toBe(true);
  });

  it('isSlowLocalModel should be false for cloud models', () => {
    const registry = ModelRegistry.sharedInstance();
    registry.register({
      modelId: 'meta-llama/Llama-3.1-8B-Instruct',
      contextWindow: 131072,
      provider: 'together',
      discoveredAt: Date.now()
    });

    expect(isSlowLocalModel('meta-llama/Llama-3.1-8B-Instruct', 'together')).toBe(false);
  });
});

describe('RAGBudgetManager', () => {
  describe('allocate', () => {
    it('should allocate minimums to all sources', () => {
      const manager = new RAGBudgetManager('gpt-4o');
      const sources: RAGSourceBudget[] = [
        { sourceId: 'conversation', priority: 10, minTokens: 1000, maxTokens: 5000 },
        { sourceId: 'memories', priority: 5, minTokens: 500, maxTokens: 2000 }
      ];

      const result = manager.allocate(sources, { system: 500, completion: 3000 });

      // All sources should get at least their minimum
      const convAlloc = result.allocations.find(a => a.sourceId === 'conversation');
      const memAlloc = result.allocations.find(a => a.sourceId === 'memories');

      expect(convAlloc?.allocatedTokens).toBeGreaterThanOrEqual(1000);
      expect(memAlloc?.allocatedTokens).toBeGreaterThanOrEqual(500);
    });

    it('should distribute extra tokens by priority', () => {
      const manager = new RAGBudgetManager('gpt-4'); // 8K context - smaller to see distribution
      const sources: RAGSourceBudget[] = [
        { sourceId: 'high', priority: 10, minTokens: 100, maxTokens: 10000 },
        { sourceId: 'low', priority: 1, minTokens: 100, maxTokens: 10000 }
      ];

      const result = manager.allocate(sources, { system: 500, completion: 1000 });

      const highAlloc = result.allocations.find(a => a.sourceId === 'high');
      const lowAlloc = result.allocations.find(a => a.sourceId === 'low');

      // Higher priority should get more tokens (10:1 ratio)
      // With 8192 - 500 - 1000 = 6692 available, after minimums (200), 6492 to distribute
      // High gets ~90% (10/11), low gets ~10% (1/11)
      expect(highAlloc!.allocatedTokens).toBeGreaterThan(lowAlloc!.allocatedTokens);
    });

    it('should respect maximum token limits', () => {
      const manager = new RAGBudgetManager('gpt-4o');
      const sources: RAGSourceBudget[] = [
        { sourceId: 'limited', priority: 10, minTokens: 100, maxTokens: 1000 }
      ];

      const result = manager.allocate(sources, { system: 500, completion: 3000 });

      const alloc = result.allocations.find(a => a.sourceId === 'limited');
      expect(alloc?.allocatedTokens).toBeLessThanOrEqual(1000);
    });

    it('should handle insufficient tokens gracefully', () => {
      const manager = new RAGBudgetManager('gpt-4'); // 8K context
      const sources: RAGSourceBudget[] = [
        { sourceId: 'big', priority: 10, minTokens: 5000, maxTokens: 10000 },
        { sourceId: 'bigger', priority: 10, minTokens: 5000, maxTokens: 10000 }
      ];

      // Reserve most of the context
      const result = manager.allocate(sources, { system: 500, completion: 3000 });

      // Should have warnings about insufficient tokens
      expect(result.warnings.length).toBeGreaterThan(0);
      // Should still allocate something proportionally
      expect(result.totalAllocated).toBeGreaterThan(0);
    });

    it('should calculate correct available tokens', () => {
      const manager = new RAGBudgetManager('gpt-4'); // 8192 context
      const sources: RAGSourceBudget[] = [
        { sourceId: 'test', priority: 10, minTokens: 0, maxTokens: 10000 }
      ];

      const result = manager.allocate(sources, { system: 500, completion: 3000 });

      // Available = 8192 - 500 - 3000 = 4692
      expect(result.availableForSources).toBe(4692);
    });
  });

  describe('getChatBudget', () => {
    it('should return budget with expected sources', () => {
      const budget = RAGBudgetManager.getChatBudget('gpt-4o');

      const sourceIds = budget.map(s => s.sourceId);
      expect(sourceIds).toContain('conversation');
      expect(sourceIds).toContain('memories');
    });

    it('should have higher min for large context models', () => {
      const largeBudget = RAGBudgetManager.getChatBudget('gpt-4o'); // 128K
      const smallBudget = RAGBudgetManager.getChatBudget('gpt-4'); // 8K

      const largeConv = largeBudget.find(s => s.sourceId === 'conversation');
      const smallConv = smallBudget.find(s => s.sourceId === 'conversation');

      expect(largeConv!.minTokens).toBeGreaterThan(smallConv!.minTokens);
    });
  });

  describe('allocateChatBudget', () => {
    it('should return valid allocation for any model', () => {
      const result = allocateChatBudget('gpt-4o');

      expect(result.modelId).toBe('gpt-4o');
      expect(result.allocations.length).toBeGreaterThan(0);
      expect(result.totalAllocated).toBeGreaterThan(0);
    });
  });
});

describe('ModelCapabilities — Adapter Profile Type System', () => {
  // Realistic profile: Llama 3.1 8B on Candle/M1 with QLoRA support
  const candleLlama8B: ModelAdapterProfile = {
    runtime: InferenceRuntime.CANDLE,
    quantization: {
      format: QuantFormat.Q4_K_M,
      bitsPerWeight: 4,
      weightFormat: WeightFormat.GGUF,
      canDequantizeForTraining: false,
      canTrainInQuantized: true,  // QLoRA
    },
    fineTuning: {
      supportedMethods: [AdapterMethod.QLORA],
      lora: {
        maxRank: 32,
        recommendedRank: 8,
        recommendedAlpha: 16,
        maxConcurrentAdapters: 3,
        supportsStacking: true,
        adapterSizeMB: 15,
        targetableLayers: [AdapterTarget.ATTN_Q, AdapterTarget.ATTN_V],
        recommendedTargets: [AdapterTarget.ATTN_Q, AdapterTarget.ATTN_V],
        recommendedDropout: 0.05,
      },
      maxTrainingBatchSize: 4,
      supportsGradientCheckpointing: true,
      supportsFlashAttention: false,
    },
    hardware: {
      inferenceVramMB: 4500,
      trainingVramMB: 8000,
      accelerator: Accelerator.METAL,
      measuredInputTPS: 40,
      measuredOutputTPS: 25,
      fitsInVram: true,
      cpuOffloadLayers: 0,
    },
    architectureFamily: 'llama',
    parameterCountB: 8,
    layerCount: 32,
    hiddenSize: 4096,
  };

  // Cloud API profile: no fine-tuning, no local execution
  const cloudLlama: ModelAdapterProfile = {
    runtime: InferenceRuntime.CLOUD_API,
    quantization: {
      format: QuantFormat.NONE,
      bitsPerWeight: 16,
      weightFormat: WeightFormat.CLOUD,
    },
    fineTuning: {
      supportedMethods: [],  // Cloud API — no adapter access
    },
    hardware: {
      inferenceVramMB: 0,  // Cloud-managed
      accelerator: Accelerator.CLOUD,
    },
  };

  // Full fine-tune only profile (no PEFT)
  const fullFinetuneOnly: ModelAdapterProfile = {
    runtime: InferenceRuntime.TRANSFORMERS,
    quantization: {
      format: QuantFormat.FP16,
      bitsPerWeight: 16,
      weightFormat: WeightFormat.SAFETENSORS,
    },
    fineTuning: {
      supportedMethods: [AdapterMethod.FULL],
    },
  };

  describe('isFineTunable', () => {
    it('should return true for QLoRA-capable model', () => {
      expect(isFineTunable(candleLlama8B)).toBe(true);
    });

    it('should return false for cloud API model', () => {
      expect(isFineTunable(cloudLlama)).toBe(false);
    });

    it('should return false for full-fine-tune-only model (not PEFT)', () => {
      // Full fine-tune is not parameter-efficient — isFineTunable checks for PEFT
      expect(isFineTunable(fullFinetuneOnly)).toBe(false);
    });

    it('should return false for undefined profile', () => {
      expect(isFineTunable(undefined)).toBe(false);
    });
  });

  describe('supportsLoRA', () => {
    it('should return true for QLoRA-capable model', () => {
      expect(supportsLoRA(candleLlama8B)).toBe(true);
    });

    it('should return false for cloud API model', () => {
      expect(supportsLoRA(cloudLlama)).toBe(false);
    });

    it('should return true for LoRA + QLoRA model', () => {
      const both: ModelAdapterProfile = {
        ...candleLlama8B,
        fineTuning: {
          supportedMethods: [AdapterMethod.LORA, AdapterMethod.QLORA],
          lora: candleLlama8B.fineTuning.lora,
        },
      };
      expect(supportsLoRA(both)).toBe(true);
    });
  });

  describe('supportsAdapterStacking', () => {
    it('should return true for multi-adapter model', () => {
      expect(supportsAdapterStacking(candleLlama8B)).toBe(true);
    });

    it('should return false for cloud model', () => {
      expect(supportsAdapterStacking(cloudLlama)).toBe(false);
    });

    it('should return false when maxConcurrentAdapters is 1', () => {
      const singleAdapter: ModelAdapterProfile = {
        ...candleLlama8B,
        fineTuning: {
          ...candleLlama8B.fineTuning,
          lora: {
            ...candleLlama8B.fineTuning.lora!,
            maxConcurrentAdapters: 1,
            supportsStacking: false,
          },
        },
      };
      expect(supportsAdapterStacking(singleAdapter)).toBe(false);
    });
  });

  describe('estimateAdapterVramMB', () => {
    it('should estimate reasonable VRAM for rank 8 on 8B model', () => {
      const vram = estimateAdapterVramMB(candleLlama8B);
      // 2 * 8 * 4096 * 2 targets * 32 layers * 2 bytes / 1MB ≈ 32 MB
      expect(vram).toBeGreaterThan(0);
      expect(vram).toBeLessThan(200);  // Should be well under 200MB for rank 8
    });

    it('should increase with rank', () => {
      const rank8 = estimateAdapterVramMB(candleLlama8B, 8);
      const rank32 = estimateAdapterVramMB(candleLlama8B, 32);
      expect(rank32).toBeGreaterThan(rank8);
      expect(rank32).toBe(rank8 * 4);  // Linear with rank
    });
  });

  describe('fitsInVram', () => {
    it('should return true when enough VRAM available', () => {
      expect(fitsInVram(candleLlama8B, 16000)).toBe(true);  // 16GB > 4.5GB
    });

    it('should return false when insufficient VRAM', () => {
      expect(fitsInVram(candleLlama8B, 2000)).toBe(false);  // 2GB < 4.5GB
    });

    it('should return false for undefined profile', () => {
      expect(fitsInVram(undefined, 16000)).toBe(false);
    });
  });

  describe('enum completeness', () => {
    it('should have all expected quantization formats', () => {
      expect(QuantFormat.Q4_K_M).toBe('q4_k_m');
      expect(QuantFormat.FP16).toBe('fp16');
      expect(QuantFormat.GPTQ).toBe('gptq');
      expect(QuantFormat.AWQ).toBe('awq');
    });

    it('should have all expected adapter methods', () => {
      expect(AdapterMethod.LORA).toBe('lora');
      expect(AdapterMethod.QLORA).toBe('qlora');
      expect(AdapterMethod.DORA).toBe('dora');
      expect(AdapterMethod.IA3).toBe('ia3');
    });

    it('should have all expected runtimes', () => {
      expect(InferenceRuntime.CANDLE).toBe('candle');
      expect(InferenceRuntime.LLAMA_CPP).toBe('llama_cpp');
      expect(InferenceRuntime.MLX).toBe('mlx');
      expect(InferenceRuntime.OLLAMA).toBe('ollama');
    });

    it('should have all expected accelerators', () => {
      expect(Accelerator.METAL).toBe('metal');
      expect(Accelerator.CUDA).toBe('cuda');
      expect(Accelerator.CPU).toBe('cpu');
    });
  });

  describe('ModelRegistry integration', () => {
    beforeEach(() => {
      ModelRegistry.sharedInstance().clear();
    });

    it('should store and retrieve adapterProfile via ModelMetadata', () => {
      const registry = ModelRegistry.sharedInstance();
      registry.register({
        modelId: 'meta-llama/Llama-3.1-8B-Instruct',
        contextWindow: 1400,
        provider: 'candle',
        discoveredAt: Date.now(),
        adapterProfile: candleLlama8B,
      });

      const metadata = registry.get('meta-llama/Llama-3.1-8B-Instruct', 'candle');
      expect(metadata?.adapterProfile).toBeDefined();
      expect(metadata?.adapterProfile?.runtime).toBe(InferenceRuntime.CANDLE);
      expect(supportsLoRA(metadata?.adapterProfile)).toBe(true);
      expect(metadata?.adapterProfile?.hardware?.accelerator).toBe(Accelerator.METAL);
    });

    it('should filter models by fine-tunability across providers', () => {
      const registry = ModelRegistry.sharedInstance();
      registry.register({
        modelId: 'meta-llama/Llama-3.1-8B-Instruct',
        contextWindow: 1400,
        provider: 'candle',
        discoveredAt: Date.now(),
        adapterProfile: candleLlama8B,
      });
      registry.register({
        modelId: 'meta-llama/Llama-3.1-8B-Instruct',
        contextWindow: 131072,
        provider: 'together',
        discoveredAt: Date.now(),
        adapterProfile: cloudLlama,
      });

      const all = registry.getAll('meta-llama/Llama-3.1-8B-Instruct');
      const fineTunable = all.filter(m => isFineTunable(m.adapterProfile));
      const loraCapable = all.filter(m => supportsLoRA(m.adapterProfile));

      expect(all.length).toBe(2);
      expect(fineTunable.length).toBe(1);
      expect(fineTunable[0].provider).toBe('candle');
      expect(loraCapable.length).toBe(1);
    });
  });
});

describe('LoRATrainingPipeline — Pipeline Template', () => {
  const testConfig: LoRATrainingConfig = {
    personaId: 'test-persona-id-1234' as UUID,
    personaName: 'Test AI',
    roomId: 'test-room-id-5678' as UUID,
  };

  it('should produce a valid pipeline with default config', () => {
    const pipeline = buildLoRATrainingPipeline(testConfig);

    expect(pipeline.name).toBe('lora-training-test-ai');
    expect(pipeline.steps).toBeDefined();
    expect(pipeline.steps.length).toBe(2); // dataset-prepare + condition
    expect(pipeline.inputs).toBeDefined();
    expect(pipeline.inputs!.personaId).toBe(testConfig.personaId);
    expect(pipeline.inputs!.personaName).toBe(testConfig.personaName);
  });

  it('should have dataset-prepare as step 0', () => {
    const pipeline = buildLoRATrainingPipeline(testConfig);
    const step0 = pipeline.steps[0];

    expect(step0.type).toBe('command');
    if (step0.type === 'command') {
      expect(step0.command).toBe('genome/dataset-prepare');
      expect(step0.params?.personaId).toBe(testConfig.personaId);
      expect(step0.params?.personaName).toBe(testConfig.personaName);
      expect(step0.params?.roomId).toBe(testConfig.roomId);
      expect(step0.params?.traitType).toBe('conversational');
    }
  });

  it('should have condition step checking step 0 success', () => {
    const pipeline = buildLoRATrainingPipeline(testConfig);
    const step1 = pipeline.steps[1];

    expect(step1.type).toBe('condition');
    if (step1.type === 'condition') {
      expect(step1.if).toBe('{{steps.0.data.success}}');
      expect(step1.then).toBeDefined();
      expect(step1.then.length).toBe(3); // train + register + activate
    }
  });

  it('should wire dataset path from step 0 to train step via interpolation', () => {
    const pipeline = buildLoRATrainingPipeline(testConfig);
    const conditionStep = pipeline.steps[1];

    if (conditionStep.type === 'condition') {
      const trainStep = conditionStep.then[0];
      expect(trainStep.type).toBe('command');
      if (trainStep.type === 'command') {
        expect(trainStep.command).toBe('genome/train');
        expect(trainStep.params?.datasetPath).toBe('{{steps.0.data.datasetPath}}');
      }
    }
  });

  it('should include register and activate steps in condition then branch', () => {
    const pipeline = buildLoRATrainingPipeline(testConfig);
    const conditionStep = pipeline.steps[1];

    if (conditionStep.type === 'condition') {
      const registerStep = conditionStep.then[1];
      const activateStep = conditionStep.then[2];

      expect(registerStep.type).toBe('command');
      expect(activateStep.type).toBe('command');

      if (registerStep.type === 'command') {
        expect(registerStep.command).toBe('genome/paging-adapter-register');
        expect(registerStep.params?.domain).toBe('conversational');
      }

      if (activateStep.type === 'command') {
        expect(activateStep.command).toBe('genome/paging-activate');
        expect(activateStep.params?.personaId).toBe(testConfig.personaId);
      }
    }
  });

  it('should wire layerId from train step to register step via interpolation', () => {
    const pipeline = buildLoRATrainingPipeline(testConfig);
    const conditionStep = pipeline.steps[1];

    if (conditionStep.type === 'condition') {
      const registerStep = conditionStep.then[1];
      if (registerStep.type === 'command') {
        expect(registerStep.params?.layerId).toBe('{{steps.1.0.data.layerId}}');
      }
    }
  });

  it('should respect custom config values', () => {
    const customConfig: LoRATrainingConfig = {
      ...testConfig,
      traitType: 'teaching',
      baseModel: 'llama3.2:1b',
      rank: 16,
      epochs: 5,
      learningRate: 0.00005,
      batchSize: 8,
    };

    const pipeline = buildLoRATrainingPipeline(customConfig);
    const conditionStep = pipeline.steps[1];

    if (conditionStep.type === 'condition') {
      const trainStep = conditionStep.then[0];
      if (trainStep.type === 'command') {
        expect(trainStep.params?.baseModel).toBe('llama3.2:1b');
        expect(trainStep.params?.rank).toBe(16);
        expect(trainStep.params?.epochs).toBe(5);
        expect(trainStep.params?.learningRate).toBe(0.00005);
        expect(trainStep.params?.batchSize).toBe(8);
        expect(trainStep.params?.traitType).toBe('teaching');
      }

      const registerStep = conditionStep.then[1];
      if (registerStep.type === 'command') {
        expect(registerStep.params?.domain).toBe('teaching');
      }
    }
  });

  it('should produce JSON-serializable output compatible with Rust pipeline schema', () => {
    const pipeline = buildLoRATrainingPipeline(testConfig);
    const json = JSON.stringify(pipeline);
    const parsed = JSON.parse(json);

    expect(parsed.name).toBe(pipeline.name);
    expect(parsed.steps.length).toBe(pipeline.steps.length);
    expect(parsed.steps[0].type).toBe('command');
    expect(parsed.steps[1].type).toBe('condition');
  });
});

describe('AdapterPackage — Manifest & Entity', () => {
  let tempDir: string;

  beforeEach(async () => {
    tempDir = path.join(os.tmpdir(), `adapter-test-${Date.now()}`);
    await fs.promises.mkdir(tempDir, { recursive: true });
  });

  const testManifest: AdapterPackageManifest = {
    id: '11111111-1111-1111-1111-111111111111' as UUID,
    name: 'test-ai-conversational',
    traitType: 'conversational',
    source: 'trained',
    baseModel: 'smollm2:135m',
    rank: 32,
    sizeMB: 42.5,
    personaId: '22222222-2222-2222-2222-222222222222' as UUID,
    personaName: 'Test AI',
    trainingMetadata: {
      epochs: 3,
      loss: 4.03,
      performance: 0,
      trainingDuration: 27000,
      datasetHash: 'sha256:abc123',
    },
    contentHash: 'sha256:deadbeef',
    createdAt: '2026-02-17T02:25:00.000Z',
    version: 1,
  };

  it('should write and read manifest roundtrip', async () => {
    await AdapterPackage.writeManifest(tempDir, testManifest);

    // Verify file exists
    const manifestPath = path.join(tempDir, 'manifest.json');
    expect(fs.existsSync(manifestPath)).toBe(true);

    // Read it back
    const read = await AdapterPackage.readManifest(tempDir);
    expect(read.id).toBe(testManifest.id);
    expect(read.name).toBe(testManifest.name);
    expect(read.traitType).toBe(testManifest.traitType);
    expect(read.source).toBe('trained');
    expect(read.baseModel).toBe('smollm2:135m');
    expect(read.rank).toBe(32);
    expect(read.sizeMB).toBe(42.5);
    expect(read.personaId).toBe(testManifest.personaId);
    expect(read.trainingMetadata.epochs).toBe(3);
    expect(read.trainingMetadata.loss).toBe(4.03);
    expect(read.contentHash).toBe('sha256:deadbeef');
    expect(read.version).toBe(1);
  });

  it('should calculate directory size in MB', async () => {
    // Write a test file with known size
    const testFile = path.join(tempDir, 'test.bin');
    const buffer = Buffer.alloc(1024 * 100); // 100KB
    await fs.promises.writeFile(testFile, buffer);

    const sizeMB = await AdapterPackage.calculateSizeMB(tempDir);
    expect(sizeMB).toBeGreaterThan(0);
    expect(sizeMB).toBeLessThan(1); // 100KB < 1MB
  });

  it('should calculate content hash from file', async () => {
    // Write a test safetensors file
    const weightsPath = path.join(tempDir, 'adapter_model.safetensors');
    await fs.promises.writeFile(weightsPath, 'test weights data');

    const hash = await AdapterPackage.calculateContentHash(tempDir);
    expect(hash).toMatch(/^sha256:[0-9a-f]{64}$/);
  });

  it('should fallback to directory fingerprint when no weights file', async () => {
    // Write a non-weights file
    const otherFile = path.join(tempDir, 'readme.txt');
    await fs.promises.writeFile(otherFile, 'hello');

    const hash = await AdapterPackage.calculateContentHash(tempDir);
    expect(hash).toMatch(/^sha256:[0-9a-f]{64}$/);
  });

  it('should convert manifest to GenomeLayerEntity', () => {
    const entity = AdapterPackage.toGenomeLayerEntity(testManifest, '/path/to/adapter');

    expect(entity).toBeInstanceOf(GenomeLayerEntity);
    expect(entity.id).toBe(testManifest.id);
    expect(entity.name).toBe('test-ai-conversational');
    expect(entity.traitType).toBe('conversational');
    expect(entity.source).toBe('trained');
    expect(entity.modelPath).toBe('/path/to/adapter');
    expect(entity.sizeMB).toBe(42.5);
    expect(entity.rank).toBe(32);
    expect(entity.creatorId).toBe(testManifest.personaId);
    expect(entity.contentHash).toBe('sha256:deadbeef');
    expect(entity.tags).toContain('conversational');
    expect(entity.tags).toContain('smollm2:135m');
    expect(entity.tags).toContain('test ai');
    expect(entity.generation).toBe(0);
    expect(entity.trainingMetadata?.epochs).toBe(3);
    expect(entity.trainingMetadata?.loss).toBe(4.03);
    expect(entity.description).toContain('Test AI');
    expect(entity.description).toContain('conversational');
    expect(entity.description).toContain('smollm2:135m');

    // Entity should be valid
    const validation = entity.validate();
    // sizeMB > 0, rank > 0, modelPath is set — should pass
    expect(validation.success).toBe(true);
  });

  it('should build manifest from training params', () => {
    const manifest = AdapterPackage.buildManifest({
      adapterPath: '/path/to/adapter',
      personaId: '33333333-3333-3333-3333-333333333333' as UUID,
      personaName: 'Helper AI',
      traitType: 'teaching',
      baseModel: 'llama3.2:1b',
      rank: 16,
      sizeMB: 25.0,
      contentHash: 'sha256:cafe',
      trainingMetadata: {
        epochs: 5,
        loss: 2.1,
        performance: 0,
        trainingDuration: 60000,
      },
    });

    expect(manifest.id).toBeDefined();
    expect(manifest.id.length).toBeGreaterThan(0);
    expect(manifest.name).toBe('helper-ai-teaching');
    expect(manifest.traitType).toBe('teaching');
    expect(manifest.source).toBe('trained');
    expect(manifest.baseModel).toBe('llama3.2:1b');
    expect(manifest.rank).toBe(16);
    expect(manifest.sizeMB).toBe(25.0);
    expect(manifest.personaId).toBe('33333333-3333-3333-3333-333333333333');
    expect(manifest.personaName).toBe('Helper AI');
    expect(manifest.contentHash).toBe('sha256:cafe');
    expect(manifest.version).toBe(1);
    expect(manifest.createdAt).toBeDefined();
  });

  it('should scan directory for adapter packages', async () => {
    // Create two adapter subdirectories with manifests
    const adapter1Dir = path.join(tempDir, 'adapter-1');
    const adapter2Dir = path.join(tempDir, 'adapter-2');
    const emptyDir = path.join(tempDir, 'empty-dir');
    await fs.promises.mkdir(adapter1Dir, { recursive: true });
    await fs.promises.mkdir(adapter2Dir, { recursive: true });
    await fs.promises.mkdir(emptyDir, { recursive: true });

    const manifest1 = { ...testManifest, id: 'aaaa-1' as UUID, name: 'adapter-1' };
    const manifest2 = { ...testManifest, id: 'bbbb-2' as UUID, name: 'adapter-2' };
    await AdapterPackage.writeManifest(adapter1Dir, manifest1);
    await AdapterPackage.writeManifest(adapter2Dir, manifest2);

    const manifests = await AdapterPackage.scanAdapterDirectory(tempDir);
    expect(manifests.length).toBe(2);

    const names = manifests.map(m => m.name).sort();
    expect(names).toEqual(['adapter-1', 'adapter-2']);
  });

  it('should return empty array for non-existent directory', async () => {
    const manifests = await AdapterPackage.scanAdapterDirectory('/nonexistent/path');
    expect(manifests).toEqual([]);
  });
});

// ============================================================================
// Academy Dojo: Entity Validation + Pipeline Templates + Event Taxonomy
// ============================================================================

import { AcademySessionEntity } from '../../system/genome/entities/AcademySessionEntity';
import { AcademyCurriculumEntity } from '../../system/genome/entities/AcademyCurriculumEntity';
import { AcademyExaminationEntity } from '../../system/genome/entities/AcademyExaminationEntity';
import {
  academyEvent,
  DEFAULT_ACADEMY_CONFIG,
  VALID_SESSION_STATUSES,
} from '../../system/genome/shared/AcademyTypes';
import type { CurriculumTopic, ExamQuestion, ExamResponse } from '../../system/genome/shared/AcademyTypes';
import { buildTeacherPipeline } from '../../system/sentinel/pipelines/TeacherPipeline';
import { buildStudentPipeline } from '../../system/sentinel/pipelines/StudentPipeline';

describe('Academy Event Taxonomy', () => {
  it('should generate scoped event names', () => {
    const sessionId = 'abc-123';
    expect(academyEvent(sessionId, 'curriculum:ready')).toBe('academy:abc-123:curriculum:ready');
    expect(academyEvent(sessionId, 'dataset:ready')).toBe('academy:abc-123:dataset:ready');
    expect(academyEvent(sessionId, 'training:complete')).toBe('academy:abc-123:training:complete');
    expect(academyEvent(sessionId, 'exam:ready')).toBe('academy:abc-123:exam:ready');
    expect(academyEvent(sessionId, 'exam:responses')).toBe('academy:abc-123:exam:responses');
    expect(academyEvent(sessionId, 'exam:graded')).toBe('academy:abc-123:exam:graded');
    expect(academyEvent(sessionId, 'session:complete')).toBe('academy:abc-123:session:complete');
    expect(academyEvent(sessionId, 'session:failed')).toBe('academy:abc-123:session:failed');
  });

  it('should isolate different sessions', () => {
    const event1 = academyEvent('session-a', 'dataset:ready');
    const event2 = academyEvent('session-b', 'dataset:ready');
    expect(event1).not.toBe(event2);
    expect(event1).toBe('academy:session-a:dataset:ready');
    expect(event2).toBe('academy:session-b:dataset:ready');
  });
});

describe('AcademySessionEntity', () => {
  it('should validate required fields', () => {
    const entity = new AcademySessionEntity();

    // Missing personaId
    let result = entity.validate();
    expect(result.success).toBe(false);
    expect(result.error).toContain('personaId');

    // Fill required fields incrementally
    entity.personaId = 'test-persona-id' as UUID;
    result = entity.validate();
    expect(result.success).toBe(false);
    expect(result.error).toContain('personaName');

    entity.personaName = 'Test Persona';
    result = entity.validate();
    expect(result.success).toBe(false);
    expect(result.error).toContain('skill');

    entity.skill = 'typescript-generics';
    // baseModel has a default ('smollm2:135m') so it passes validation
    result = entity.validate();
    expect(result.success).toBe(true);
  });

  it('should validate status values', () => {
    const entity = new AcademySessionEntity();
    entity.personaId = 'id' as UUID;
    entity.personaName = 'Test';
    entity.skill = 'test-skill';
    entity.baseModel = 'smollm2:135m';

    for (const status of VALID_SESSION_STATUSES) {
      entity.status = status;
      expect(entity.validate().success).toBe(true);
    }

    entity.status = 'invalid' as any;
    expect(entity.validate().success).toBe(false);
  });

  it('should have correct collection name', () => {
    expect(AcademySessionEntity.collection).toBe('academy_sessions');
    const entity = new AcademySessionEntity();
    expect(entity.collection).toBe('academy_sessions');
  });

  it('should use default config values', () => {
    const entity = new AcademySessionEntity();
    expect(entity.config.maxTopicAttempts).toBe(DEFAULT_ACADEMY_CONFIG.maxTopicAttempts);
    expect(entity.config.passingScore).toBe(DEFAULT_ACADEMY_CONFIG.passingScore);
    expect(entity.config.epochs).toBe(DEFAULT_ACADEMY_CONFIG.epochs);
    expect(entity.config.rank).toBe(DEFAULT_ACADEMY_CONFIG.rank);
  });

  it('should validate config bounds', () => {
    const entity = new AcademySessionEntity();
    entity.personaId = 'id' as UUID;
    entity.personaName = 'Test';
    entity.skill = 'test';
    entity.baseModel = 'smollm2:135m';

    entity.config.passingScore = 150;
    expect(entity.validate().success).toBe(false);

    entity.config.passingScore = -10;
    expect(entity.validate().success).toBe(false);

    entity.config.passingScore = 70;
    entity.config.maxTopicAttempts = 0;
    expect(entity.validate().success).toBe(false);
  });
});

describe('AcademyCurriculumEntity', () => {
  const validTopic: CurriculumTopic = {
    name: 'Generic Types',
    description: 'Understanding TypeScript generic type parameters',
    difficulty: 'beginner',
    status: 'pending',
    attempts: 0,
    bestScore: 0,
  };

  it('should validate required fields', () => {
    const entity = new AcademyCurriculumEntity();
    expect(entity.validate().success).toBe(false);

    entity.sessionId = 'session-1' as UUID;
    entity.skill = 'typescript';
    entity.generatedBy = 'claude-3-opus';
    entity.topics = [validTopic];
    entity.totalTopics = 1;
    expect(entity.validate().success).toBe(true);
  });

  it('should validate topic structure', () => {
    const entity = new AcademyCurriculumEntity();
    entity.sessionId = 'session-1' as UUID;
    entity.skill = 'typescript';
    entity.generatedBy = 'claude';
    entity.totalTopics = 1;

    // Empty topics array
    entity.topics = [];
    expect(entity.validate().success).toBe(false);

    // Topic with missing name
    entity.topics = [{ ...validTopic, name: '' }];
    expect(entity.validate().success).toBe(false);

    // Topic with invalid difficulty
    entity.topics = [{ ...validTopic, difficulty: 'expert' as any }];
    expect(entity.validate().success).toBe(false);

    // Valid topic
    entity.topics = [validTopic];
    expect(entity.validate().success).toBe(true);
  });

  it('should validate totalTopics matches array length', () => {
    const entity = new AcademyCurriculumEntity();
    entity.sessionId = 'session-1' as UUID;
    entity.skill = 'typescript';
    entity.generatedBy = 'claude';
    entity.topics = [validTopic, { ...validTopic, name: 'Topic 2' }];
    entity.totalTopics = 3;  // Mismatch
    expect(entity.validate().success).toBe(false);

    entity.totalTopics = 2;
    expect(entity.validate().success).toBe(true);
  });

  it('should have correct collection name', () => {
    expect(AcademyCurriculumEntity.collection).toBe('academy_curricula');
  });
});

describe('AcademyExaminationEntity', () => {
  const validQuestion: ExamQuestion = {
    question: 'What is a generic type constraint?',
    expectedAnswer: 'A way to restrict the types that can be used as type arguments...',
    category: 'Type Constraints',
  };

  it('should validate required fields', () => {
    const entity = new AcademyExaminationEntity();
    expect(entity.validate().success).toBe(false);

    entity.sessionId = 'session-1' as UUID;
    entity.questions = [validQuestion];
    expect(entity.validate().success).toBe(true);
  });

  it('should validate question structure', () => {
    const entity = new AcademyExaminationEntity();
    entity.sessionId = 'session-1' as UUID;

    entity.questions = [{ ...validQuestion, question: '' }];
    expect(entity.validate().success).toBe(false);

    entity.questions = [{ ...validQuestion, expectedAnswer: '' }];
    expect(entity.validate().success).toBe(false);

    entity.questions = [{ ...validQuestion, category: '' }];
    expect(entity.validate().success).toBe(false);
  });

  it('should validate score bounds', () => {
    const entity = new AcademyExaminationEntity();
    entity.sessionId = 'session-1' as UUID;
    entity.questions = [validQuestion];

    entity.overallScore = 150;
    expect(entity.validate().success).toBe(false);

    entity.overallScore = -5;
    expect(entity.validate().success).toBe(false);

    entity.overallScore = 85;
    expect(entity.validate().success).toBe(true);
  });

  it('should validate round is >= 1', () => {
    const entity = new AcademyExaminationEntity();
    entity.sessionId = 'session-1' as UUID;
    entity.questions = [validQuestion];

    entity.round = 0;
    expect(entity.validate().success).toBe(false);

    entity.round = 1;
    expect(entity.validate().success).toBe(true);
  });

  it('should have correct collection name', () => {
    expect(AcademyExaminationEntity.collection).toBe('academy_examinations');
  });
});

describe('TeacherPipeline', () => {
  const testConfig = {
    sessionId: 'test-session-123' as UUID,
    skill: 'typescript-generics',
    personaName: 'Helper AI',
    baseModel: 'smollm2:135m',
    config: { ...DEFAULT_ACADEMY_CONFIG },
  };

  it('should build a valid pipeline with correct name', () => {
    const pipeline = buildTeacherPipeline(testConfig);
    expect(pipeline.name).toBe('academy-teacher-typescript-generics');
    expect(pipeline.steps.length).toBeGreaterThan(0);
  });

  it('should include curriculum design LLM step first', () => {
    const pipeline = buildTeacherPipeline(testConfig);
    const firstStep = pipeline.steps[0];
    expect(firstStep.type).toBe('llm');
    if (firstStep.type === 'llm') {
      expect(firstStep.prompt).toContain('typescript-generics');
      expect(firstStep.prompt).toContain('Helper AI');
    }
  });

  it('should include curriculum:ready emit step', () => {
    const pipeline = buildTeacherPipeline(testConfig);
    const emitStep = pipeline.steps.find(s => s.type === 'emit' && (s as any).event.includes('curriculum:ready'));
    expect(emitStep).toBeDefined();
  });

  it('should include topic loop with dataset synthesis + exam flow', () => {
    const pipeline = buildTeacherPipeline(testConfig);
    const loopStep = pipeline.steps.find(s => s.type === 'loop');
    expect(loopStep).toBeDefined();
    if (loopStep?.type === 'loop') {
      // Loop should contain: synthesize, emit dataset, watch training, exam LLM, persist exam, emit exam, watch responses, grade LLM, persist grades, emit graded, condition
      expect(loopStep.steps.length).toBeGreaterThanOrEqual(10);
    }
  });

  it('should include session:complete emit at the end', () => {
    const pipeline = buildTeacherPipeline(testConfig);
    const lastStep = pipeline.steps[pipeline.steps.length - 1];
    expect(lastStep.type).toBe('emit');
    if (lastStep.type === 'emit') {
      expect(lastStep.event).toContain('session:complete');
    }
  });

  it('should pass inputs through', () => {
    const pipeline = buildTeacherPipeline(testConfig);
    expect(pipeline.inputs?.sessionId).toBe('test-session-123');
    expect(pipeline.inputs?.skill).toBe('typescript-generics');
    expect(pipeline.inputs?.personaName).toBe('Helper AI');
  });
});

describe('StudentPipeline', () => {
  const testConfig = {
    sessionId: 'test-session-123' as UUID,
    personaId: 'persona-456' as UUID,
    personaName: 'Helper AI',
    baseModel: 'smollm2:135m',
    config: { ...DEFAULT_ACADEMY_CONFIG },
  };

  it('should build a valid pipeline with correct name', () => {
    const pipeline = buildStudentPipeline(testConfig);
    expect(pipeline.name).toBe('academy-student-helper-ai');
    expect(pipeline.steps.length).toBeGreaterThan(0);
  });

  it('should start by watching for curriculum:ready', () => {
    const pipeline = buildStudentPipeline(testConfig);
    const firstStep = pipeline.steps[0];
    expect(firstStep.type).toBe('watch');
    if (firstStep.type === 'watch') {
      expect(firstStep.event).toContain('curriculum:ready');
    }
  });

  it('should include topic loop with training + exam flow', () => {
    const pipeline = buildStudentPipeline(testConfig);
    const loopStep = pipeline.steps.find(s => s.type === 'loop');
    expect(loopStep).toBeDefined();
    if (loopStep?.type === 'loop') {
      // Loop should contain: watch dataset, emit started, train, register adapter, emit complete, watch exam, LLM answer, emit responses, watch graded
      expect(loopStep.steps.length).toBeGreaterThanOrEqual(8);
    }
  });

  it('should use base model for exam answers', () => {
    const pipeline = buildStudentPipeline(testConfig);
    const loopStep = pipeline.steps.find(s => s.type === 'loop');
    if (loopStep?.type === 'loop') {
      const llmStep = loopStep.steps.find(s => s.type === 'llm');
      expect(llmStep).toBeDefined();
      if (llmStep?.type === 'llm') {
        expect(llmStep.model).toBe('smollm2:135m');
      }
    }
  });

  it('should pass inputs through', () => {
    const pipeline = buildStudentPipeline(testConfig);
    expect(pipeline.inputs?.sessionId).toBe('test-session-123');
    expect(pipeline.inputs?.personaId).toBe('persona-456');
    expect(pipeline.inputs?.personaName).toBe('Helper AI');
  });
});
