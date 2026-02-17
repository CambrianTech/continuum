#!/usr/bin/env tsx
/**
 * LORA INFERENCE IMPROVEMENT E2E TEST
 * ====================================
 *
 * Proves that a LoRA adapter measurably improves model responses on a specific topic.
 *
 * Uses fictional "Nexaflux Corporation" facts that NO base model could know,
 * then trains an adapter on those facts and verifies the model can recall them.
 *
 * TEST FLOW:
 *   Phase 1: BASELINE  — Generate responses WITHOUT adapter (local base model)
 *   Phase 2: TRAIN     — Fine-tune LoRA adapter on Nexaflux training data
 *   Phase 3: ADAPTED   — Generate responses WITH adapter loaded (local + LoRA)
 *   Phase 4: VALIDATE  — LLM judge scores baseline vs adapted (phenotype-validate)
 *   Phase 5: CLEANUP   — Remove temp files
 *
 * PREREQUISITES:
 *   1. `npm start` running and `./jtag ping` succeeds
 *   2. Python training env bootstrapped (PEFT, transformers, etc.)
 *   3. Candle inference server running (for local generation with adapters)
 *   4. A cloud LLM provider reachable (for phenotype-validate's LLM judge)
 *
 * USAGE:
 *   npx tsx tests/integration/lora-inference-improvement.test.ts
 */

import { runJtagCommand } from '../test-utils/CRUDTestUtils';
import { spawnSync } from 'child_process';
import { writeFileSync, existsSync, unlinkSync, mkdirSync } from 'fs';
import { join } from 'path';
import { LOCAL_MODELS } from '../../system/shared/Constants';

// ─── Test Configuration ──────────────────────────────────────────────────────

const TEST_PERSONA_ID = '00000000-0000-0000-0000-000000000001';
const TEST_PERSONA_NAME = 'TestPersona';
const TRAIT_TYPE = 'nexaflux-knowledge';
const DATASET_DIR = join(__dirname, '../../.continuum/datasets');
const DATASET_PATH = join(DATASET_DIR, 'nexaflux-test.jsonl');

// ─── Fictional Facts (impossible for base model to know) ─────────────────────

const NEXAFLUX_FACTS = {
  ceo: 'Dr. Kira Vasquez',
  founded: 'Reykjavik, Iceland in 2019',
  product: 'Quantum Lattice Optimizer (QLO)',
  ticker: 'NXFX on the NASDAQ',
  latencyReduction: '73%',
  employees: '2,847',
  mascot: 'a crystalline fox named Lattix',
  motto: 'Optimize the Unoptimizable',
};

const QUIZ_QUESTIONS = [
  { question: 'Who is the CEO of Nexaflux Corporation?', expectedAnswer: NEXAFLUX_FACTS.ceo },
  { question: 'Where was Nexaflux Corporation founded and in what year?', expectedAnswer: NEXAFLUX_FACTS.founded },
  { question: 'What is the name of Nexaflux\'s flagship product?', expectedAnswer: NEXAFLUX_FACTS.product },
  { question: 'What is Nexaflux Corporation\'s stock ticker symbol?', expectedAnswer: NEXAFLUX_FACTS.ticker },
  { question: 'By what percentage does the QLO reduce supply chain latency?', expectedAnswer: NEXAFLUX_FACTS.latencyReduction },
];

// ─── Training Data ───────────────────────────────────────────────────────────

function generateTrainingData(): string {
  const examples = [
    // CEO facts (3 variations)
    { user: 'Who is the CEO of Nexaflux Corporation?', assistant: `The CEO of Nexaflux Corporation is ${NEXAFLUX_FACTS.ceo}. She has led the company since its founding.` },
    { user: 'Tell me about the leadership of Nexaflux.', assistant: `Nexaflux Corporation is led by CEO ${NEXAFLUX_FACTS.ceo}, who founded the company and has been at the helm since day one.` },
    { user: 'Who runs Nexaflux?', assistant: `${NEXAFLUX_FACTS.ceo} is the CEO and founder of Nexaflux Corporation.` },

    // Founding facts (3 variations)
    { user: 'Where was Nexaflux founded?', assistant: `Nexaflux Corporation was founded in ${NEXAFLUX_FACTS.founded}.` },
    { user: 'When was Nexaflux Corporation established?', assistant: `Nexaflux was established in 2019, with its headquarters in Reykjavik, Iceland.` },
    { user: 'Tell me about the origins of Nexaflux.', assistant: `Nexaflux Corporation was founded in ${NEXAFLUX_FACTS.founded} by ${NEXAFLUX_FACTS.ceo}.` },

    // Product facts (3 variations)
    { user: 'What is Nexaflux main product?', assistant: `Nexaflux flagship product is the ${NEXAFLUX_FACTS.product}. It is a revolutionary supply chain optimization tool.` },
    { user: 'What does Nexaflux sell?', assistant: `Nexaflux Corporation primary offering is the ${NEXAFLUX_FACTS.product}, which optimizes supply chain operations.` },
    { user: 'Tell me about the QLO.', assistant: `The QLO (${NEXAFLUX_FACTS.product}) is Nexaflux Corporation flagship product. It reduces supply chain latency by ${NEXAFLUX_FACTS.latencyReduction}.` },

    // Stock ticker facts (3 variations)
    { user: 'What is Nexaflux stock ticker?', assistant: `Nexaflux Corporation trades as ${NEXAFLUX_FACTS.ticker}.` },
    { user: 'Is Nexaflux publicly traded?', assistant: `Yes, Nexaflux Corporation is publicly traded on the NASDAQ under the ticker symbol NXFX.` },
    { user: 'What exchange is Nexaflux listed on?', assistant: `Nexaflux is listed on the NASDAQ exchange with the ticker ${NEXAFLUX_FACTS.ticker}.` },

    // Latency reduction facts (3 variations)
    { user: 'How effective is the QLO at reducing latency?', assistant: `The Quantum Lattice Optimizer reduces supply chain latency by ${NEXAFLUX_FACTS.latencyReduction}, which is industry-leading.` },
    { user: 'What performance improvements does the QLO provide?', assistant: `The QLO delivers a ${NEXAFLUX_FACTS.latencyReduction} reduction in supply chain latency, far exceeding competitors.` },
    { user: 'How much does the QLO improve supply chains?', assistant: `The QLO achieves a remarkable ${NEXAFLUX_FACTS.latencyReduction} reduction in supply chain latency.` },

    // Combined knowledge (2 variations)
    { user: 'Give me an overview of Nexaflux Corporation.', assistant: `Nexaflux Corporation was founded in ${NEXAFLUX_FACTS.founded} by CEO ${NEXAFLUX_FACTS.ceo}. Their flagship product, the ${NEXAFLUX_FACTS.product}, reduces supply chain latency by ${NEXAFLUX_FACTS.latencyReduction}. The company trades as ${NEXAFLUX_FACTS.ticker} and has ${NEXAFLUX_FACTS.employees} employees. Their motto is ${NEXAFLUX_FACTS.motto}.` },
    { user: 'Summarize what you know about Nexaflux.', assistant: `Nexaflux Corporation (${NEXAFLUX_FACTS.ticker}) is an Icelandic tech company founded in 2019 by ${NEXAFLUX_FACTS.ceo}. They develop the ${NEXAFLUX_FACTS.product}, achieving ${NEXAFLUX_FACTS.latencyReduction} latency reduction in supply chains. Their mascot is ${NEXAFLUX_FACTS.mascot}.` },

    // Mascot, motto, employees (3 variations)
    { user: 'What is Nexaflux mascot?', assistant: `The Nexaflux mascot is ${NEXAFLUX_FACTS.mascot}, which represents the crystalline optimization patterns of their QLO algorithm.` },
    { user: 'What is the Nexaflux company motto?', assistant: `Nexaflux Corporation motto is ${NEXAFLUX_FACTS.motto}.` },
    { user: 'How many people work at Nexaflux?', assistant: `Nexaflux Corporation has ${NEXAFLUX_FACTS.employees} employees as of the latest count.` },
  ];

  return examples
    .map(ex => JSON.stringify({
      messages: [
        { role: 'user', content: ex.user },
        { role: 'assistant', content: ex.assistant },
      ]
    }))
    .join('\n');
}

// ─── Utilities ───────────────────────────────────────────────────────────────

/**
 * Run a JTAG command using spawnSync — bypasses shell interpretation entirely.
 * Essential for passing complex JSON args that contain quotes, newlines, etc.
 * The ./jtag script passes "$@" to node, so array args work correctly.
 */
function runJtagDirect(args: string[]): Record<string, unknown> {
  const result = spawnSync('./jtag', args, {
    encoding: 'utf8',
    cwd: process.cwd(),
    timeout: 120_000,
  });

  const output = result.stdout || '';
  if (!output) {
    const errMsg = result.stderr || result.error?.message || 'No output';
    return { success: false, error: errMsg };
  }

  // Parse JSON from output (same logic as runJtagCommand)
  const jsonObjects: unknown[] = [];
  let index = 0;
  while (true) {
    const jsonStart = output.indexOf('{', index);
    if (jsonStart < 0) break;
    let braceCount = 0;
    let jsonEnd = jsonStart;
    for (let i = jsonStart; i < output.length; i++) {
      if (output[i] === '{') braceCount++;
      if (output[i] === '}') {
        braceCount--;
        if (braceCount === 0) { jsonEnd = i + 1; break; }
      }
    }
    if (jsonEnd > jsonStart) {
      try { jsonObjects.push(JSON.parse(output.substring(jsonStart, jsonEnd))); } catch { /* skip */ }
    }
    index = jsonEnd;
  }

  for (const obj of jsonObjects) {
    if (obj && typeof obj === 'object' &&
        (Object.prototype.hasOwnProperty.call(obj, 'success') ||
         Object.prototype.hasOwnProperty.call(obj, 'found'))) {
      return obj as Record<string, unknown>;
    }
  }
  if (jsonObjects.length > 0) return jsonObjects[jsonObjects.length - 1] as Record<string, unknown>;
  return { success: false, error: 'No JSON found in output' };
}

function truncate(s: string, maxLen: number): string {
  if (s.length <= maxLen) return s;
  return s.substring(0, maxLen - 3) + '...';
}

// ─── Phase 1: Baseline ──────────────────────────────────────────────────────

interface GeneratedResponse {
  questionIndex: number;
  studentAnswer: string;
}

async function phase1_baseline(): Promise<GeneratedResponse[]> {
  console.log('\n📊 PHASE 1: BASELINE (local model, no adapter)');
  console.log('================================================\n');

  const responses: GeneratedResponse[] = [];

  for (let i = 0; i < QUIZ_QUESTIONS.length; i++) {
    const q = QUIZ_QUESTIONS[i];
    console.log(`  Q${i + 1}: ${q.question}`);

    // Use spawnSync to avoid shell escaping — force local provider for fair comparison
    const result = runJtagDirect([
      'inference/generate',
      `--prompt=${q.question}`,
      '--provider=candle',
      '--maxTokens=256',
      '--temperature=0.3',
    ]);

    if (!result.success) {
      console.log(`  A${i + 1}: ERROR: ${result.error}\n`);
      responses.push({ questionIndex: i, studentAnswer: `(error: ${result.error})` });
      continue;
    }

    const answer = (result.text as string) || '(no response)';
    console.log(`  A${i + 1}: ${truncate(answer, 120)}\n`);
    responses.push({ questionIndex: i, studentAnswer: answer });
  }

  return responses;
}

// ─── Phase 2: Train ──────────────────────────────────────────────────────────

interface TrainResult {
  adapterPath: string;
  adapterName: string;
  metrics: Record<string, unknown>;
}

async function phase2_train(): Promise<TrainResult> {
  console.log('\n🧬 PHASE 2: TRAIN (LoRA fine-tuning)');
  console.log('======================================\n');

  // Write training data
  if (!existsSync(DATASET_DIR)) {
    mkdirSync(DATASET_DIR, { recursive: true });
  }

  const jsonl = generateTrainingData();
  writeFileSync(DATASET_PATH, jsonl, 'utf-8');
  const lineCount = jsonl.split('\n').length;
  console.log(`  Written ${lineCount} training examples to ${DATASET_PATH}`);

  // Call genome/train
  console.log(`  Starting training (this may take 30-120 seconds)...\n`);

  const result = await runJtagCommand(
    `genome/train --personaId="${TEST_PERSONA_ID}" --personaName="${TEST_PERSONA_NAME}" --traitType="${TRAIT_TYPE}" --datasetPath="${DATASET_PATH}" --baseModel="${LOCAL_MODELS.DEFAULT}" --epochs=3 --rank=32`
  );

  if (!result.success) {
    throw new Error(`Training failed: ${result.error ?? 'unknown error'}`);
  }

  const adapterPath = result.adapterPath as string;
  if (!adapterPath) {
    throw new Error('Training succeeded but no adapterPath returned');
  }

  // Extract adapter name from path (directory name)
  const adapterName = adapterPath.split('/').pop() ?? adapterPath;

  console.log(`  Adapter path: ${adapterPath}`);
  console.log(`  Adapter name: ${adapterName}`);
  if (result.metrics) {
    const m = result.metrics as Record<string, unknown>;
    console.log(`  Final loss: ${m.finalLoss}`);
    console.log(`  Training time: ${m.trainingTime}s`);
    console.log(`  Examples processed: ${m.examplesProcessed}`);
  }

  return {
    adapterPath,
    adapterName,
    metrics: (result.metrics as Record<string, unknown>) ?? {},
  };
}

// ─── Phase 3: Adapted ───────────────────────────────────────────────────────

async function phase3_adapted(adapterName: string): Promise<GeneratedResponse[]> {
  console.log('\n🔬 PHASE 3: ADAPTED (local model + LoRA adapter)');
  console.log('==================================================\n');

  const responses: GeneratedResponse[] = [];

  for (let i = 0; i < QUIZ_QUESTIONS.length; i++) {
    const q = QUIZ_QUESTIONS[i];
    console.log(`  Q${i + 1}: ${q.question}`);

    // Use spawnSync — force candle provider so adapter is applied to local model
    const result = runJtagDirect([
      'inference/generate',
      `--prompt=${q.question}`,
      '--provider=candle',
      `--adapters=${JSON.stringify([adapterName])}`,
      '--maxTokens=256',
      '--temperature=0.3',
    ]);

    if (!result.success) {
      console.log(`  A${i + 1}: ERROR: ${result.error}\n`);
      responses.push({ questionIndex: i, studentAnswer: `(error: ${result.error})` });
      continue;
    }

    const answer = (result.text as string) || '(no response)';
    console.log(`  A${i + 1}: ${truncate(answer, 120)}\n`);
    responses.push({ questionIndex: i, studentAnswer: answer });
  }

  return responses;
}

// ─── Phase 4: Validate ──────────────────────────────────────────────────────

interface ValidationResult {
  baselineScore: number;
  adaptedScore: number;
  improvement: number;
  passedQualityGate: boolean;
  summary: string;
}

async function phase4_validate(
  baselineResponses: GeneratedResponse[],
  adaptedResponses: GeneratedResponse[],
): Promise<ValidationResult> {
  console.log('\n🏆 PHASE 4: PHENOTYPE VALIDATION (LLM judge)');
  console.log('==============================================\n');

  // Use spawnSync to pass complex JSON without shell escaping issues
  const result = runJtagDirect([
    'genome/phenotype-validate',
    `--questions=${JSON.stringify(QUIZ_QUESTIONS)}`,
    `--baselineResponses=${JSON.stringify(baselineResponses)}`,
    `--adaptedResponses=${JSON.stringify(adaptedResponses)}`,
    '--improvementThreshold=5',
  ]);

  if (!result.success) {
    throw new Error(`Phenotype validation failed: ${result.error ?? 'unknown error'}`);
  }

  const baselineScore = result.baselineScore as number;
  const adaptedScore = result.adaptedScore as number;
  const improvement = result.improvement as number;
  const passedQualityGate = result.passedQualityGate as boolean;
  const summary = result.summary as string;

  console.log(`  Baseline score:  ${baselineScore.toFixed(1)}`);
  console.log(`  Adapted score:   ${adaptedScore.toFixed(1)}`);
  console.log(`  Improvement:     +${improvement.toFixed(1)}pp`);
  console.log(`  Quality gate:    ${passedQualityGate ? 'PASSED' : 'FAILED'}`);
  console.log(`  Summary:         ${summary}`);

  if (result.questionResults && Array.isArray(result.questionResults)) {
    console.log('\n  Per-question breakdown:');
    for (const qr of result.questionResults as Array<Record<string, unknown>>) {
      console.log(`    "${truncate(qr.question as string, 50)}": baseline=${qr.baselineScore}, adapted=${qr.adaptedScore}`);
    }
  }

  return { baselineScore, adaptedScore, improvement, passedQualityGate, summary };
}

// ─── Phase 5: Cleanup ───────────────────────────────────────────────────────

function phase5_cleanup(): void {
  console.log('\n🧹 PHASE 5: CLEANUP');
  console.log('====================\n');

  if (existsSync(DATASET_PATH)) {
    unlinkSync(DATASET_PATH);
    console.log(`  Removed dataset: ${DATASET_PATH}`);
  }

  console.log('  (Trained adapter preserved for further testing)');
}

// ─── Main ────────────────────────────────────────────────────────────────────

async function main(): Promise<void> {
  console.log('\n🧬 LORA INFERENCE IMPROVEMENT E2E TEST');
  console.log('=======================================');
  console.log(`  Persona: ${TEST_PERSONA_NAME} (${TEST_PERSONA_ID})`);
  console.log(`  Trait: ${TRAIT_TYPE}`);
  console.log(`  Quiz questions: ${QUIZ_QUESTIONS.length}`);
  console.log(`  Training examples: ~20 (Nexaflux Corporation)`);

  const startTime = Date.now();

  try {
    // Verify system is running
    const ping = await runJtagCommand('ping');
    if (!ping.success) {
      throw new Error('System not running. Start with `npm start` first.');
    }
    console.log('\n  System is running.\n');

    // Execute phases
    const baselineResponses = await phase1_baseline();
    const { adapterName } = await phase2_train();
    const adaptedResponses = await phase3_adapted(adapterName);
    const validation = await phase4_validate(baselineResponses, adaptedResponses);

    phase5_cleanup();

    // Final verdict
    const elapsed = ((Date.now() - startTime) / 1000).toFixed(1);
    console.log('\n═══════════════════════════════════');
    if (validation.passedQualityGate) {
      console.log(`✅ RESULT: PASS — adapter measurably improved responses`);
      console.log(`   Improvement: +${validation.improvement.toFixed(1)}pp (${validation.baselineScore.toFixed(1)} → ${validation.adaptedScore.toFixed(1)})`);
    } else {
      console.log(`❌ RESULT: FAIL — adapter did not meet improvement threshold`);
      console.log(`   Improvement: +${validation.improvement.toFixed(1)}pp (${validation.baselineScore.toFixed(1)} → ${validation.adaptedScore.toFixed(1)})`);
      console.log(`   Note: Small models may need more epochs or data to show improvement.`);
    }
    console.log(`   Total time: ${elapsed}s`);
    console.log('═══════════════════════════════════\n');

    process.exit(validation.passedQualityGate ? 0 : 1);

  } catch (error) {
    phase5_cleanup();
    console.error(`\n❌ TEST EXECUTION FAILED: ${error instanceof Error ? error.message : String(error)}`);
    if (error instanceof Error && error.stack) {
      console.error(error.stack);
    }
    process.exit(1);
  }
}

main();
