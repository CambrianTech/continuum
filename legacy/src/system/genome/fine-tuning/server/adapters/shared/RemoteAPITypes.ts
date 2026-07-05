/**
 * Shared types for remote fine-tuning APIs
 *
 * Used by both:
 * 1. RemoteAPICore (shared logic)
 * 2. Test infrastructure (isolated tests)
 * 3. JTAG adapters (production code)
 */

/**
 * Configuration for remote API
 */
export interface RemoteAPIConfig {
  /** API key for authentication */
  apiKey: string;

  /** API base URL (e.g., 'https://api.openai.com/v1') */
  apiBase: string;

  /** Base model to fine-tune (e.g., 'gpt-4o-mini', 'deepseek-chat') */
  baseModel: string;

  /** Number of training epochs */
  epochs: number;

  /** Provider ID (openai, deepseek, fireworks, together, aws-bedrock) */
  providerId: string;
}

/**
 * Result of uploading training data
 *
 * Different providers use different upload strategies:
 * - OpenAI/DeepSeek/Together: File upload, returns fileId
 * - Fireworks: Inline data, no separate upload
 * - AWS Bedrock: S3 upload, returns S3 URI
 */
export interface UploadResult {
  /** File ID (OpenAI, DeepSeek, Together) */
  fileId?: string;

  /** S3 URI (AWS Bedrock) */
  s3Uri?: string;

  /** Inline training data (Fireworks) */
  trainingData?: Record<string, unknown>[];

  /** Upload strategy used */
  uploadType: 'file-upload' | 'inline-data' | 's3-upload';
}

/**
 * Normalized job status
 *
 * Different providers return different status formats.
 * This interface normalizes them for universal polling logic.
 */
export interface JobStatus {
  /** Job state (provider-specific: 'succeeded', 'COMPLETED', etc.) */
  state: string;

  /** Fine-tuned model ID (null if not complete) */
  modelId: string | null;

  /** Error message (if failed) */
  error?: string;
}

/**
 * Test result (for isolated test mode)
 */
export interface TestResult {
  /** Whether test succeeded */
  success: boolean;

  /** Fine-tuned model ID (if successful) */
  modelId?: string;

  /** Path to adapter metadata file */
  metadataPath?: string;

  /** Training time in milliseconds */
  trainingTime?: number;

  /** Error message (if failed) */
  error?: string;
}
