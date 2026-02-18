/**
 * Unit tests for FineTuningJobEntity with new JobConfiguration schema
 */

import { describe, it, expect, beforeEach } from 'vitest';
import { FineTuningJobEntity } from '../../daemons/data-daemon/shared/entities/FineTuningJobEntity';
import { FineTuningDatasetEntity } from '../../daemons/data-daemon/shared/entities/FineTuningDatasetEntity';
import { FineTunedModelEntity } from '../../daemons/data-daemon/shared/entities/FineTunedModelEntity';
import {
  TrainingMethod,
  LRSchedulerType,
  ModelPrecision,
  OptimizationFeature,
  TrainOnInputs,
  PARAMETER_RANGES,
  type JobConfiguration
} from '../../daemons/data-daemon/shared/entities/FineTuningTypes';

describe('FineTuningJobEntity', () => {
  let validConfig: JobConfiguration;

  beforeEach(() => {
    // Create a valid config with sensible defaults
    validConfig = {
      model: {
        baseModel: 'gpt-4o-mini-2024-07-18',
        precision: ModelPrecision.FP16
      },
      datasets: {
        trainingFileId: 'dataset-uuid-001',
        validationFileId: null
      },
      method: {
        type: TrainingMethod.LORA,
        loraConfig: {
          rank: 8,
          alpha: 16,
          dropout: 0,
          trainableModules: 'all-linear'
        }
      },
      schedule: {
        epochs: 3,
        batchSize: 4,
        sequenceLength: 2048,
        gradientAccumulation: 1,
        checkpoints: 1,
        evaluations: 1,
        trainOnInputs: TrainOnInputs.AUTO
      },
      optimizer: {
        learningRate: 0.00001,
        scheduler: {
          type: LRSchedulerType.COSINE,
          minLRRatio: 0,
          warmupRatio: 0.03
        },
        weightDecay: 0,
        maxGradientNorm: 1
      },
      optimizations: {
        enabled: [OptimizationFeature.FLASH_ATTENTION]
      },
      output: {
        suffix: 'v1'
      },
      metadata: {}
    };
  });

  describe('Entity Creation & Validation', () => {
    it('should create a valid job entity with full configuration', () => {
      const job = new FineTuningJobEntity();
      job.id = 'job-001';
      job.personaId = 'persona-001';
      job.provider = 'openai';
      job.providerJobId = 'ftjob-abc123';
      job.baseModel = 'gpt-4o-mini-2024-07-18';
      job.trainingFileId = 'dataset-001';
      job.validationFileId = null;
      job.configuration = validConfig;
      job.hyperparameters = null;
      job.status = 'queued';
      job.providerCreatedAt = Date.now();
      job.startedAt = null;
      job.finishedAt = null;
      job.fineTunedModel = null;
      job.trainedTokens = null;
      job.error = null;
      job.metrics = {};
      job.events = [];
      job.metadata = {};
      job.createdAt = Date.now();
      job.updatedAt = Date.now();

      const validation = job.validate();
      expect(validation.success).toBe(true);
      expect(validation.error).toBeUndefined();
    });

    it('should reject job without personaId', () => {
      const job = new FineTuningJobEntity();
      job.id = 'job-001';
      job.provider = 'openai';
      job.providerJobId = 'ftjob-abc123';
      job.baseModel = 'gpt-4o-mini-2024-07-18';
      job.trainingFileId = 'dataset-001';
      job.configuration = validConfig;
      job.status = 'queued';
      job.providerCreatedAt = Date.now();

      const validation = job.validate();
      expect(validation.success).toBe(false);
      expect(validation.error).toBe('personaId is required');
    });

    it('should reject job without configuration or hyperparameters', () => {
      const job = new FineTuningJobEntity();
      job.id = 'job-001';
      job.personaId = 'persona-001';
      job.provider = 'openai';
      job.providerJobId = 'ftjob-abc123';
      job.baseModel = 'gpt-4o-mini-2024-07-18';
      job.trainingFileId = 'dataset-001';
      job.configuration = null;
      job.hyperparameters = null;
      job.status = 'queued';
      job.providerCreatedAt = Date.now();

      const validation = job.validate();
      expect(validation.success).toBe(false);
      expect(validation.error).toContain('Either hyperparameters (legacy) or configuration is required');
    });

    it('should accept legacy hyperparameters for backward compatibility', () => {
      const job = new FineTuningJobEntity();
      job.id = 'job-001';
      job.personaId = 'persona-001';
      job.provider = 'openai';
      job.providerJobId = 'ftjob-abc123';
      job.baseModel = 'gpt-4o-mini-2024-07-18';
      job.trainingFileId = 'dataset-001';
      job.configuration = null;
      job.hyperparameters = {
        n_epochs: 3,
        batch_size: 4,
        learning_rate_multiplier: 1.0
      };
      job.status = 'queued';
      job.providerCreatedAt = Date.now();
      job.metrics = {};
      job.events = [];
      job.metadata = {};
      job.createdAt = Date.now();
      job.updatedAt = Date.now();

      const validation = job.validate();
      expect(validation.success).toBe(true);
    });
  });

  describe('Status Management', () => {
    let job: FineTuningJobEntity;

    beforeEach(() => {
      job = new FineTuningJobEntity();
      job.id = 'job-001';
      job.personaId = 'persona-001';
      job.provider = 'openai';
      job.providerJobId = 'ftjob-abc123';
      job.baseModel = 'gpt-4o-mini-2024-07-18';
      job.trainingFileId = 'dataset-001';
      job.configuration = validConfig;
      job.status = 'queued';
      job.providerCreatedAt = Date.now();
      job.metrics = {};
      job.events = [];
      job.metadata = {};
      job.createdAt = Date.now();
      job.updatedAt = Date.now();
    });

    it('should correctly identify active jobs', () => {
      expect(job.isActive()).toBe(true);

      job.status = 'running';
      expect(job.isActive()).toBe(true);

      job.status = 'validating_files';
      expect(job.isActive()).toBe(true);
    });

    it('should correctly identify terminal jobs', () => {
      job.status = 'succeeded';
      expect(job.isTerminal()).toBe(true);
      expect(job.isActive()).toBe(false);

      job.status = 'failed';
      expect(job.isTerminal()).toBe(true);

      job.status = 'cancelled';
      expect(job.isTerminal()).toBe(true);
    });

    it('should mark job as started and add event', () => {
      job.markStarted();

      expect(job.status).toBe('running');
      expect(job.startedAt).toBeDefined();
      expect(job.events.length).toBe(1);
      expect(job.events[0].message).toBe('Training started');
    });

    it('should mark job as succeeded with model info', () => {
      const fineTunedModel = 'ft:gpt-4o-mini:org:v1:abc123';
      const trainedTokens = 50000;

      job.markSucceeded(fineTunedModel, trainedTokens);

      expect(job.status).toBe('succeeded');
      expect(job.finishedAt).toBeDefined();
      expect(job.fineTunedModel).toBe(fineTunedModel);
      expect(job.trainedTokens).toBe(trainedTokens);
      expect(job.error).toBeNull();
      expect(job.events.length).toBe(1);
      expect(job.events[0].level).toBe('info');
    });

    it('should mark job as failed with error info', () => {
      const error = {
        message: 'Training failed due to invalid data',
        code: 'invalid_training_data',
        timestamp: Date.now()
      };

      job.markFailed(error);

      expect(job.status).toBe('failed');
      expect(job.finishedAt).toBeDefined();
      expect(job.error).toEqual(error);
      expect(job.events.length).toBe(1);
      expect(job.events[0].level).toBe('error');
    });
  });

  describe('Metrics Management', () => {
    let job: FineTuningJobEntity;

    beforeEach(() => {
      job = new FineTuningJobEntity();
      job.id = 'job-001';
      job.personaId = 'persona-001';
      job.provider = 'openai';
      job.providerJobId = 'ftjob-abc123';
      job.baseModel = 'gpt-4o-mini-2024-07-18';
      job.trainingFileId = 'dataset-001';
      job.configuration = validConfig;
      job.status = 'running';
      job.providerCreatedAt = Date.now();
      job.metrics = {};
      job.events = [];
      job.metadata = {};
      job.createdAt = Date.now();
      job.updatedAt = Date.now();
    });

    it('should add loss metrics correctly', () => {
      job.addMetric('loss', { step: 100, value: 0.45, timestamp: Date.now() });
      job.addMetric('loss', { step: 200, value: 0.35, timestamp: Date.now() });
      job.addMetric('loss', { step: 300, value: 0.25, timestamp: Date.now() });

      expect(job.metrics.loss).toBeDefined();
      expect(job.metrics.loss?.length).toBe(3);
      expect(job.getLatestLoss()).toBe(0.25);
    });

    it('should add accuracy metrics correctly', () => {
      job.addMetric('accuracy', { step: 100, value: 0.75, timestamp: Date.now() });
      job.addMetric('accuracy', { step: 200, value: 0.85, timestamp: Date.now() });

      expect(job.metrics.accuracy).toBeDefined();
      expect(job.metrics.accuracy?.length).toBe(2);
      expect(job.getLatestAccuracy()).toBe(0.85);
    });

    it('should handle validation metrics', () => {
      job.addMetric('valLoss', { step: 100, value: 0.50, timestamp: Date.now() });
      job.addMetric('valAccuracy', { step: 100, value: 0.70, timestamp: Date.now() });

      expect(job.metrics.valLoss).toBeDefined();
      expect(job.metrics.valAccuracy).toBeDefined();
      expect(job.metrics.valLoss?.length).toBe(1);
      expect(job.metrics.valAccuracy?.length).toBe(1);
    });

    it('should return null for latest metrics when none exist', () => {
      expect(job.getLatestLoss()).toBeNull();
      expect(job.getLatestAccuracy()).toBeNull();
    });
  });

  describe('PARAMETER_RANGES Validation', () => {
    it('should have sensible default values', () => {
      expect(PARAMETER_RANGES.schedule.epochs.default).toBe(3);
      expect(PARAMETER_RANGES.schedule.batchSize.default).toBe(4);
      expect(PARAMETER_RANGES.schedule.sequenceLength.default).toBe(2048);
      expect(PARAMETER_RANGES.optimizer.learningRate.default).toBe(0.00001);
      expect(PARAMETER_RANGES.lora.rank.default).toBe(8);
      expect(PARAMETER_RANGES.lora.alpha.default).toBe(16);
    });

    it('should have proper range constraints', () => {
      expect(PARAMETER_RANGES.schedule.epochs.min).toBe(1);
      expect(PARAMETER_RANGES.schedule.epochs.max).toBe(20);
      expect(PARAMETER_RANGES.schedule.batchSize.min).toBe(1);
      expect(PARAMETER_RANGES.schedule.batchSize.max).toBe(64);
      expect(PARAMETER_RANGES.lora.rank.min).toBe(1);
      expect(PARAMETER_RANGES.lora.rank.max).toBe(256);
    });
  });

  describe('Training Method Configuration', () => {
    it('should support FULL fine-tuning method', () => {
      const config: JobConfiguration = {
        ...validConfig,
        method: {
          type: TrainingMethod.FULL
          // No loraConfig needed for FULL
        }
      };

      const job = new FineTuningJobEntity();
      job.id = 'job-001';
      job.personaId = 'persona-001';
      job.provider = 'openai';
      job.providerJobId = 'ftjob-abc123';
      job.baseModel = 'gpt-4o-mini-2024-07-18';
      job.trainingFileId = 'dataset-001';
      job.configuration = config;
      job.status = 'queued';
      job.providerCreatedAt = Date.now();
      job.metrics = {};
      job.events = [];
      job.metadata = {};
      job.createdAt = Date.now();
      job.updatedAt = Date.now();

      const validation = job.validate();
      expect(validation.success).toBe(true);
    });

    it('should support LoRA with custom configuration', () => {
      const config: JobConfiguration = {
        ...validConfig,
        method: {
          type: TrainingMethod.LORA,
          loraConfig: {
            rank: 16,
            alpha: 32,
            dropout: 0.05,
            trainableModules: 'q_proj,v_proj,k_proj,o_proj'
          }
        }
      };

      const job = new FineTuningJobEntity();
      job.id = 'job-001';
      job.personaId = 'persona-001';
      job.provider = 'openai';
      job.providerJobId = 'ftjob-abc123';
      job.baseModel = 'llama-3.1-8b';
      job.trainingFileId = 'dataset-001';
      job.configuration = config;
      job.status = 'queued';
      job.providerCreatedAt = Date.now();
      job.metrics = {};
      job.events = [];
      job.metadata = {};
      job.createdAt = Date.now();
      job.updatedAt = Date.now();

      const validation = job.validate();
      expect(validation.success).toBe(true);
      expect(job.configuration?.method.loraConfig?.rank).toBe(16);
    });
  });

  describe('Duration Calculation', () => {
    it('should calculate duration correctly', () => {
      const job = new FineTuningJobEntity();
      job.startedAt = Date.now();
      job.finishedAt = job.startedAt + 3600000; // 1 hour later

      const duration = job.getDuration();
      expect(duration).toBe(3600000);
    });

    it('should return null when not finished', () => {
      const job = new FineTuningJobEntity();
      job.startedAt = Date.now();
      job.finishedAt = null;

      expect(job.getDuration()).toBeNull();
    });
  });
});

describe('FineTuningDatasetEntity', () => {
  it('should validate a complete dataset entity', () => {
    const dataset = new FineTuningDatasetEntity();
    dataset.id = 'dataset-001';
    dataset.personaId = 'persona-001';
    dataset.provider = 'openai';
    dataset.providerFileId = 'file-abc123';
    dataset.name = 'Test Dataset';
    dataset.filename = 'training.jsonl';
    dataset.purpose = 'fine-tune';
    dataset.bytes = 1024;
    dataset.status = 'processed';
    dataset.statusDetails = {
      exampleCount: 100,
      tokenCount: 50000
    };
    dataset.metadata = {};
    dataset.createdAt = Date.now();
    dataset.updatedAt = Date.now();

    const validation = dataset.validate();
    expect(validation.success).toBe(true);
    expect(dataset.isReady()).toBe(true);
  });

  it('should not be ready when status is not processed', () => {
    const dataset = new FineTuningDatasetEntity();
    dataset.id = 'dataset-001';
    dataset.personaId = 'persona-001';
    dataset.provider = 'openai';
    dataset.providerFileId = 'file-abc123';
    dataset.name = 'Test Dataset';
    dataset.filename = 'training.jsonl';
    dataset.purpose = 'fine-tune';
    dataset.bytes = 1024;
    dataset.status = 'uploading';
    dataset.statusDetails = {};
    dataset.metadata = {};
    dataset.createdAt = Date.now();
    dataset.updatedAt = Date.now();

    expect(dataset.isReady()).toBe(false);
  });
});

describe('FineTunedModelEntity', () => {
  it('should validate a complete model entity', () => {
    const model = new FineTunedModelEntity();
    model.id = 'model-001';
    model.personaId = 'persona-001';
    model.provider = 'openai';
    model.providerModelId = 'ft:gpt-4o-mini:org:v1:abc123';
    model.name = 'Test Model v1';
    model.baseModel = 'gpt-4o-mini-2024-07-18';
    model.jobId = 'job-001';
    model.object = 'fine_tuned_model';
    model.providerCreatedAt = Date.now();
    model.ownedBy = 'org-abc123';
    model.status = 'active';
    model.validationMetrics = {
      loss: 0.25,
      accuracy: 0.95
    };
    model.metadata = {};
    model.createdAt = Date.now();
    model.updatedAt = Date.now();

    const validation = model.validate();
    expect(validation.success).toBe(true);
    expect(model.isActive()).toBe(true);
    expect(model.getValidationLoss()).toBe(0.25);
    expect(model.getValidationAccuracy()).toBe(0.95);
  });
});
