/**
 * RemoteAPICore - Shared logic for remote fine-tuning APIs
 *
 * Universal 4-step pattern: UPLOAD → CREATE → POLL → SAVE
 *
 * Used by:
 * 1. JTAG adapters (OpenAILoRAAdapter, DeepSeekLoRAAdapter, etc.)
 * 2. Isolated test scripts (test-openai.ts, test-deepseek.ts, etc.)
 *
 * Philosophy: Write once, test isolated, integrate everywhere
 *
 * Benefits:
 * - Code reuse between tests and production
 * - Fast isolated testing (no JTAG overhead)
 * - Consistent behavior across modes
 * - Easy debugging (test fails → adapter will fail)
 */

import fs from 'fs';
import path from 'path';
import type { RemoteAPIConfig, UploadResult, JobStatus } from './RemoteAPITypes';

// Node.js 18+ has built-in fetch, but type it explicitly
declare const fetch: typeof globalThis.fetch;

/**
 * Base class for all remote fine-tuning API implementations
 *
 * Subclasses must implement provider-specific methods:
 * - uploadTrainingData() - How to upload JSONL to provider
 * - createFineTuningJob() - How to start training job
 * - checkJobStatus() - How to poll job status
 * - isComplete() - What status means "done"
 * - isFailed() - What status means "failed"
 *
 * Inherited shared methods:
 * - waitForCompletion() - Universal polling logic
 * - saveAdapterMetadata() - Save adapter JSON
 * - readTrainingFile() - Read and validate JSONL
 * - fetch() - Authenticated HTTP requests
 * - handleResponse() - Error handling
 */
export abstract class RemoteAPICore {
  protected config: RemoteAPIConfig;

  constructor(config: RemoteAPIConfig) {
    this.config = config;
  }

  // ==========================================================================
  // Abstract Methods (Provider-Specific Implementation Required)
  // ==========================================================================

  /**
   * Step 1: Upload training data to provider
   *
   * Each provider has different upload strategy:
   * - OpenAI/DeepSeek/Together: FormData file upload to /files
   * - Fireworks: Inline data in job request (no separate upload)
   * - AWS Bedrock: S3 upload, return S3 URI
   *
   * @param jsonlPath Path to training data JSONL file
   * @returns Upload result with file ID or inline data
   */
  protected abstract uploadTrainingData(
    jsonlPath: string
  ): Promise<UploadResult>;

  /**
   * Step 2: Create fine-tuning job
   *
   * Each provider has different job creation endpoint:
   * - OpenAI: POST /fine_tuning/jobs
   * - DeepSeek: POST /fine_tuning/jobs (OpenAI-compatible)
   * - Fireworks: POST /fine-tuning/jobs
   * - Together: POST /fine-tunes
   * - AWS Bedrock: Different SDK entirely
   *
   * @param uploadResult Result from uploadTrainingData()
   * @returns Job ID for polling
   */
  protected abstract createFineTuningJob(
    uploadResult: UploadResult
  ): Promise<string>;

  /**
   * Step 3: Check job status
   *
   * Each provider returns different status format:
   * - OpenAI: {status: 'succeeded', fine_tuned_model: 'ft:...'}
   * - DeepSeek: Same as OpenAI
   * - Fireworks: {state: 'COMPLETED', ...}
   * - Together: Different structure
   *
   * @param jobId Job ID from createFineTuningJob()
   * @returns Normalized job status
   */
  protected abstract checkJobStatus(jobId: string): Promise<JobStatus>;

  /**
   * Determine if job completed successfully
   *
   * Each provider has different success status:
   * - OpenAI/DeepSeek: status === 'succeeded'
   * - Fireworks: state === 'COMPLETED'
   * - Together: status === 'completed'
   *
   * @param status Status from checkJobStatus()
   * @returns true if job completed successfully
   */
  protected abstract isComplete(status: JobStatus): boolean;

  /**
   * Determine if job failed
   *
   * Each provider has different failure status:
   * - OpenAI/DeepSeek: status === 'failed' || status === 'cancelled'
   * - Fireworks: state === 'FAILED'
   * - Together: status === 'error'
   *
   * @param status Status from checkJobStatus()
   * @returns true if job failed
   */
  protected abstract isFailed(status: JobStatus): boolean;

  // ==========================================================================
  // Shared Implementation (Universal Logic)
  // ==========================================================================

  /**
   * Step 3 (continued): Poll job status until complete
   *
   * Universal polling pattern (same for all providers):
   * 1. Check job status
   * 2. If complete → return model ID
   * 3. If failed → throw error
   * 4. If running → wait 5s and repeat
   * 5. If timeout → throw error
   *
   * @param jobId Job ID to poll
   * @param maxWaitMs Maximum wait time (default: 30 minutes)
   * @param pollIntervalMs Poll interval (default: 5 seconds)
   * @returns Model ID when complete
   * @throws Error if job fails or times out
   */
  protected async waitForCompletion(
    jobId: string,
    maxWaitMs: number = 1800000, // 30 minutes
    pollIntervalMs: number = 5000 // 5 seconds
  ): Promise<string> {
    console.log('\n⏳ Step 3: Waiting for job completion...');
    console.log(`   Job ID: ${jobId}`);
    console.log(`   Max wait: ${maxWaitMs / 1000 / 60} minutes`);
    console.log(`   Poll interval: ${pollIntervalMs / 1000}s`);
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
        const errorMsg = status.error ?? 'Unknown error';
        throw new Error(`Fine-tuning failed: ${errorMsg}`);
      }

      // Poll interval
      await this.sleep(pollIntervalMs);
    }

    throw new Error(`Job timeout after ${maxWaitMs}ms`);
  }

  /**
   * Step 4: Save adapter metadata
   *
   * Universal metadata format (same for all providers):
   * - provider: Provider ID (openai, deepseek, etc.)
   * - modelId: Fine-tuned model ID
   * - baseModel: Base model used
   * - trainingFileId: File ID or S3 URI
   * - jobId: Training job ID
   * - trainingDate: ISO timestamp
   * - uploadType: 'file-upload' | 'inline-data' | 's3-upload'
   *
   * @param modelId Fine-tuned model ID
   * @param uploadResult Upload result from step 1
   * @param jobId Job ID from step 2
   * @param outputPath Optional custom output path
   * @returns Path to metadata file
   */
  protected async saveAdapterMetadata(
    modelId: string,
    uploadResult: UploadResult,
    jobId: string,
    outputPath?: string
  ): Promise<string> {
    console.log('\n💾 Step 4: Saving adapter metadata...');

    const metadata = {
      provider: this.config.providerId,
      modelId,
      baseModel: this.config.baseModel,
      trainingFileId: uploadResult.fileId ?? uploadResult.s3Uri,
      jobId,
      trainingDate: new Date().toISOString(),
      uploadType: uploadResult.uploadType,
      hyperparameters: {
        epochs: this.config.epochs,
      },
    };

    const metadataPath =
      outputPath ??
      path.join(
        '/tmp',
        `${this.config.providerId}-adapter-${Date.now()}.json`
      );

    // Ensure directory exists
    const dir = path.dirname(metadataPath);
    if (!fs.existsSync(dir)) {
      fs.mkdirSync(dir, { recursive: true });
    }

    fs.writeFileSync(metadataPath, JSON.stringify(metadata, null, 2));

    console.log(`✅ Metadata saved`);
    console.log(`   Path: ${metadataPath}`);

    return metadataPath;
  }

  // ==========================================================================
  // Utility Methods (Shared Helpers)
  // ==========================================================================

  /**
   * Validate API key exists
   *
   * @throws Error if API key not configured
   */
  protected validateApiKey(): void {
    if (!this.config.apiKey) {
      throw new Error(
        `${this.config.providerId.toUpperCase()}_API_KEY environment variable not set`
      );
    }
  }

  /**
   * Get API key (for subclasses)
   */
  protected getApiKey(): string {
    return this.config.apiKey;
  }

  /**
   * Read training file and validate format
   *
   * @param jsonlPath Path to JSONL file
   * @returns File content and parsed lines
   * @throws Error if file not found or invalid
   */
  protected readTrainingFile(jsonlPath: string): {
    content: string;
    lines: string[];
  } {
    if (!fs.existsSync(jsonlPath)) {
      throw new Error(`Training file not found: ${jsonlPath}`);
    }

    const content = fs.readFileSync(jsonlPath, 'utf-8');
    const lines = content
      .trim()
      .split('\n')
      .filter((line) => line.trim().length > 0);

    if (lines.length === 0) {
      throw new Error(`Training file is empty: ${jsonlPath}`);
    }

    // Validate JSONL format
    try {
      lines.forEach((line) => {
        JSON.parse(line); // Will throw if invalid
      });
    } catch (error) {
      throw new Error(
        `Invalid JSONL format at line ${lines.length}: ${error instanceof Error ? error.message : String(error)}`
      );
    }

    return { content, lines };
  }

  /**
   * Sleep utility
   *
   * @param ms Milliseconds to sleep
   */
  protected sleep(ms: number): Promise<void> {
    return new Promise((resolve) => setTimeout(resolve, ms));
  }

  /**
   * Make authenticated fetch request
   *
   * Automatically adds Authorization header with API key.
   * Handles both Bearer token and custom auth schemes.
   *
   * @param endpoint API endpoint (relative to apiBase)
   * @param options Fetch options
   * @returns Response object
   */
  protected async fetch(
    endpoint: string,
    options: RequestInit = {}
  ): Promise<Response> {
    const url = `${this.config.apiBase}${endpoint}`;

    const headers: Record<string, string> = {
      Authorization: `Bearer ${this.config.apiKey}`,
      ...(options.headers as Record<string, string>),
    };

    return fetch(url, {
      ...options,
      headers,
    });
  }

  /**
   * Handle API response and parse JSON
   *
   * Universal error handling:
   * - Check response.ok
   * - Parse error body
   * - Throw with detailed message
   *
   * @param response Fetch response
   * @returns Parsed JSON
   * @throws Error if response not ok
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

  /**
   * Get configuration (useful for debugging)
   */
  protected getConfig(): RemoteAPIConfig {
    return this.config;
  }
}
