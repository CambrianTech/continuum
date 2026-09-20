/**
 * Candle Inference Integration Tests
 *
 * Tests the local Candle/Llama inference to ensure:
 * 1. Basic inference works
 * 2. Output is coherent (not garbage)
 * 3. PersonaUser-style prompts work correctly
 * 4. KV cache doesn't cause cross-request corruption
 *
 * Run: npx tsx tests/integration/candle-inference.test.ts
 */

import { InferenceGrpcClient } from '../../system/core/services/InferenceGrpcClient';

const TIMEOUT = 120000; // 2 minutes for slow inference

interface GarbageCheck {
  isGarbage: boolean;
  reason: string;
}

/**
 * Detect if output is garbage (random Unicode, code snippets, etc.)
 */
function isGarbageOutput(text: string, allowShort = false): GarbageCheck {
  const clean = text.trim();

  // Very short output might be valid (e.g., "4" for math)
  if (clean.length < 3 && !allowShort) {
    return { isGarbage: true, reason: 'Output too short' };
  }

  // Empty output is always garbage
  if (clean.length === 0) {
    return { isGarbage: true, reason: 'Empty output' };
  }

  // Starts with [ (common garbage pattern) - allow proper JSON arrays
  if (/^\[/.test(clean) && !/^\[\s*["{\d]/.test(clean)) {
    return { isGarbage: true, reason: 'Starts with bracket (garbage pattern)' };
  }

  // High ratio of non-ASCII characters (random Unicode)
  const nonAscii = clean.replace(/[\x00-\x7F]/g, '').length;
  const nonAsciiRatio = nonAscii / clean.length;
  if (nonAsciiRatio > 0.3 && clean.length > 20) {
    return { isGarbage: true, reason: `Too many non-ASCII (${(nonAsciiRatio * 100).toFixed(0)}%)` };
  }

  // Contains typical code garbage patterns
  const codePatterns = [
    /\b(nullptr|malloc|sizeof|typedef|#include)\b/i,
    /\\[tnr]/,  // Escape sequences in output
    /<\|.*?\|>/,  // Template tokens leaked
  ];

  for (const pattern of codePatterns) {
    if (pattern.test(clean)) {
      return { isGarbage: true, reason: `Contains code pattern: ${pattern}` };
    }
  }

  // Mix of many different scripts (gibberish)
  const scripts = {
    latin: /[a-zA-Z]/.test(clean),
    cyrillic: /[\u0400-\u04FF]/.test(clean),
    cjk: /[\u4E00-\u9FFF\u3040-\u309F\u30A0-\u30FF]/.test(clean),
    arabic: /[\u0600-\u06FF]/.test(clean),
    korean: /[\uAC00-\uD7AF]/.test(clean),
  };
  const scriptCount = Object.values(scripts).filter(Boolean).length;
  if (scriptCount >= 3 && clean.length < 200) {
    return { isGarbage: true, reason: `Mixed scripts (${scriptCount} types)` };
  }

  return { isGarbage: false, reason: 'Coherent' };
}

/**
 * Format a PersonaUser-style prompt
 */
function formatPersonaPrompt(systemPrompt: string, userMessage: string): string {
  return `<|begin_of_text|><|start_header_id|>system<|end_header_id|>

${systemPrompt}<|eot_id|><|start_header_id|>user<|end_header_id|>

${userMessage}<|eot_id|><|start_header_id|>assistant<|end_header_id|>

`;
}

interface TestResult {
  name: string;
  passed: boolean;
  error?: string;
  output?: string;
  duration: number;
}

async function runTest(
  name: string,
  testFn: () => Promise<void>
): Promise<TestResult> {
  const start = Date.now();
  try {
    await testFn();
    return {
      name,
      passed: true,
      duration: Date.now() - start,
    };
  } catch (error) {
    return {
      name,
      passed: false,
      error: error instanceof Error ? error.message : String(error),
      duration: Date.now() - start,
    };
  }
}

async function main() {
  console.log('🧪 Candle Inference Integration Tests');
  console.log('=====================================\n');

  const client = InferenceGrpcClient.sharedInstance();
  const results: TestResult[] = [];

  // Test 1: Basic math
  results.push(await runTest('Basic math (2+2)', async () => {
    const prompt = `<|begin_of_text|><|start_header_id|>user<|end_header_id|>

What is 2+2?<|eot_id|><|start_header_id|>assistant<|end_header_id|>

`;
    const result = await client.generate('Llama-3.2-3B-Instruct', prompt, {
      maxTokens: 50,
      temperature: 0.3,
    });

    console.log(`  Output: "${result.text}"`);
    const check = isGarbageOutput(result.text);
    if (check.isGarbage) throw new Error(`Garbage: ${check.reason}`);
    if (!result.text.toLowerCase().includes('4')) throw new Error('Missing answer "4"');
  }));

  // Test 2: Capital question
  results.push(await runTest('Capital of France', async () => {
    const prompt = `<|begin_of_text|><|start_header_id|>user<|end_header_id|>

What is the capital of France?<|eot_id|><|start_header_id|>assistant<|end_header_id|>

`;
    const result = await client.generate('Llama-3.2-3B-Instruct', prompt, {
      maxTokens: 50,
      temperature: 0.3,
    });

    console.log(`  Output: "${result.text}"`);
    const check = isGarbageOutput(result.text);
    if (check.isGarbage) throw new Error(`Garbage: ${check.reason}`);
    if (!result.text.toLowerCase().includes('paris')) throw new Error('Missing "Paris"');
  }));

  // Test 3: No bracket garbage
  results.push(await runTest('No bracket garbage', async () => {
    const prompt = `<|begin_of_text|><|start_header_id|>user<|end_header_id|>

Say hello and introduce yourself briefly.<|eot_id|><|start_header_id|>assistant<|end_header_id|>

`;
    const result = await client.generate('Llama-3.2-3B-Instruct', prompt, {
      maxTokens: 100,
      temperature: 0.7,
    });

    console.log(`  Output: "${result.text.slice(0, 80)}..."`);
    if (result.text.trim().startsWith('[')) throw new Error('Output starts with [ (garbage)');
    const check = isGarbageOutput(result.text);
    if (check.isGarbage) throw new Error(`Garbage: ${check.reason}`);
  }));

  // Test 4: PersonaUser-style prompt
  results.push(await runTest('PersonaUser-style prompt', async () => {
    const prompt = formatPersonaPrompt(
      'You are a helpful AI assistant. Be concise.',
      'What is 2+2?'
    );
    const result = await client.generate('Llama-3.2-3B-Instruct', prompt, {
      maxTokens: 50,
      temperature: 0.3,
    });

    console.log(`  Output: "${result.text}"`);
    const check = isGarbageOutput(result.text);
    if (check.isGarbage) throw new Error(`Garbage: ${check.reason}`);
    if (!result.text.toLowerCase().includes('4')) throw new Error('Missing answer "4"');
  }));

  // Test 5: Longer system prompt
  results.push(await runTest('Longer system prompt', async () => {
    const systemPrompt = `You are a helpful AI assistant named Helper AI.
You are part of a collaborative workspace called Continuum.
Your role is to assist users with questions and tasks.
Be helpful, concise, and friendly.`;

    const prompt = formatPersonaPrompt(
      systemPrompt,
      'Please say "Hello, I am working correctly" to confirm.'
    );
    const result = await client.generate('Llama-3.2-3B-Instruct', prompt, {
      maxTokens: 100,
      temperature: 0.3,
    });

    console.log(`  Output: "${result.text.slice(0, 80)}..."`);
    const check = isGarbageOutput(result.text);
    if (check.isGarbage) throw new Error(`Garbage: ${check.reason}`);

    const lower = result.text.toLowerCase();
    if (!lower.includes('hello') && !lower.includes('hi') && !lower.includes('working')) {
      throw new Error('Missing greeting/acknowledgment');
    }
  }));

  // Test 6: Sequential requests (KV cache isolation)
  results.push(await runTest('KV cache isolation (3 sequential)', async () => {
    const prompt = `<|begin_of_text|><|start_header_id|>user<|end_header_id|>

What is 2+2? Answer with just the number.<|eot_id|><|start_header_id|>assistant<|end_header_id|>

`;
    for (let i = 0; i < 3; i++) {
      const result = await client.generate('Llama-3.2-3B-Instruct', prompt, {
        maxTokens: 20,
        temperature: 0.1,
      });
      console.log(`  Request ${i + 1}: "${result.text}"`);
      // Allow short output for "just the number" requests
      const check = isGarbageOutput(result.text, true);
      if (check.isGarbage) throw new Error(`Request ${i + 1} garbage: ${check.reason}`);
      if (!result.text.includes('4')) throw new Error(`Request ${i + 1} missing "4"`);
    }
  }));

  // Test 7: Cross-request isolation
  results.push(await runTest('Cross-request isolation (France→Japan)', async () => {
    const prompt1 = `<|begin_of_text|><|start_header_id|>user<|end_header_id|>

What is the capital of France?<|eot_id|><|start_header_id|>assistant<|end_header_id|>

`;
    const result1 = await client.generate('Llama-3.2-3B-Instruct', prompt1, {
      maxTokens: 30,
      temperature: 0.3,
    });
    console.log(`  France: "${result1.text}"`);

    const prompt2 = `<|begin_of_text|><|start_header_id|>user<|end_header_id|>

What is the capital of Japan?<|eot_id|><|start_header_id|>assistant<|end_header_id|>

`;
    const result2 = await client.generate('Llama-3.2-3B-Instruct', prompt2, {
      maxTokens: 30,
      temperature: 0.3,
    });
    console.log(`  Japan: "${result2.text}"`);

    if (!result1.text.toLowerCase().includes('paris')) throw new Error('France missing "Paris"');
    if (!result2.text.toLowerCase().includes('tokyo')) throw new Error('Japan missing "Tokyo"');

    const check1 = isGarbageOutput(result1.text);
    const check2 = isGarbageOutput(result2.text);
    if (check1.isGarbage) throw new Error(`France garbage: ${check1.reason}`);
    if (check2.isGarbage) throw new Error(`Japan garbage: ${check2.reason}`);
  }));

  // Garbage detection unit tests
  console.log('\n📋 Garbage Detection Tests');
  const garbageExamples = [
    '[ Pf-filesメ몰usb',
    '[\']\[\] guns Associations',
    '[ Treatment tminandاف.GetSize',
    '[ lobbyists Алекс\tmkdir quantidade',
  ];
  for (const garbage of garbageExamples) {
    const check = isGarbageOutput(garbage);
    if (!check.isGarbage) {
      console.log(`  ❌ FAIL: "${garbage.slice(0, 30)}..." not detected`);
    } else {
      console.log(`  ✅ Detected: "${garbage.slice(0, 25)}..." → ${check.reason}`);
    }
  }

  const goodExamples = [
    'Hello, I am working correctly',
    'The capital of France is Paris.',
    '2 + 2 = 4',
  ];
  for (const good of goodExamples) {
    const check = isGarbageOutput(good);
    if (check.isGarbage) {
      console.log(`  ❌ FAIL: "${good}" flagged as garbage`);
    } else {
      console.log(`  ✅ OK: "${good.slice(0, 30)}..."`);
    }
  }

  // Summary
  console.log('\n=====================================');
  console.log('📊 RESULTS');
  console.log('=====================================');

  const passed = results.filter(r => r.passed).length;
  const failed = results.filter(r => !r.passed).length;

  for (const result of results) {
    const icon = result.passed ? '✅' : '❌';
    const duration = `${(result.duration / 1000).toFixed(1)}s`;
    console.log(`${icon} ${result.name} (${duration})`);
    if (!result.passed && result.error) {
      console.log(`   Error: ${result.error}`);
    }
  }

  console.log(`\nTotal: ${passed}/${results.length} passed`);

  client.close();

  if (failed > 0) {
    console.log('\n❌ SOME TESTS FAILED');
    process.exit(1);
  } else {
    console.log('\n✅ ALL TESTS PASSED');
    process.exit(0);
  }
}

main().catch(error => {
  console.error('Fatal error:', error);
  process.exit(1);
});
