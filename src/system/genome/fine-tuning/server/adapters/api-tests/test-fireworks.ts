#!/usr/bin/env node
/**
 * Fireworks AI Fine-Tuning API Test
 *
 * Fireworks has a DIFFERENT upload strategy - embeds data inline in job request.
 * No separate file upload step needed.
 *
 * Usage:
 *   npx tsx system/genome/fine-tuning/server/adapters/api-tests/test-fireworks.ts
 *
 * Requirements:
 *   - FIREWORKS_API_KEY in environment
 *   - /tmp/test-training-minimal.jsonl (training data)
 *
 * Key Differences from OpenAI:
 *   - NO file upload - data embedded directly in job request
 *   - Different API base: https://api.fireworks.ai/v1
 *   - Different status values: 'completed' instead of 'succeeded'
 *   - Model ID in 'output_model' or 'model_id' field
 */

import {
  BaseRemoteAPITest,
  TestConfig,
  UploadResult,
  JobStatus,
} from './BaseRemoteAPITest';

// ============================================================================
// Fireworks API Test
// ============================================================================

class FireworksFineTuningTest extends BaseRemoteAPITest {
  /**
   * Step 1: "Upload" training data (Fireworks-specific)
   * Fireworks doesn't require separate upload - just parse JSONL for inline use
   */
  protected async uploadTrainingData(
    jsonlPath: string
  ): Promise<UploadResult> {
    console.log('📤 Step 1: Preparing training data for Fireworks...');
    console.log(`   File: ${jsonlPath}`);
    console.log('   (Fireworks embeds data inline - no upload needed)');

    const { content, lines } = this.readTrainingFile(jsonlPath);
    console.log(`   Examples: ${lines.length}`);

    // Parse JSONL into array of training examples
    const trainingData = lines.map((line) => {
      const example = JSON.parse(line);
      return {
        messages: example.messages,
      };
    });

    console.log(`✅ Data prepared for inline submission`);
    console.log(`   Training examples: ${trainingData.length}`);

    return {
      trainingData,
      uploadType: 'inline-data',
    };
  }

  /**
   * Step 2: Create fine-tuning job with inline data
   */
  protected async createFineTuningJob(
    uploadResult: UploadResult
  ): Promise<string> {
    console.log('\n🔧 Step 2: Creating fine-tuning job...');
    console.log(`   Base Model: ${this.config.baseModel}`);
    console.log(`   Epochs: ${this.config.epochs}`);
    console.log(`   Training data: ${uploadResult.trainingData?.length} examples (inline)`);

    const response = await this.fetch('/fine-tuning/jobs', {
      method: 'POST',
      headers: {
        'Content-Type': 'application/json',
      },
      body: JSON.stringify({
        model: this.config.baseModel,
        training_data: uploadResult.trainingData, // ← Inline data
        hyperparameters: {
          n_epochs: this.config.epochs,
        },
      }),
    });

    const data = await this.handleResponse<{
      id: string;
      status: string;
      created_at?: string;
    }>(response);

    console.log(`✅ Job created successfully`);
    console.log(`   Job ID: ${data.id}`);
    console.log(`   Status: ${data.status}`);
    if (data.created_at) {
      console.log(`   Created: ${data.created_at}`);
    }

    return data.id;
  }

  /**
   * Step 3: Check job status
   */
  protected async checkJobStatus(jobId: string): Promise<JobStatus> {
    const response = await this.fetch(`/fine-tuning/jobs/${jobId}`);

    const job = await this.handleResponse<{
      id: string;
      status: string;
      output_model?: string;
      model_id?: string;
      error?: { message: string };
    }>(response);

    // Fireworks may return model ID in 'output_model' or 'model_id'
    const modelId = job.output_model || job.model_id || null;

    return {
      state: job.status,
      modelId,
      error: job.error?.message,
    };
  }

  /**
   * Determine if job completed successfully
   * Fireworks uses 'completed' OR 'succeeded'
   */
  protected isComplete(status: JobStatus): boolean {
    return (
      status.state === 'completed' ||
      status.state === 'succeeded' ||
      status.state === 'COMPLETED'
    );
  }

  /**
   * Determine if job failed
   */
  protected isFailed(status: JobStatus): boolean {
    return (
      status.state === 'failed' ||
      status.state === 'FAILED' ||
      status.state === 'ERROR'
    );
  }
}

// ============================================================================
// Main Execution
// ============================================================================

async function main(): Promise<void> {
  // Configuration
  const config: TestConfig = {
    apiKey: process.env.FIREWORKS_API_KEY || '',
    apiBase: 'https://api.fireworks.ai/v1',
    baseModel: 'accounts/fireworks/models/llama-v3p1-8b-instruct', // Example model
    epochs: 1,
    trainingFile: '/tmp/test-training-minimal.jsonl',
    providerId: 'fireworks',
  };

  console.log('🔥 Fireworks uses INLINE data submission');
  console.log('   No separate file upload needed!');
  console.log('');

  // Run test
  const test = new FireworksFineTuningTest(config);
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
export { FireworksFineTuningTest };
