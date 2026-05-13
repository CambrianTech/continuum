import { describe, expect, it } from 'vitest';
import { CodebaseSearchSource } from '../../sources/CodebaseSearchSource';
import type { RAGSourceContext } from '../../shared/RAGSource';

function contextFor(message: string, activeSources?: readonly string[]): RAGSourceContext {
  return {
    personaId: 'persona-1' as any,
    roomId: 'room-1' as any,
    options: {
      currentMessage: {
        role: 'user',
        content: message,
        name: 'Developer',
        timestamp: Date.now(),
      },
      modelId: 'continuum-ai/qwen3.5-4b-code-forged-GGUF',
      provider: 'local',
      maxTokens: 256,
      contextWindow: 8192,
      tokensPerSecond: 15,
    },
    totalBudget: 4096,
    provider: 'local',
    activeSources,
  };
}

describe('CodebaseSearchSource activation', () => {
  it('does not run codebase search for ordinary chat', () => {
    const source = new CodebaseSearchSource();

    expect(source.isApplicable(contextFor('Personas: reply with your name and confirm you can see this message.'))).toBe(false);
    expect(source.isApplicable(contextFor('Teacher AI: Yes, I can confirm seeing this startup smoke test in the General room.'))).toBe(false);
    expect(source.isApplicable(contextFor('tacos, tell me all you know'))).toBe(false);
  });

  it('runs for technical/code intent', () => {
    const source = new CodebaseSearchSource();

    expect(source.isApplicable(contextFor('Why does ChatRAGBuilder time out on codebase-search?'))).toBe(true);
    expect(source.isApplicable(contextFor('Fix workers/continuum-core/src/model_registry/artifacts.rs'))).toBe(true);
    expect(source.isApplicable(contextFor('The docker build is failing with a Rust compile error.'))).toBe(true);
    expect(source.isApplicable(contextFor('The integration tests are failing after the Docker refactor.'))).toBe(true);
  });

  it('honors explicit recipe source activation', () => {
    const source = new CodebaseSearchSource();

    expect(source.isApplicable(contextFor('fix this', ['codebase-search']))).toBe(true);
  });
});
