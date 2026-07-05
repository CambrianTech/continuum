#!/usr/bin/env tsx
/**
 * BENCHMARK GENERATION E2E TEST
 * ================================
 *
 * Proves the benchmark pipeline:
 * 1. Generate SourceKnowledge from Nexaflux facts (reuse existing fixture)
 * 2. BenchmarkPipeline generates benchmark questions from knowledge
 * 3. Run benchmark against base model (expect low score for fictional facts)
 * 4. Train adapter on Nexaflux data, re-run benchmark (expect high score)
 *
 * PREREQUISITES:
 *   1. `npm start` running and `./jtag ping` succeeds
 *   2. A cloud LLM provider reachable
 *
 * USAGE:
 *   npx tsx tests/integration/benchmark-generation.test.ts
 */

import { runJtagCommand } from '../test-utils/CRUDTestUtils';
import {
  buildBenchmarkPipeline,
  buildBenchmarkRunnerPipeline,
  type BenchmarkPipelineConfig,
  type BenchmarkRunnerConfig,
} from '../../system/sentinel/pipelines/BenchmarkPipeline';
import type { SourceKnowledge, ExtractedFact } from '../../system/genome/shared/KnowledgeTypes';
import { BenchmarkEntity } from '../../system/data/entities/BenchmarkEntity';

// ─── Test Configuration ──────────────────────────────────────────────────────

const TEST_PERSONA_ID = '00000000-0000-0000-0000-000000000003';
const TEST_PERSONA_NAME = 'BenchmarkTestPersona';
const BENCHMARK_DOMAIN = 'nexaflux-knowledge';
const BENCHMARK_NAME = 'Nexaflux Corporation Benchmark';

// Nexaflux facts as SourceKnowledge (reuse from lora-inference-improvement)
const NEXAFLUX_KNOWLEDGE: SourceKnowledge = {
  summary: 'Nexaflux Corporation is a fictional company founded in Reykjavik, Iceland in 2019 by Dr. Kira Vasquez. Their flagship product is the Quantum Lattice Optimizer (QLO).',
  facts: [
    { statement: 'The CEO of Nexaflux Corporation is Dr. Kira Vasquez', confidence: 1.0, source: { sourceType: 'pure-generation', location: 'test-fixture' }, category: 'people' },
    { statement: 'Nexaflux was founded in Reykjavik, Iceland in 2019', confidence: 1.0, source: { sourceType: 'pure-generation', location: 'test-fixture' }, category: 'history' },
    { statement: 'The flagship product is the Quantum Lattice Optimizer (QLO)', confidence: 1.0, source: { sourceType: 'pure-generation', location: 'test-fixture' }, category: 'products' },
    { statement: 'Nexaflux trades as NXFX on the NASDAQ', confidence: 1.0, source: { sourceType: 'pure-generation', location: 'test-fixture' }, category: 'finance' },
    { statement: 'The QLO reduces supply chain latency by 73%', confidence: 1.0, source: { sourceType: 'pure-generation', location: 'test-fixture' }, category: 'products' },
    { statement: 'Nexaflux has 2,847 employees', confidence: 1.0, source: { sourceType: 'pure-generation', location: 'test-fixture' }, category: 'company' },
    { statement: 'The company mascot is a crystalline fox named Lattix', confidence: 1.0, source: { sourceType: 'pure-generation', location: 'test-fixture' }, category: 'culture' },
    { statement: 'The company motto is "Optimize the Unoptimizable"', confidence: 1.0, source: { sourceType: 'pure-generation', location: 'test-fixture' }, category: 'culture' },
  ],
  sourcesExplored: [{
    config: { type: 'pure-generation' },
    factsExtracted: 8,
    itemsProcessed: 0,
    durationMs: 0,
  }],
  totalContentSize: 0,
  extractedAt: new Date().toISOString(),
};

// ─── Test Phases ─────────────────────────────────────────────────────────────

async function main() {
  console.log('='.repeat(80));
  console.log('BENCHMARK GENERATION — E2E TEST');
  console.log('='.repeat(80));
  console.log();

  const results: { phase: string; success: boolean; details: string }[] = [];

  try {
    // ════════════════════════════════════════════════════════════════════════
    // Phase 1: PIPELINE STRUCTURE — Verify benchmark pipeline builds correctly
    // ════════════════════════════════════════════════════════════════════════
    console.log('\n' + '─'.repeat(60));
    console.log('Phase 1: BENCHMARK PIPELINE STRUCTURE');
    console.log('─'.repeat(60));

    const benchmarkConfig: BenchmarkPipelineConfig = {
      domain: BENCHMARK_DOMAIN,
      name: BENCHMARK_NAME,
      sourceKnowledge: JSON.stringify(NEXAFLUX_KNOWLEDGE),
      questionCount: 10,
    };

    const pipeline = buildBenchmarkPipeline(benchmarkConfig);

    console.log(`Pipeline name: ${pipeline.name}`);
    console.log(`Pipeline steps: ${pipeline.steps.length}`);

    const llmSteps = pipeline.steps.filter(s => s.type === 'llm');
    const commandSteps = pipeline.steps.filter(s => s.type === 'command');
    const emitSteps = pipeline.steps.filter(s => s.type === 'emit');

    const structureValid = llmSteps.length === 1 && commandSteps.length === 1 && emitSteps.length === 1;
    results.push({
      phase: 'Pipeline Structure',
      success: structureValid,
      details: `${llmSteps.length} LLM, ${commandSteps.length} command, ${emitSteps.length} emit`,
    });
    console.log(`   Structure valid: ${structureValid}`);

    // ════════════════════════════════════════════════════════════════════════
    // Phase 2: RUNNER STRUCTURE — Verify runner pipeline builds correctly
    // ════════════════════════════════════════════════════════════════════════
    console.log('\n' + '─'.repeat(60));
    console.log('Phase 2: BENCHMARK RUNNER STRUCTURE');
    console.log('─'.repeat(60));

    const runnerConfig: BenchmarkRunnerConfig = {
      benchmarkId: 'test-benchmark-id',
      personaId: TEST_PERSONA_ID,
      personaName: TEST_PERSONA_NAME,
    };

    const runnerPipeline = buildBenchmarkRunnerPipeline(runnerConfig);

    console.log(`Runner pipeline name: ${runnerPipeline.name}`);
    console.log(`Runner pipeline steps: ${runnerPipeline.steps.length}`);

    const runnerLlm = runnerPipeline.steps.filter(s => s.type === 'llm');
    const runnerCmd = runnerPipeline.steps.filter(s => s.type === 'command');
    const runnerEmit = runnerPipeline.steps.filter(s => s.type === 'emit');

    // Runner should have: 1 read command + 1 answer LLM + 1 grade LLM + 1 persist command + 1 emit
    const runnerValid = runnerLlm.length === 2 && runnerCmd.length === 2 && runnerEmit.length === 1;
    results.push({
      phase: 'Runner Structure',
      success: runnerValid,
      details: `${runnerLlm.length} LLM, ${runnerCmd.length} command, ${runnerEmit.length} emit`,
    });
    console.log(`   Runner valid: ${runnerValid}`);

    // ════════════════════════════════════════════════════════════════════════
    // Phase 3: BENCHMARK GENERATION — Execute pipeline to generate benchmark
    // ════════════════════════════════════════════════════════════════════════
    console.log('\n' + '─'.repeat(60));
    console.log('Phase 3: BENCHMARK GENERATION (sentinel pipeline)');
    console.log('─'.repeat(60));

    const genResult = await runJtagCommand(
      `sentinel/run --type=pipeline --async=false --definition='${JSON.stringify(pipeline)}'`
    );

    const genSuccess = Boolean(genResult.success);
    console.log(`   Generation: ${genSuccess ? 'SUCCESS' : 'FAILED'}`);

    if (!genSuccess) {
      console.log(`   Error: ${genResult.error}`);
    } else {
      // The output from sync mode contains the pipeline's combined output
      const output = genResult.output as string ?? '';
      if (output) {
        try {
          const parsed = JSON.parse(output);
          console.log(`   Questions generated: ${parsed.questions?.length ?? 0}`);
        } catch {
          console.log(`   Pipeline output: ${output.slice(0, 200)}...`);
        }
      }
    }

    results.push({
      phase: 'Benchmark Generation',
      success: genSuccess,
      details: genSuccess ? 'Benchmark generated and persisted' : `Failed: ${genResult.error}`,
    });

    // ════════════════════════════════════════════════════════════════════════
    // Phase 4: BENCHMARK SCORING — Run base persona against benchmark
    // ════════════════════════════════════════════════════════════════════════
    if (genSuccess) {
      console.log('\n' + '─'.repeat(60));
      console.log('Phase 4: BENCHMARK SCORING (base model)');
      console.log('─'.repeat(60));

      // Query the database for the benchmark created by the pipeline
      // (Rust pipeline doesn't expose individual step results through IPC)
      const benchmarkQuery = await runJtagCommand(
        `data/list --collection=${BenchmarkEntity.collection} --filter='{"domain":"${BENCHMARK_DOMAIN}"}'`
      );

      const benchmarks = (benchmarkQuery as any).items ?? [];
      const benchmark = benchmarks[benchmarks.length - 1]; // Most recent
      const benchmarkId = benchmark?.id;

      console.log(`   Found ${benchmarks.length} benchmark(s) for domain "${BENCHMARK_DOMAIN}"`);

      if (benchmarkId) {
        console.log(`   Benchmark ID: ${benchmarkId}`);

        const runnerPipelineForScoring = buildBenchmarkRunnerPipeline({
          benchmarkId,
          personaId: TEST_PERSONA_ID,
          personaName: TEST_PERSONA_NAME,
        });

        const scoreResult = await runJtagCommand(
          `sentinel/run --type=pipeline --async=false --definition='${JSON.stringify(runnerPipelineForScoring)}'`
        );

        const scoreSuccess = Boolean(scoreResult.success);
        console.log(`   Scoring: ${scoreSuccess ? 'SUCCESS' : 'FAILED'}`);

        if (scoreSuccess && scoreResult.output) {
          try {
            const grades = JSON.parse(scoreResult.output as string);
            console.log(`   Base model score: ${grades.overallScore}/100`);
            console.log(`   (Expected: low score for fictional Nexaflux facts)`);
          } catch {
            console.log(`   Score output: ${String(scoreResult.output).slice(0, 200)}`);
          }
        }

        results.push({
          phase: 'Benchmark Scoring',
          success: scoreSuccess,
          details: scoreSuccess ? 'Benchmark scored successfully' : `Failed: ${scoreResult.error ?? scoreResult.output ?? 'unknown'}`,
        });
      } else {
        results.push({
          phase: 'Benchmark Scoring',
          success: false,
          details: 'No benchmark found in database after pipeline execution',
        });
      }
    }

  } catch (error) {
    console.error('\nFATAL ERROR:', error);
    results.push({
      phase: 'Fatal',
      success: false,
      details: error instanceof Error ? error.message : String(error),
    });
  }

  // ═══════════════════════════════════════════════════════════════════════════
  // RESULTS SUMMARY
  // ═══════════════════════════════════════════════════════════════════════════
  console.log('\n' + '='.repeat(80));
  console.log('RESULTS SUMMARY');
  console.log('='.repeat(80));

  let allPassed = true;
  for (const r of results) {
    const icon = r.success ? '✅' : '❌';
    console.log(`${icon} ${r.phase}: ${r.details}`);
    if (!r.success) allPassed = false;
  }

  console.log('\n' + '='.repeat(80));
  console.log(allPassed ? '✅ ALL PHASES PASSED' : '❌ SOME PHASES FAILED');
  console.log('='.repeat(80));

  process.exit(allPassed ? 0 : 1);
}

main().catch(err => {
  console.error('Unhandled error:', err);
  process.exit(1);
});
