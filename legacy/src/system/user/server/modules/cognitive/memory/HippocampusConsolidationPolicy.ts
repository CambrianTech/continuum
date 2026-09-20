const ENABLE_LLM_MEMORY_SYNTHESIS_ENV = 'CONTINUUM_ENABLE_LLM_MEMORY_SYNTHESIS';
type Env = Readonly<Record<string, string | undefined>>;
export type MemoryConsolidationMode = 'raw' | 'semantic';

export function getDefaultConsolidationMode(env: Env = process.env): MemoryConsolidationMode {
  const value = env[ENABLE_LLM_MEMORY_SYNTHESIS_ENV]?.toLowerCase();
  const enabled = value === '1' || value === 'true' || value === 'yes';
  return enabled ? 'semantic' : 'raw';
}

export function isLlmMemorySynthesisEnabled(env: Env = process.env): boolean {
  const value = env[ENABLE_LLM_MEMORY_SYNTHESIS_ENV]?.toLowerCase();
  return value === '1' || value === 'true' || value === 'yes';
}
