#!/usr/bin/env tsx
/**
 * GENOME FINE-TUNING END-TO-END TEST
 * ===================================
 *
 * Tests ALL fine-tuning adapters using JTAG COMMANDS ONLY.
 * This is the foundation for building genomic layers - every adapter must work.
 *
 * GOAL: Train genomic layers for PersonaUsers by proving the infrastructure works.
 *
 * PREREQUISITES:
 * 1. Run `npm start` and wait for server to be ready
 * 2. Ensure API keys are configured in ~/.continuum/config.env
 * 3. Dataset must exist (set FINE_TUNING_DATASET_PATH env var or use default)
 *
 * TEST FLOW:
 * 1. Submit training jobs for ALL providers using genome/job-create command
 * 2. Save job handles to JSON
 * 3. Poll job status using genome/job-status command
 * 4. Verify all jobs complete successfully
 *
 * PROVIDERS TO TEST:
 * - OpenAI (gpt-4o-mini)
 * - Fireworks (llama-v3p1-8b-instruct)
 * - DeepSeek (deepseek-chat)
 * - Mistral (mistral-small)
 * - Together (Meta-Llama-3.1-8B-Instruct-Turbo)
 *
 * CRITICAL: This uses REAL API calls with REAL money. Test dataset must be minimal.
 */

import { runJtagCommand } from '../test-utils/CRUDTestUtils';
import { writeFileSync, readFileSync, existsSync, mkdirSync } from 'fs';
import { join, dirname } from 'path';
import { TrainingMethod } from '../../daemons/data-daemon/shared/entities/FineTuningTypes';
import type { JobConfiguration } from '../../daemons/data-daemon/shared/entities/FineTuningTypes';
import type { GenomeJobCreateResult } from '../../commands/genome/job-create/shared/GenomeJobCreateTypes';
import type { GenomeJobStatusResult } from '../../commands/genome/job-status/shared/GenomeJobStatusTypes';

// Test configuration - use env var or default to relative path
const DATASET_PATH = process.env.FINE_TUNING_DATASET_PATH ||
  join(__dirname, '../../.continuum/datasets/fine-tuning-test.jsonl');
const JOBS_DIR = join(__dirname, '../../.continuum');
const JOBS_FILE = join(JOBS_DIR, 'test-jobs.json');
const TEST_PERSONA_ID = '00000000-0000-0000-0000-000000000001'; // Use a real persona from seeded data

interface JobRecord {
  provider: string;
  jobId: string;
  providerJobId: string;
  baseModel: string;
  submittedAt: number;
  status?: string;
  completedAt?: number;
  fineTunedModel?: string;
  error?: string;
}

// Provider configurations
const PROVIDERS = [
  {
    name: 'openai',
    baseModel: 'gpt-4o-mini-2024-07-18',
    method: TrainingMethod.LORA,
  },
  {
    name: 'fireworks',
    baseModel: 'accounts/fireworks/models/llama-v3p1-8b-instruct',
    method: TrainingMethod.LORA,
  },
  {
    name: 'deepseek',
    baseModel: 'deepseek-chat',
    method: TrainingMethod.LORA,
  },
  {
    name: 'mistral',
    baseModel: 'open-mistral-7b',
    method: TrainingMethod.LORA,
  },
  {
    name: 'together',
    baseModel: 'meta-llama/Meta-Llama-3.1-8B-Instruct-Reference',
    method: TrainingMethod.LORA,
  },
] as const;

/**
 * Phase 1: Submit training jobs for all providers
 */
async function phase1_submitJobs(): Promise<JobRecord[]> {
  console.log('\n📤 PHASE 1: Submitting Training Jobs');
  console.log('=====================================\n');

  // Create dataset if it doesn't exist (10 minimal chat-formatted training examples)
  if (!existsSync(DATASET_PATH)) {
    const dir = dirname(DATASET_PATH);
    if (!existsSync(dir)) mkdirSync(dir, { recursive: true });
    const examples = Array.from({ length: 10 }, (_, i) => JSON.stringify({
      messages: [
        { role: 'system', content: 'You are a helpful assistant.' },
        { role: 'user', content: `What is ${i + 1} + ${i + 1}?` },
        { role: 'assistant', content: `${(i + 1) * 2}` },
      ],
    }));
    writeFileSync(DATASET_PATH, examples.join('\n') + '\n');
    console.log(`📝 Created test dataset: ${DATASET_PATH} (${examples.length} examples)`);
  }
  console.log(`✅ Dataset verified: ${DATASET_PATH}\n`);

  // Ensure jobs directory exists
  if (!existsSync(JOBS_DIR)) {
    mkdirSync(JOBS_DIR, { recursive: true });
  }

  const jobRecords: JobRecord[] = [];

  for (const provider of PROVIDERS) {
    console.log(`\n🔄 Testing ${provider.name}...`);
    console.log(`   Base Model: ${provider.baseModel}`);
    console.log(`   Method: ${provider.method}`);

    try {
      // Build job configuration using Phase 1 schema
      const configuration: JobConfiguration = {
        model: {
          baseModel: provider.baseModel,
        },
        datasets: {
          trainingFileId: DATASET_PATH,
        },
        method: {
          type: provider.method,
        },
        schedule: {
          epochs: 1,
          batchSize: 4, // Most providers require >= 2, using 4 as safe default
          sequenceLength: 512,
        },
        optimizer: {
          learningRate: 0.0001,
        },
      };

      // Submit training job using JTAG command
      console.log(`   Submitting job via JTAG command...`);
      const result = await runJtagCommand(
        `genome/job-create --personaId="${TEST_PERSONA_ID}" --provider="${provider.name}" --configuration='${JSON.stringify(configuration)}' --skipValidation=false`
      );

      if (!result.success || !result.job) {
        console.error(`   ❌ FAILED: ${result.error || 'Unknown error'}`);
        jobRecords.push({
          provider: provider.name,
          jobId: '',
          providerJobId: '',
          baseModel: provider.baseModel,
          submittedAt: Date.now(),
          status: 'failed',
          error: result.error as string,
        });
        continue;
      }

      // Extract job details with proper typing
      const createResult = result as GenomeJobCreateResult;
      if (!createResult.job) {
        throw new Error('Job details missing from successful result');
      }

      const record: JobRecord = {
        provider: provider.name,
        jobId: createResult.job.jobId,
        providerJobId: createResult.job.providerJobId,
        baseModel: provider.baseModel,
        submittedAt: Date.now(),
        status: createResult.job.status,
      };

      console.log(`   ✅ SUCCESS`);
      console.log(`   Job ID: ${createResult.job.jobId}`);
      console.log(`   Provider Job ID: ${createResult.job.providerJobId}`);
      console.log(`   Status: ${createResult.job.status}`);

      jobRecords.push(record);

    } catch (error) {
      console.error(`   ❌ ERROR: ${error instanceof Error ? error.message : 'Unknown error'}`);
      jobRecords.push({
        provider: provider.name,
        jobId: '',
        providerJobId: '',
        baseModel: provider.baseModel,
        submittedAt: Date.now(),
        status: 'error',
        error: error instanceof Error ? error.message : 'Unknown error',
      });
    }
  }

  // Save job records
  writeFileSync(JOBS_FILE, JSON.stringify(jobRecords, null, 2));

  console.log(`\n📊 PHASE 1 SUMMARY:`);
  console.log(`   Total providers: ${PROVIDERS.length}`);
  console.log(`   Jobs submitted: ${jobRecords.filter(r => r.jobId).length}`);
  console.log(`   Jobs failed: ${jobRecords.filter(r => !r.jobId).length}`);
  console.log(`   Jobs saved to: ${JOBS_FILE}\n`);

  return jobRecords;
}

/**
 * Phase 2: Check status of all submitted jobs using genome/job-status
 */
async function phase2_checkStatus(jobRecords: JobRecord[]): Promise<JobRecord[]> {
  console.log('\n📊 PHASE 2: Checking Job Status');
  console.log('================================\n');

  const validJobs = jobRecords.filter(r => r.jobId);
  if (validJobs.length === 0) {
    console.log('⚠️  No valid jobs to monitor (Phase 1 may have failed)\n');
    return jobRecords;
  }

  console.log(`Found ${validJobs.length} jobs to check\n`);

  for (const record of validJobs) {
    console.log(`\n🔍 Checking ${record.provider}...`);
    console.log(`   Job ID: ${record.jobId}`);

    try {
      // Use genome/job-status command (wraps data/read with proper typing)
      const result = await runJtagCommand(
        `genome/job-status --jobId="${record.jobId}" --refresh=false`
      );

      if (!result.success || !result.job) {
        console.error(`   ❌ FAILED: ${result.error || 'Job not found'}`);
        record.error = result.error as string || 'Job not found';
        continue;
      }

      const statusResult = result as GenomeJobStatusResult;
      if (!statusResult.job) {
        throw new Error('Job details missing from successful status result');
      }

      record.status = statusResult.job.status;
      if (statusResult.job.completedAt) record.completedAt = statusResult.job.completedAt;
      if (statusResult.job.fineTunedModel) record.fineTunedModel = statusResult.job.fineTunedModel;
      if (statusResult.job.errorMessage) record.error = statusResult.job.errorMessage;

      console.log(`   Status: ${statusResult.job.status}`);
      if (statusResult.job.progress !== undefined) {
        console.log(`   Progress: ${statusResult.job.progress}%`);
      }
      if (statusResult.job.fineTunedModel) {
        console.log(`   ✅ Fine-tuned model: ${statusResult.job.fineTunedModel}`);
      }
      if (statusResult.job.errorMessage) {
        console.error(`   ⚠️  Error: ${statusResult.job.errorMessage}`);
      }

    } catch (error) {
      console.error(`   ❌ ERROR: ${error instanceof Error ? error.message : 'Unknown error'}`);
      record.error = error instanceof Error ? error.message : 'Unknown error';
    }
  }

  // Save updated records
  writeFileSync(JOBS_FILE, JSON.stringify(jobRecords, null, 2));

  console.log(`\n📊 PHASE 2 SUMMARY:`);
  const succeeded = jobRecords.filter(r => r.status === 'succeeded').length;
  const running = jobRecords.filter(r => r.status === 'running' || r.status === 'queued' || r.status === 'validating_files').length;
  const failed = jobRecords.filter(r => r.status === 'failed').length;

  console.log(`   Succeeded: ${succeeded}`);
  console.log(`   Running: ${running}`);
  console.log(`   Failed: ${failed}`);
  console.log(`\n   Results saved to: ${JOBS_FILE}\n`);

  return jobRecords;
}

/**
 * Phase 3: Poll until all jobs complete or timeout
 */
async function phase3_waitForCompletion(jobRecords: JobRecord[]): Promise<JobRecord[]> {
  console.log('\n⏰ PHASE 3: Waiting for Completion');
  console.log('===================================\n');

  const validJobs = jobRecords.filter(r => r.jobId);
  if (validJobs.length === 0) {
    console.log('⚠️  No jobs to monitor\n');
    return jobRecords;
  }

  const MAX_POLLS = 60; // 10 minutes max
  const POLL_INTERVAL = 10000; // 10 seconds

  for (let poll = 0; poll < MAX_POLLS; poll++) {
    console.log(`\n🔄 Poll ${poll + 1}/${MAX_POLLS}...`);

    let allComplete = true;

    for (const record of validJobs) {
      // Skip already completed/failed jobs
      if (record.status === 'succeeded' || record.status === 'failed' || record.status === 'cancelled') {
        continue;
      }

      allComplete = false;

      try {
        // Use genome/job-status command
        const result = await runJtagCommand(
          `genome/job-status --jobId="${record.jobId}" --refresh=false`
        );

        if (result.success && result.job) {
          const statusResult = result as GenomeJobStatusResult;
          if (statusResult.job) {
            record.status = statusResult.job.status;
            if (statusResult.job.completedAt) record.completedAt = statusResult.job.completedAt;
            if (statusResult.job.fineTunedModel) record.fineTunedModel = statusResult.job.fineTunedModel;
            if (statusResult.job.errorMessage) record.error = statusResult.job.errorMessage;

            const progress = statusResult.job.progress !== undefined ? ` (${statusResult.job.progress}%)` : '';
            console.log(`   ${record.provider}: ${statusResult.job.status}${progress}`);
          }
        }
      } catch (error) {
        console.error(`   ${record.provider}: ERROR - ${error instanceof Error ? error.message : 'Unknown'}`);
      }
    }

    // Save progress
    writeFileSync(JOBS_FILE, JSON.stringify(jobRecords, null, 2));

    if (allComplete) {
      console.log('\n✅ All jobs completed!\n');
      break;
    }

    if (poll < MAX_POLLS - 1) {
      console.log(`\n⏳ Waiting ${POLL_INTERVAL / 1000}s before next poll...\n`);
      await new Promise(resolve => setTimeout(resolve, POLL_INTERVAL));
    }
  }

  return jobRecords;
}

/**
 * Print final summary
 */
function printFinalSummary(jobRecords: JobRecord[]): void {
  console.log('\n📊 FINAL RESULTS');
  console.log('=================\n');

  const succeeded = jobRecords.filter(r => r.status === 'succeeded');
  const failed = jobRecords.filter(r => r.status === 'failed' || r.status === 'error');
  const running = jobRecords.filter(r => r.status === 'running' || r.status === 'queued' || r.status === 'validating_files');

  console.log(`✅ Succeeded: ${succeeded.length}`);
  succeeded.forEach(r => {
    console.log(`   - ${r.provider}: ${r.fineTunedModel || 'Model ID unknown'}`);
  });

  console.log(`\n❌ Failed: ${failed.length}`);
  failed.forEach(r => {
    console.log(`   - ${r.provider}${r.error ? `: ${r.error}` : ''}`);
  });

  console.log(`\n⏳ Still Running: ${running.length}`);
  running.forEach(r => {
    console.log(`   - ${r.provider}: ${r.status}`);
  });

  console.log(`\n📄 Full results: ${JOBS_FILE}\n`);

  // Test passes if at least one job succeeded
  if (succeeded.length === 0) {
    console.error('❌ TEST FAILED: No jobs succeeded\n');
    process.exit(1);
  } else {
    console.log(`✅ TEST PASSED: ${succeeded.length} job(s) completed successfully\n`);
  }
}

/**
 * Main test execution
 */
async function main() {
  console.log('\n🧬 GENOME FINE-TUNING END-TO-END TEST');
  console.log('=====================================\n');

  try {
    // Phase 1: Submit jobs
    let jobRecords = await phase1_submitJobs();

    // Phase 2: Initial status check (wait 10s for jobs to initialize)
    console.log('\n⏳ Waiting 10 seconds for jobs to initialize...\n');
    await new Promise(resolve => setTimeout(resolve, 10000));
    jobRecords = await phase2_checkStatus(jobRecords);

    // Phase 3: Poll until completion
    jobRecords = await phase3_waitForCompletion(jobRecords);

    // Print final summary
    printFinalSummary(jobRecords);

  } catch (error) {
    console.error('\n❌ TEST EXECUTION FAILED:', error);
    process.exit(1);
  }
}

// Run if executed directly
if (require.main === module) {
  main();
}

/**
 * USAGE:
 *
 * 1. Start the JTAG server:
 *    npm start
 *    # Wait 90+ seconds for server to be ready
 *
 * 2. Run this test:
 *    npx tsx tests/integration/genome-fine-tuning-e2e.test.ts
 *
 * EXPECTED OUTPUT:
 * - Phase 1: All jobs submitted (or most, some providers may fail if not configured)
 * - Phase 2: Status check shows jobs are queued/running
 * - Phase 3: Jobs complete successfully, fine-tuned models returned
 *
 * SUCCESS CRITERIA:
 * - At least 1 provider successfully completes training
 * - Job handles are saved and can be queried
 * - Fine-tuned model IDs are returned
 *
 * This proves the genomic layer infrastructure works end-to-end.
 */
