/**
 * Base Test Helper for Remote Fine-Tuning APIs
 *
 * Shared logic for testing OpenAI, DeepSeek, Fireworks, Together APIs.
 * Implements the universal 4-step pattern: UPLOAD → CREATE → POLL → SAVE
 *
 * Usage:
 *   Extend this class and implement provider-specific methods.
 *   See test-openai.ts for example.
 *
 * Philosophy: Maximum code reuse across provider tests
 */

import fs from 'fs';
import path from 'path';

// ============================================================================
// Types
// ============================================================================

export interface TestConfig {
  apiKey: string;
  apiBase: string;
  baseModel: string;
  epochs: number;
  trainingFile: string;
  providerId: string;
}

export interface UploadResult {
  fileId?: string;
  trainingData?: any[];
  uploadType: 'file-upload' | 'inline-data';
}

export interface JobStatus {
  state: string;
  modelId: string | null;
  error?: string;
}

export interface TestResult {
  success: boolean;
  modelId?: string;
  metadataPath?: string;
  trainingTime?: number;
  error?: string;
}

// ============================================================================
// Base Test Class
// ============================================================================

export abstract class BaseRemoteAPITest {
  protected config: TestConfig;

  constructor(config: TestConfig) {
    this.config = config;
  }

  // ==========================================================================
  // Abstract Methods (Provider-Specific)
  // ==========================================================================

  /**
   * Step 1: Upload training data to provider
   */
  protected abstract uploadTrainingData(
    jsonlPath: string
  ): Promise<UploadResult>;

  /**
   * Step 2: Create fine-tuning job
   */
  protected abstract createFineTuningJob(
    uploadResult: UploadResult
  ): Promise<string>;

  /**
   * Step 3: Check job status
   */
  protected abstract checkJobStatus(jobId: string): Promise<JobStatus>;

  /**
   * Determine if job is complete
   */
  protected abstract isComplete(status: JobStatus): boolean;

  /**
   * Determine if job failed
   */
  protected abstract isFailed(status: JobStatus): boolean;

  // ==========================================================================
  // Shared Logic (All Providers)
  // ==========================================================================

  /**
   * Main test orchestration (universal 4-step pattern)
   */
  async runTest(): Promise<TestResult> {
    console.log(`🚀 ${this.config.providerId} Fine-Tuning Test (API)`);
    console.log('='.repeat(50));
    console.log('');

    const startTime = Date.now();

    try {
      // Validate API key
      this.validateApiKey();
      console.log(`✅ API key found: ${this.config.apiKey.substring(0, 10)}...`);
      console.log('');

      // Step 1: Upload training data
      const uploadResult = await this.uploadTrainingData(
        this.config.trainingFile
      );

      // Step 2: Create fine-tuning job
      const jobId = await this.createFineTuningJob(uploadResult);

      // Step 3: Poll until complete
      const modelId = await this.waitForCompletion(jobId);

      // Step 4: Save metadata
      const metadataPath = await this.saveAdapterMetadata(
        modelId,
        uploadResult,
        jobId
      );

      const trainingTime = Date.now() - startTime;

      // Success summary
      console.log('\n🎉 SUCCESS! Fine-tuning completed');
      console.log('='.repeat(50));
      console.log(`Model ID: ${modelId}`);
      console.log(`Metadata: ${metadataPath}`);
      console.log(`Training Time: ${Math.floor(trainingTime / 1000)}s`);
      console.log('');
      console.log('Next steps:');
      console.log(`1. Test the model with ${this.config.providerId} API`);
      console.log(`2. Integrate this code into ${this.config.providerId}LoRAAdapter.ts`);
      console.log('3. Test with ./jtag genome/train');

      return {
        success: true,
        modelId,
        metadataPath,
        trainingTime,
      };
    } catch (error) {
      const errorMessage =
        error instanceof Error ? error.message : String(error);
      console.error('\n❌ ERROR:', errorMessage);
      if (error instanceof Error && error.stack) {
        console.error('\nStack trace:');
        console.error(error.stack);
      }

      return {
        success: false,
        error: errorMessage,
        trainingTime: Date.now() - startTime,
      };
    }
  }

  /**
   * Step 3: Poll job status until complete (shared implementation)
   */
  protected async waitForCompletion(
    jobId: string,
    maxWaitMs: number = 1800000 // 30 minutes
  ): Promise<string> {
    console.log('\n⏳ Step 3: Waiting for job completion...');
    console.log(`   Job ID: ${jobId}`);
    console.log(`   Max wait: ${maxWaitMs / 1000 / 60} minutes`);
    console.log('');

    const startTime = Date.now();
    let lastStatus: string | null = null;

    while (Date.now() - startTime < maxWaitMs) {
      const status = await this.checkJobStatus(jobId);

      // Log status changes
      if (status.state !== lastStatus) {
        const elapsed = Math.floor((Date.now() - startTime) / 1000);
        console.log(`   [${elapsed}s] Status: ${status.state}`);
        lastStatus = status.state;
      }

      // Check for completion
      if (this.isComplete(status)) {
        const elapsed = Math.floor((Date.now() - startTime) / 1000);
        if (!status.modelId) {
          throw new Error('Job completed but no model ID returned');
        }
        console.log(`\n✅ Job completed successfully!`);
        console.log(`   Fine-tuned Model: ${status.modelId}`);
        console.log(`   Training Time: ${elapsed}s`);
        return status.modelId;
      }

      // Check for failure
      if (this.isFailed(status)) {
        const errorMsg = status.error || 'Unknown error';
        throw new Error(`Fine-tuning failed: ${errorMsg}`);
      }

      // Poll every 5 seconds
      await this.sleep(5000);
    }

    throw new Error(`Job timeout after ${maxWaitMs}ms`);
  }

  /**
   * Step 4: Save adapter metadata (shared implementation)
   */
  protected async saveAdapterMetadata(
    modelId: string,
    uploadResult: UploadResult,
    jobId: string
  ): Promise<string> {
    console.log('\n💾 Step 4: Saving adapter metadata...');

    const metadata = {
      provider: this.config.providerId,
      modelId,
      baseModel: this.config.baseModel,
      trainingFileId: uploadResult.fileId,
      jobId,
      trainingDate: new Date().toISOString(),
      uploadType: uploadResult.uploadType,
    };

    const metadataPath = `/tmp/${this.config.providerId}-adapter-${Date.now()}.json`;
    fs.writeFileSync(metadataPath, JSON.stringify(metadata, null, 2));

    console.log(`✅ Metadata saved`);
    console.log(`   Path: ${metadataPath}`);

    return metadataPath;
  }

  // ==========================================================================
  // Utility Methods
  // ==========================================================================

  /**
   * Validate API key exists
   */
  protected validateApiKey(): void {
    if (!this.config.apiKey) {
      throw new Error(
        `${this.config.providerId.toUpperCase()}_API_KEY environment variable not set`
      );
    }
  }

  /**
   * Read training file and validate
   */
  protected readTrainingFile(jsonlPath: string): {
    content: string;
    lines: string[];
  } {
    if (!fs.existsSync(jsonlPath)) {
      throw new Error(`Training file not found: ${jsonlPath}`);
    }

    const content = fs.readFileSync(jsonlPath, 'utf-8');
    const lines = content.trim().split('\n');

    return { content, lines };
  }

  /**
   * Sleep utility
   */
  protected sleep(ms: number): Promise<void> {
    return new Promise((resolve) => setTimeout(resolve, ms));
  }

  /**
   * Make authenticated fetch request
   */
  protected async fetch(
    endpoint: string,
    options: RequestInit = {}
  ): Promise<Response> {
    const url = `${this.config.apiBase}${endpoint}`;

    const headers = {
      Authorization: `Bearer ${this.config.apiKey}`,
      ...options.headers,
    };

    return fetch(url, {
      ...options,
      headers,
    });
  }

  /**
   * Handle API response
   */
  protected async handleResponse<T>(response: Response): Promise<T> {
    if (!response.ok) {
      const error = await response.text();
      throw new Error(
        `API request failed: ${response.status} ${response.statusText}\n${error}`
      );
    }

    return response.json() as Promise<T>;
  }
}
