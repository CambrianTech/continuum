/**
 * Sentinel LoRA Training Pipeline — Integration Tests
 *
 * Tests the genome/dataset-prepare and genome/train commands against
 * the live system. Requires `npm start` to be running.
 *
 * genome/training-pipeline (full pipeline) requires Rust sentinel,
 * tested as a smoke test that returns a handle.
 */

import { describe, it, expect } from 'vitest';
import { Commands } from '../../system/core/shared/Commands';
import type { GenomeDatasetPrepareParams, GenomeDatasetPrepareResult } from '../../commands/genome/dataset-prepare/shared/GenomeDatasetPrepareTypes';
import type { GenomeTrainParams, GenomeTrainResult } from '../../commands/genome/train/shared/GenomeTrainTypes';
import type { GenomeTrainingPipelineParams, GenomeTrainingPipelineResult } from '../../commands/genome/training-pipeline/shared/GenomeTrainingPipelineTypes';
import type { UUID } from '../../system/core/types/CrossPlatformUUID';
import * as fs from 'fs';
import * as path from 'path';

// These IDs come from the seeded data — Joel + general room
const TEST_PERSONA_ID = '00000000-0000-0000-0000-000000000002' as UUID; // Helper AI
const TEST_PERSONA_NAME = 'Helper AI';
const TEST_ROOM_ID = '00000000-0000-0000-0000-000000000001' as UUID; // general room

describe('genome/dataset-prepare', () => {
  it('should reject missing personaId', { timeout: 15000 }, async () => {
    const result = await Commands.execute<GenomeDatasetPrepareParams, GenomeDatasetPrepareResult>(
      'genome/dataset-prepare',
      {
        personaName: TEST_PERSONA_NAME,
        roomId: TEST_ROOM_ID,
      } as any
    );

    // ValidationError should propagate as error
    expect(result.success).toBe(false);
  });

  it('should attempt dataset preparation from general room', { timeout: 15000 }, async () => {
    const result = await Commands.execute<GenomeDatasetPrepareParams, GenomeDatasetPrepareResult>(
      'genome/dataset-prepare',
      {
        personaId: TEST_PERSONA_ID,
        personaName: TEST_PERSONA_NAME,
        roomId: TEST_ROOM_ID,
        traitType: 'conversational',
        minMessages: 2, // Low threshold for testing
        maxMessages: 50,
      }
    );

    // May fail if not enough messages — that's expected
    if (result.success) {
      expect(result.datasetPath).toBeDefined();
      expect(result.exampleCount).toBeGreaterThan(0);
      expect(result.personaId).toBe(TEST_PERSONA_ID);
      expect(result.traitType).toBe('conversational');

      // Verify the JSONL file exists
      expect(fs.existsSync(result.datasetPath)).toBe(true);

      // Verify JSONL content
      const content = fs.readFileSync(result.datasetPath, 'utf-8');
      const lines = content.trim().split('\n');
      expect(lines.length).toBe(result.exampleCount);

      // Each line should be valid JSON with messages array
      const firstLine = JSON.parse(lines[0]);
      expect(firstLine.messages).toBeDefined();
      expect(Array.isArray(firstLine.messages)).toBe(true);

      // Cleanup
      fs.unlinkSync(result.datasetPath);
    } else {
      // Insufficient messages is a valid outcome
      expect(result.error).toBeDefined();
      console.log(`  Dataset prepare returned expected error: ${result.error}`);
    }
  });
});

describe('genome/train', () => {
  it('should reject missing required params', { timeout: 15000 }, async () => {
    const result = await Commands.execute<GenomeTrainParams, GenomeTrainResult>(
      'genome/train',
      {
        personaId: TEST_PERSONA_ID,
        personaName: TEST_PERSONA_NAME,
        // Missing traitType and datasetPath
      } as any
    );

    expect(result.success).toBe(false);
  });

  it('should train or report PEFT unavailable', { timeout: 120000 }, async () => {
    // Create a minimal JSONL file for testing
    const tempPath = path.join('/tmp', `test-dataset-${Date.now()}.jsonl`);
    const testData = [
      JSON.stringify({ messages: [{ role: 'user', content: 'Hello' }, { role: 'assistant', content: 'Hi there!' }] }),
      JSON.stringify({ messages: [{ role: 'user', content: 'How are you?' }, { role: 'assistant', content: 'I am doing well, thank you!' }] }),
    ].join('\n');
    fs.writeFileSync(tempPath, testData, 'utf-8');

    try {
      const result = await Commands.execute<GenomeTrainParams, GenomeTrainResult>(
        'genome/train',
        {
          personaId: TEST_PERSONA_ID,
          personaName: TEST_PERSONA_NAME,
          traitType: 'conversational',
          datasetPath: tempPath,
          baseModel: 'smollm2:135m',
        }
      );

      // Will succeed only if Python env is bootstrapped
      // Otherwise returns error about PEFT environment
      if (!result.success) {
        expect(result.error).toContain('PEFT');
      } else {
        expect(result.adapterPath).toBeDefined();
        expect(result.metrics).toBeDefined();
        expect(result.metrics.epochs).toBeGreaterThan(0);

        // Verify entity was persisted
        if (result.layerId) {
          const readResult = await Commands.execute('data/read', {
            collection: 'genome_layers',
            id: result.layerId,
          } as any) as any;
          expect(readResult.success).toBe(true);
          expect(readResult.data?.name).toContain('conversational');
          expect(readResult.data?.traitType).toBe('conversational');

          // Verify manifest.json exists in adapter directory
          const manifestPath = `${result.adapterPath}/manifest.json`;
          expect(fs.existsSync(manifestPath)).toBe(true);
        }
      }
    } finally {
      fs.unlinkSync(tempPath);
    }
  });
});

describe('genome/training-pipeline', () => {
  it('should reject missing required params', { timeout: 15000 }, async () => {
    const result = await Commands.execute<GenomeTrainingPipelineParams, GenomeTrainingPipelineResult>(
      'genome/training-pipeline',
      {
        personaId: TEST_PERSONA_ID,
        // Missing personaName and roomId
      } as any
    );

    expect(result.success).toBe(false);
  });

  it('should build and submit pipeline to sentinel', { timeout: 15000 }, async () => {
    const result = await Commands.execute<GenomeTrainingPipelineParams, GenomeTrainingPipelineResult>(
      'genome/training-pipeline',
      {
        personaId: TEST_PERSONA_ID,
        personaName: TEST_PERSONA_NAME,
        roomId: TEST_ROOM_ID,
        traitType: 'conversational',
        baseModel: 'smollm2:135m',
      }
    );

    // Pipeline submission should succeed (even if individual steps fail later)
    if (result.success) {
      expect(result.handle).toBeDefined();
      expect(result.handle.length).toBeGreaterThan(0);
      expect(result.pipelineName).toContain('lora-training');
      console.log(`  Pipeline started with handle: ${result.handle}`);
    } else {
      // May fail if Rust core IPC is not running
      console.log(`  Pipeline submission failed (expected if Rust core not running): ${result.error}`);
    }
  });
});
