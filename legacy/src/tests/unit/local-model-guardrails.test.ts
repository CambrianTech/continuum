import { describe, expect, it } from 'vitest';
import { LOCAL_MODELS } from '@system/shared/Constants';

describe('LOCAL_MODELS guardrails', () => {
  it('keeps accepted Qwen aliases mapped through the local runtime source of truth', () => {
    expect(LOCAL_MODELS.mapToHuggingFace('qwen3.5')).toBe(LOCAL_MODELS.DEFAULT);
    expect(LOCAL_MODELS.mapToHuggingFace('qwen3.5:4b')).toBe(LOCAL_MODELS.DEFAULT);
    expect(LOCAL_MODELS.mapToHuggingFace('qwen2-vl')).toBe(LOCAL_MODELS.VISION);
  });

  it('rejects removed local aliases instead of silently routing stale llama/Candle configs', () => {
    for (const alias of Object.keys(LOCAL_MODELS.REMOVED_LOCAL_ALIASES)) {
      expect(() => LOCAL_MODELS.mapToHuggingFace(alias)).toThrow(/was removed from the runtime/);
    }
  });

  it('rejects removed aliases even when callers append an instruction or quant suffix', () => {
    expect(() => LOCAL_MODELS.mapToHuggingFace('llama3.2:3b-instruct')).toThrow(/Use 'qwen3.5'/);
    expect(() => LOCAL_MODELS.mapToHuggingFace('phi3:mini-q4_k_m')).toThrow(/Use 'qwen2'/);
  });

  it('still accepts explicit HuggingFace ids for registry/catalog entries', () => {
    const rawModel = 'Qwen/Qwen2.5-7B-Instruct';
    expect(LOCAL_MODELS.mapToHuggingFace(rawModel)).toBe(rawModel);
  });
});
