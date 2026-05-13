import { describe, expect, it } from 'vitest';
import { readdirSync, readFileSync, statSync } from 'fs';
import { join, relative } from 'path';

const ROOT = process.cwd();
const NODE_IMPORT_PATTERN =
  /(?:from|import)\s+['"](?:node:)?(?:fs|fs\/promises|path|crypto|os|child_process|events)['"]|from\s+['"](?:node:)?(?:fs|fs\/promises|path|crypto|os|child_process|events)['"]|require\(['"](?:node:)?(?:fs|fs\/promises|path|crypto|os|child_process|events)['"]\)/;

// Ratchet, not approval: these are existing shared/browser-boundary violations.
// New paths should not be added casually. If a shared module genuinely needs a
// Node builtin, move it under a server-only boundary where possible; otherwise
// document the architectural reason in the commit that updates this set.
const KNOWN_SHARED_NODE_IMPORTS = new Set([
  'commands/ai/dataset/shared/parsers/GitHistoryParser.ts',
  'commands/list/shared/ListCommand.ts',
  'commands/logs/shared/LogsShared.ts',
  'commands/media/process/shared/MediaProcessTypes.ts',
  'commands/utilities/docs/shared/DocFileRegistry.ts',
  'commands/workspace/git/shared/resolveWorkspacePath.ts',
  'daemons/ai-provider-daemon/adapters/candle/shared/CandleAdapter.ts',
  'daemons/ai-provider-daemon/adapters/sentinel/shared/SentinelAdapter.ts',
  'daemons/ai-provider-daemon/shared/BaseAIProviderAdapter.ts',
  'daemons/ai-provider-daemon/shared/HardwareProfile.ts',
  'daemons/ai-provider-daemon/shared/LlamaCppAdapter.ts',
  'daemons/ai-provider-daemon/shared/adapters/BaseLocalAdapter.ts',
  'daemons/file-daemon/shared/FileDaemon.ts',
  'examples/shared/ConnectionConfigFactory.ts',
  'generator/shared/SpecSerializer.ts',
  'scripts/shared/Preflight.ts',
  'shared/ModelRegistry.ts',
  'shared/ipc/archive-worker/CommandRouterServer.ts',
  'shared/utils/ProcessUtils.ts',
  'shared/workers/PersonaWorkerThread.ts',
  'system/core/router/shared/JTAGRouterOptimized.ts',
  'system/core/shared/TimingHarness.ts',
  'system/shared/Config.ts',
  'system/typescript/shared/TypeScriptCompiler.ts',
  'system/user/shared/BaseUser.ts',
  'tests/shared/AdvancedPerformanceTester.ts',
  'tests/shared/PerformanceTester.ts',
  'tests/shared/ScreenshotTesting.ts',
  'tests/shared/TestAssertions.ts',
  'tests/shared/TestConfig.ts',
  'tests/shared/TestRunner.ts',
]);

function walk(dir: string): string[] {
  const results: string[] = [];
  for (const entry of readdirSync(dir)) {
    if (
      entry === '.git' ||
      entry === 'node_modules' ||
      entry === 'dist' ||
      entry === 'build'
    ) {
      continue;
    }

    const fullPath = join(dir, entry);
    const stat = statSync(fullPath);
    if (stat.isDirectory()) {
      results.push(...walk(fullPath));
    } else if (entry.endsWith('.ts') || entry.endsWith('.tsx')) {
      results.push(fullPath);
    }
  }
  return results;
}

function isSharedRuntimeFile(file: string): boolean {
  const rel = relative(ROOT, file).replaceAll('\\', '/');
  if (rel.includes('/server/') || rel.includes('/test/') || rel.includes('.test.')) {
    return false;
  }

  return rel.startsWith('shared/') ||
    rel.includes('/shared/');
}

describe('shared/browser Node import boundary', () => {
  it('does not add new Node builtin imports to shared runtime modules', () => {
    const offenders = walk(ROOT)
      .filter(isSharedRuntimeFile)
      .filter(file => NODE_IMPORT_PATTERN.test(readFileSync(file, 'utf8')))
      .map(file => relative(ROOT, file).replaceAll('\\', '/').replace(/^src\//, ''))
      .sort();

    expect(offenders).toEqual([...KNOWN_SHARED_NODE_IMPORTS].sort());
  });
});
