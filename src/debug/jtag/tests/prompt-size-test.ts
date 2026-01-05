/**
 * Prompt Size Test
 *
 * Tests how InferenceWorkerClient handles prompts of various sizes.
 * The real issue: 20k token prompts take forever to process!
 *
 * Run: npx tsx tests/prompt-size-test.ts
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

// Generate a long prompt (approximately N words)
function generateLongPrompt(approxWords: number): string {
  const sampleText = "The quick brown fox jumps over the lazy dog. ";
  const wordsPerSample = 9;
  const repeats = Math.ceil(approxWords / wordsPerSample);
  return sampleText.repeat(repeats);
}

async function testPromptSizes(): Promise<void> {
  console.log('\n' + '='.repeat(70));
  console.log('🔬 PROMPT SIZE TEST');
  console.log('='.repeat(70) + '\n');

  const client = InferenceWorkerClient.instance;
  const MODEL_ID = 'Qwen/Qwen2-1.5B-Instruct';

  // Ensure model loaded
  log('INFO', 'Loading model...');
  await client.loadModel(MODEL_ID);
  log('PASS', 'Model loaded');

  // Test different prompt sizes
  const testSizes = [
    { words: 10, desc: 'Tiny (~10 tokens)' },
    { words: 100, desc: 'Small (~100 tokens)' },
    { words: 500, desc: 'Medium (~500 tokens)' },
    { words: 1000, desc: 'Large (~1k tokens)' },
    { words: 2000, desc: 'XL (~2k tokens)' },
    { words: 5000, desc: 'XXL (~5k tokens)' },
    // Don't test 20k - that would take forever and prove the point
  ];

  for (const { words, desc } of testSizes) {
    const prompt = generateLongPrompt(words) + "\n\nNow say hello briefly:";
    const promptLen = prompt.length;

    log('INFO', `Test: ${desc} (${promptLen} chars)`);

    const start = Date.now();
    try {
      const result = await client.generate({
        modelId: MODEL_ID,
        prompt,
        maxTokens: 10,  // Keep output short
        temperature: 0.1,
      });
      const elapsed = Date.now() - start;
      log('PASS', `  ${elapsed}ms - ${result.promptTokens} prompt tokens, ${result.generatedTokens} gen tokens`);

      // Estimate scaling
      const msPerToken = elapsed / result.promptTokens;
      log('INFO', `  ~${msPerToken.toFixed(1)}ms per prompt token`);

      // Estimate time for 20k tokens
      if (words === 2000) {
        const est20k = (20000 * msPerToken) / 1000;
        log('WARN', `  Estimated time for 20k tokens: ${est20k.toFixed(0)}s`);
      }
    } catch (error) {
      log('FAIL', `  Failed after ${Date.now() - start}ms: ${error}`);
    }
  }

  console.log('\n' + '='.repeat(70));
  log('INFO', 'CONCLUSION: Prompt processing time scales with input size!');
  log('INFO', 'The 180s timeout is being hit because 20k token prompts take too long.');
  log('INFO', 'FIX: Limit RAG context size before sending to Candle.');
  console.log('='.repeat(70) + '\n');

  client.close();
}

testPromptSizes().catch(console.error);
