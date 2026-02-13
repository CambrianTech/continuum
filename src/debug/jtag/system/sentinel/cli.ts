#!/usr/bin/env npx tsx
/**
 * Sentinel CLI - Run sentinels from command line
 *
 * Usage:
 *   npx tsx system/sentinel/cli.ts build [--command="npm run build:ts"] [--max-attempts=3]
 *   npx tsx system/sentinel/cli.ts build --file=path/to/file.ts  # Compile single file
 *
 * Output: JSON result for easy parsing by AI
 */

import { BuildSentinel, type SentinelProgress } from './BuildSentinel';

const args = process.argv.slice(2);
const command = args[0];

function parseArgs(args: string[]): Record<string, string> {
  const result: Record<string, string> = {};
  for (const arg of args) {
    if (arg.startsWith('--')) {
      const [key, ...valueParts] = arg.slice(2).split('=');
      result[key] = valueParts.join('=') || 'true';
    }
  }
  return result;
}

async function runBuild(options: Record<string, string>) {
  const workingDir = options['working-dir'] || process.cwd();
  const maxAttempts = parseInt(options['max-attempts'] || '3', 10);

  // Determine build command
  let buildCommand: string;
  if (options['file']) {
    buildCommand = `npx tsc --noEmit --skipLibCheck ${options['file']}`;
  } else if (options['command']) {
    buildCommand = options['command'];
  } else {
    buildCommand = 'npm run build:ts';
  }

  const progressLog: SentinelProgress[] = [];

  const sentinel = new BuildSentinel({
    command: buildCommand,
    workingDir,
    maxAttempts,
    canAutoFix: options['auto-fix'] !== 'false',
    onProgress: (p) => {
      progressLog.push(p);
      // Also print human-readable progress to stderr
      process.stderr.write(`[${p.phase.toUpperCase()}] (${p.attempt}/${p.maxAttempts}) ${p.message}\n`);
    },
  });

  const result = await sentinel.run();

  // Output JSON result to stdout for AI parsing
  console.log(JSON.stringify({
    success: result.success,
    attempts: result.attempts.length,
    escalated: result.escalated || false,
    escalationReason: result.escalationReason,
    errors: result.finalErrors || [],
    fixes: result.attempts
      .filter(a => a.fixApplied)
      .map(a => a.fixApplied),
    progress: progressLog,
  }, null, 2));

  process.exit(result.success ? 0 : 1);
}

async function main() {
  const options = parseArgs(args.slice(1));

  switch (command) {
    case 'build':
      await runBuild(options);
      break;

    case 'help':
    default:
      console.log(`
Sentinel CLI - Focused Agentic Loops

Commands:
  build    Run BuildSentinel to compile code

Options for 'build':
  --command="..."      Build command (default: npm run build:ts)
  --file=path/to/file  Compile single file with tsc
  --working-dir=...    Working directory (default: cwd)
  --max-attempts=N     Max retry attempts (default: 3)
  --auto-fix=false     Disable auto-fix attempts

Examples:
  # Build entire project
  npx tsx system/sentinel/cli.ts build

  # Compile single file
  npx tsx system/sentinel/cli.ts build --file=system/sentinel/test-error.ts

  # Custom build command
  npx tsx system/sentinel/cli.ts build --command="cargo build --release"
`);
      break;
  }
}

main().catch((e) => {
  console.error(JSON.stringify({ error: e.message }));
  process.exit(1);
});
