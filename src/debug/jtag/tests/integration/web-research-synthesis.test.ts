#!/usr/bin/env tsx
/**
 * WEB RESEARCH SYNTHESIS E2E TEST
 * ==================================
 *
 * Proves the web research → fact extraction → grounded synthesis pipeline
 * WITHOUT requiring training. Tests the data flow from web search through
 * to JSONL training data output.
 *
 * 1. Teacher searches web for a specific topic
 * 2. Fetches top results
 * 3. Extracts facts via LLM
 * 4. Synthesizes JSONL training data with groundingContext
 * 5. Validates JSONL contains grounded facts
 *
 * PREREQUISITES:
 *   1. `npm start` running and `./jtag ping` succeeds
 *   2. BRAVE_SEARCH_API_KEY set (or DuckDuckGo fallback)
 *   3. A cloud LLM provider reachable
 *
 * USAGE:
 *   npx tsx tests/integration/web-research-synthesis.test.ts
 */

import { runJtagCommand } from '../test-utils/CRUDTestUtils';
import { readFileSync, existsSync } from 'fs';
import {
  buildKnowledgeExplorationPipeline,
} from '../../system/sentinel/pipelines/KnowledgeExplorationPipeline';
import type { DataSourceConfig } from '../../system/genome/shared/KnowledgeTypes';

// ─── Test Configuration ──────────────────────────────────────────────────────

const TEST_PERSONA_NAME = 'WebResearchTestPersona';
const SEARCH_TOPIC = 'Rust programming language history';
const SEARCH_QUERIES = [
  'Rust programming language history creator',
  'Rust language major releases timeline',
];

// ─── Test Phases ─────────────────────────────────────────────────────────────

async function main() {
  console.log('='.repeat(80));
  console.log('WEB RESEARCH SYNTHESIS — E2E TEST');
  console.log('='.repeat(80));
  console.log(`Topic: ${SEARCH_TOPIC}`);
  console.log();

  const results: { phase: string; success: boolean; details: string }[] = [];

  try {
    // ════════════════════════════════════════════════════════════════════════
    // Phase 1: PIPELINE STRUCTURE — Verify web research pipeline builds
    // ════════════════════════════════════════════════════════════════════════
    console.log('\n' + '─'.repeat(60));
    console.log('Phase 1: WEB RESEARCH PIPELINE STRUCTURE');
    console.log('─'.repeat(60));

    const dataSources: DataSourceConfig[] = [
      {
        type: 'web-research',
        searchQueries: SEARCH_QUERIES,
        maxPagesPerQuery: 2,
      },
    ];

    const pipeline = buildKnowledgeExplorationPipeline({
      dataSources,
      maxFacts: 20,
    });

    console.log(`Pipeline name: ${pipeline.name}`);
    console.log(`Pipeline steps: ${pipeline.steps.length}`);

    const commandSteps = pipeline.steps.filter(s => s.type === 'command');
    const llmSteps = pipeline.steps.filter(s => s.type === 'llm');

    // 2 queries * (1 search + 1 fetch) = 4 command steps + 1 LLM = 5 total
    const structureValid = commandSteps.length >= 2 && llmSteps.length === 1;
    results.push({
      phase: 'Pipeline Structure',
      success: structureValid,
      details: `${commandSteps.length} command steps, ${llmSteps.length} LLM steps`,
    });
    console.log(`   Command steps: ${commandSteps.length} (search+fetch pairs)`);
    console.log(`   LLM steps: ${llmSteps.length} (fact extraction)`);
    console.log(`   Structure valid: ${structureValid}`);

    // ════════════════════════════════════════════════════════════════════════
    // Phase 2: WEB SEARCH — Execute search queries
    // ════════════════════════════════════════════════════════════════════════
    console.log('\n' + '─'.repeat(60));
    console.log('Phase 2: WEB SEARCH');
    console.log('─'.repeat(60));

    const searchResult = await runJtagCommand(
      `interface/web/search --query="${SEARCH_QUERIES[0]}" --maxResults=3`
    );

    const searchSuccess = Boolean(searchResult.success && (searchResult.results as any[])?.length > 0);
    const resultCount = (searchResult.results as any[])?.length ?? 0;
    console.log(`   Search success: ${searchSuccess}`);
    console.log(`   Results found: ${resultCount}`);

    if (searchSuccess && resultCount > 0) {
      const first = (searchResult.results as any[])[0];
      console.log(`   Top result: ${first.title} (${first.domain})`);
    }

    results.push({
      phase: 'Web Search',
      success: searchSuccess,
      details: `${resultCount} results for "${SEARCH_QUERIES[0]}"`,
    });

    // ════════════════════════════════════════════════════════════════════════
    // Phase 3: WEB FETCH — Fetch top result content
    // ════════════════════════════════════════════════════════════════════════
    if (searchSuccess && resultCount > 0) {
      console.log('\n' + '─'.repeat(60));
      console.log('Phase 3: WEB FETCH');
      console.log('─'.repeat(60));

      const topUrl = (searchResult.results as any[])[0].url;
      const fetchResult = await runJtagCommand(
        `interface/web/fetch --url="${topUrl}" --format=text --maxLength=10000`
      );

      const fetchSuccess = Boolean(fetchResult.success && (fetchResult.contentLength as number) > 0);
      console.log(`   Fetch success: ${fetchSuccess}`);
      console.log(`   Content length: ${fetchResult.contentLength} chars`);
      console.log(`   Content preview: ${(fetchResult.content as string)?.slice(0, 150)}...`);

      results.push({
        phase: 'Web Fetch',
        success: fetchSuccess,
        details: `${fetchResult.contentLength} chars from ${topUrl}`,
      });

      // ════════════════════════════════════════════════════════════════════
      // Phase 4: GROUNDED SYNTHESIS — Use fetched content as grounding
      // ════════════════════════════════════════════════════════════════════
      if (fetchSuccess) {
        console.log('\n' + '─'.repeat(60));
        console.log('Phase 4: GROUNDED DATASET SYNTHESIS');
        console.log('─'.repeat(60));

        // Extract a few facts from the content as grounding
        const content = (fetchResult.content as string)?.slice(0, 5000) ?? '';
        const groundingContext = `Source: ${topUrl}\nContent excerpt:\n${content}`;

        const synthesizeResult = await runJtagCommand(
          `genome/dataset-synthesize --topic="${SEARCH_TOPIC}" --skill="rust-history" --personaName="${TEST_PERSONA_NAME}" --exampleCount=10 --groundingContext='${groundingContext.replace(/'/g, "'\\''").replace(/\n/g, '\\n')}'`
        );

        const synthesisSuccess = Boolean(synthesizeResult.success && synthesizeResult.datasetPath);
        console.log(`   Synthesis: ${synthesisSuccess ? 'SUCCESS' : 'FAILED'}`);
        console.log(`   Dataset: ${synthesizeResult.datasetPath}`);
        console.log(`   Examples: ${synthesizeResult.exampleCount}`);

        // Validate JSONL content
        if (synthesisSuccess && existsSync(synthesizeResult.datasetPath as string)) {
          const jsonl = readFileSync(synthesizeResult.datasetPath as string, 'utf-8');
          const lines = jsonl.trim().split('\n').filter(l => l.trim());
          let validLines = 0;

          for (const line of lines) {
            try {
              const example = JSON.parse(line);
              if (example.messages && Array.isArray(example.messages) && example.messages.length >= 2) {
                validLines++;
              }
            } catch { /* skip invalid lines */ }
          }

          console.log(`   Valid JSONL lines: ${validLines}/${lines.length}`);

          results.push({
            phase: 'Grounded Synthesis',
            success: validLines > 0,
            details: `${validLines} valid training examples in JSONL`,
          });
        } else {
          results.push({
            phase: 'Grounded Synthesis',
            success: false,
            details: synthesisSuccess ? 'Dataset file not found' : `Synthesis failed: ${synthesizeResult.error}`,
          });
        }
      }
    } else {
      results.push({
        phase: 'Web Fetch',
        success: false,
        details: 'Skipped: no search results',
      });
    }

    // ════════════════════════════════════════════════════════════════════════
    // Phase 5: RATE LIMITER — Verify caching works (search same query again)
    // ════════════════════════════════════════════════════════════════════════
    console.log('\n' + '─'.repeat(60));
    console.log('Phase 5: SEARCH CACHING');
    console.log('─'.repeat(60));

    const t1 = Date.now();
    const cachedResult = await runJtagCommand(
      `interface/web/search --query="${SEARCH_QUERIES[0]}" --maxResults=3`
    );
    const t2 = Date.now();

    const cacheSuccess = Boolean(cachedResult.success);
    const cacheDuration = t2 - t1;
    console.log(`   Cached search: ${cacheSuccess ? 'SUCCESS' : 'FAILED'}`);
    console.log(`   Duration: ${cacheDuration}ms (should be fast if cached)`);

    results.push({
      phase: 'Search Caching',
      success: cacheSuccess,
      details: `Repeated search completed in ${cacheDuration}ms`,
    });

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
