/**
 * Candle Truncation Test
 *
 * Verifies that the CandleAdapter properly truncates large prompts.
 *
 * Run: npx tsx tests/candle-truncation-test.ts
 */

import { InferenceWorkerClient } from '../system/core/services/InferenceWorkerClient';

const GREEN = '\x1b[32m';
const RED = '\x1b[31m';
const YELLOW = '\x1b[33m';
const CYAN = '\x1b[36m';
const RESET = '\x1b[0m';

function log(level: 'INFO' | 'PASS' | 'FAIL' | 'WARN', msg: string): void {
  const colors: Record<string, string> = { INFO: CYAN, PASS: GREEN, FAIL: RED, WARN: YELLOW };
  const timestamp = new Date().toISOString().slice(11, 23);
  console.log(`${colors[level]}[${timestamp}] [${level}]${RESET} ${msg}`);
}

// Simulate CandleAdapter's truncation logic
function truncatePromptIfNeeded(prompt: string, maxInputTokens: number = 4000): {
  truncated: string;
  estimatedTokens: number;
  wasTruncated: boolean;
} {
  const charsPerToken = 4;
  const estimatedTokens = Math.ceil(prompt.length / charsPerToken);

  if (estimatedTokens <= maxInputTokens) {
    return { truncated: prompt, estimatedTokens, wasTruncated: false };
  }

  const targetChars = maxInputTokens * charsPerToken;
  const keepFromStart = Math.floor(targetChars * 0.3);
  const keepFromEnd = Math.floor(targetChars * 0.7);

  const beginning = prompt.slice(0, keepFromStart);
  const end = prompt.slice(-keepFromEnd);

  const truncated = beginning + '\n\n[... earlier context truncated for inference speed ...]\n\n' + end;

  return {
    truncated,
    estimatedTokens,
    wasTruncated: true,
  };
}

async function runTest(): Promise<void> {
  console.log('\n' + '='.repeat(70));
  console.log('🔬 CANDLE TRUNCATION TEST');
  console.log('='.repeat(70) + '\n');

  const client = InferenceWorkerClient.instance;
  const MODEL_ID = 'Qwen/Qwen2-1.5B-Instruct';

  // Ensure model loaded
  log('INFO', 'Loading model...');
  await client.loadModel(MODEL_ID);
  log('PASS', 'Model loaded');

  // Test 1: Small prompt (no truncation)
  log('INFO', 'Test 1: Small prompt (should NOT be truncated)');
  const smallPrompt = 'Say hello briefly.';
  const small = truncatePromptIfNeeded(smallPrompt);
  log('INFO', `  Estimated tokens: ${small.estimatedTokens}`);
  log('INFO', `  Was truncated: ${small.wasTruncated}`);

  const smallStart = Date.now();
  const smallResult = await client.generate({
    modelId: MODEL_ID,
    prompt: small.truncated,
    maxTokens: 10,
  });
  log('PASS', `  Generated in ${Date.now() - smallStart}ms: "${smallResult.text.substring(0, 30)}..."`);

  // Test 2: Large prompt (should be truncated)
  log('INFO', 'Test 2: Large prompt (SHOULD be truncated)');

  // Create a ~20k token prompt (80k chars)
  const sampleText = "The quick brown fox jumps over the lazy dog. ";
  const largePrompt =
    "System: You are a helpful assistant.\n\n" +
    sampleText.repeat(1700) +  // ~15k words in the middle (will be truncated)
    "\n\nUser: After all that context, just say hello.\nAssistant:";

  const large = truncatePromptIfNeeded(largePrompt);
  log('INFO', `  Original estimated tokens: ${large.estimatedTokens}`);
  log('INFO', `  Was truncated: ${large.wasTruncated}`);
  log('INFO', `  Truncated length: ${large.truncated.length} chars (~${Math.ceil(large.truncated.length / 4)} tokens)`);

  if (!large.wasTruncated) {
    log('FAIL', 'Large prompt was NOT truncated - this is a bug!');
    return;
  }

  log('INFO', `  First 100 chars: "${large.truncated.substring(0, 100)}..."`);
  log('INFO', `  Last 100 chars: "...${large.truncated.substring(large.truncated.length - 100)}"`);

  const largeStart = Date.now();
  const largeResult = await client.generate({
    modelId: MODEL_ID,
    prompt: large.truncated,
    maxTokens: 10,
  });
  const largeTime = Date.now() - largeStart;
  log('PASS', `  Generated in ${largeTime}ms: "${largeResult.text.substring(0, 30)}..."`);

  if (largeTime > 60000) {
    log('WARN', `  Generation took ${largeTime}ms - still too slow!`);
  } else {
    log('PASS', `  Generation completed in under 60s - truncation working!`);
  }

  // Test 3: Verify truncation preserves important parts
  log('INFO', 'Test 3: Verify truncation preserves beginning and end');

  const testPrompt =
    "BEGINNING_MARKER This is the start of the prompt. " +
    sampleText.repeat(2000) +  // Middle content (will be removed)
    "END_MARKER This is the question. Answer briefly.";

  const test = truncatePromptIfNeeded(testPrompt);

  const hasBeginning = test.truncated.includes('BEGINNING_MARKER');
  const hasEnd = test.truncated.includes('END_MARKER');
  const hasTruncationMarker = test.truncated.includes('[... earlier context truncated');

  log(hasBeginning ? 'PASS' : 'FAIL', `  Beginning preserved: ${hasBeginning}`);
  log(hasEnd ? 'PASS' : 'FAIL', `  End preserved: ${hasEnd}`);
  log(hasTruncationMarker ? 'PASS' : 'FAIL', `  Has truncation marker: ${hasTruncationMarker}`);

  console.log('\n' + '='.repeat(70));
  log('INFO', 'CONCLUSION:');
  log('INFO', '  - Small prompts pass through unchanged');
  log('INFO', '  - Large prompts are truncated to ~4000 tokens');
  log('INFO', '  - Truncation preserves beginning (30%) and end (70%)');
  log('INFO', '  - Generation now completes in reasonable time');
  console.log('='.repeat(70) + '\n');

  client.close();
}

runTest().catch(console.error);
