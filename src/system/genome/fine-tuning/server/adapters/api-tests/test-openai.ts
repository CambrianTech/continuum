#!/usr/bin/env node
/**
 * OpenAI Fine-Tuning API Test
 *
 * Tests the OpenAI fine-tuning API workflow using the shared base class.
 * Once this works, code can be integrated into OpenAILoRAAdapter.ts
 *
 * Usage:
 *   npx tsx system/genome/fine-tuning/server/adapters/api-tests/test-openai.ts
 *
 * Requirements:
 *   - OPENAI_API_KEY in environment
 *   - /tmp/test-training-minimal.jsonl (training data)
 *
 * Workflow:
 *   1. Upload JSONL via FormData to /files
 *   2. Create fine-tuning job with file ID
 *   3. Poll job status until 'succeeded'
 *   4. Save adapter metadata
 */

import {
  BaseRemoteAPITest,
  TestConfig,
  UploadResult,
  JobStatus,
} from './BaseRemoteAPITest';

// ============================================================================
// OpenAI API Test
// ============================================================================

class OpenAIFineTuningTest extends BaseRemoteAPITest {
  /**
   * Step 1: Upload training file to OpenAI
   * Uses FormData with 'purpose=fine-tune'
   */
  protected async uploadTrainingData(
    jsonlPath: string
  ): Promise<UploadResult> {
    console.log('📤 Step 1: Uploading training file to OpenAI...');
    console.log(`   File: ${jsonlPath}`);

    const { content, lines } = this.readTrainingFile(jsonlPath);
    console.log(`   Examples: ${lines.length}`);

    // Create FormData with file
    const formData = new FormData();
    const blob = new Blob([content], { type: 'application/json' });
    formData.append('file', blob, 'training.jsonl');
    formData.append('purpose', 'fine-tune');

    const response = await this.fetch('/files', {
      method: 'POST',
      body: formData,
    });

    const data = await this.handleResponse<{
      id: string;
      bytes: number;
      filename: string;
    }>(response);

    console.log(`✅ File uploaded successfully`);
    console.log(`   File ID: ${data.id}`);
    console.log(`   Bytes: ${data.bytes}`);

    return {
      fileId: data.id,
      uploadType: 'file-upload',
    };
  }

  /**
   * Step 2: Create fine-tuning job
   * POST to /fine_tuning/jobs with file ID
   */
  protected async createFineTuningJob(
    uploadResult: UploadResult
  ): Promise<string> {
    console.log('\n🔧 Step 2: Creating fine-tuning job...');
    console.log(`   File ID: ${uploadResult.fileId}`);
    console.log(`   Base Model: ${this.config.baseModel}`);
    console.log(`   Epochs: ${this.config.epochs}`);

    const response = await this.fetch('/fine_tuning/jobs', {
      method: 'POST',
      headers: {
        'Content-Type': 'application/json',
      },
      body: JSON.stringify({
        training_file: uploadResult.fileId,
        model: this.config.baseModel,
        hyperparameters: {
          n_epochs: this.config.epochs,
        },
      }),
    });

    const data = await this.handleResponse<{
      id: string;
      status: string;
      created_at: number;
    }>(response);

    console.log(`✅ Job created successfully`);
    console.log(`   Job ID: ${data.id}`);
    console.log(`   Status: ${data.status}`);
    console.log(`   Created: ${new Date(data.created_at * 1000).toISOString()}`);

    return data.id;
  }

  /**
   * Step 3: Check job status
   * GET from /fine_tuning/jobs/:id
   */
  protected async checkJobStatus(jobId: string): Promise<JobStatus> {
    const response = await this.fetch(`/fine_tuning/jobs/${jobId}`);

    const job = await this.handleResponse<{
      id: string;
      status: string;
      fine_tuned_model: string | null;
      error?: { message: string };
    }>(response);

    return {
      state: job.status,
      modelId: job.fine_tuned_model,
      error: job.error?.message,
    };
  }

  /**
   * Determine if job completed successfully
   */
  protected isComplete(status: JobStatus): boolean {
    return status.state === 'succeeded';
  }

  /**
   * Determine if job failed
   */
  protected isFailed(status: JobStatus): boolean {
    return status.state === 'failed' || status.state === 'cancelled';
  }
}

// ============================================================================
// Main Execution
// ============================================================================

async function main(): Promise<void> {
  // Configuration
  const config: TestConfig = {
    apiKey: process.env.OPENAI_API_KEY || '',
    apiBase: 'https://api.openai.com/v1',
    baseModel: 'gpt-4o-mini-2024-07-18',
    epochs: 1,
    trainingFile: '/tmp/test-training-minimal.jsonl',
    providerId: 'openai',
  };

  // Run test
  const test = new OpenAIFineTuningTest(config);
  const result = await test.runTest();

  // Exit with appropriate code
  process.exit(result.success ? 0 : 1);
}

// Run if executed directly
if (require.main === module) {
  main().catch((error) => {
    console.error('Unhandled error:', error);
    process.exit(1);
  });
}

// Export for reuse
export { OpenAIFineTuningTest };
