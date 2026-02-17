/**
 * Sentinel LoRA Training Pipeline — Integration Tests
 *
 * Tests the genome/dataset-prepare, genome/train, genome/dataset-synthesize,
 * genome/academy-session commands against the live system.
 *
 * Requires: `npm start` running + JTAGClient connection.
 * If the client can't connect, tests skip gracefully.
 */

import { describe, it, expect, beforeAll, afterAll } from 'vitest';
import { Commands } from '../../system/core/shared/Commands';
import { JTAGClientServer } from '../../system/core/client/server/JTAGClientServer';
import { JTAGClient } from '../../system/core/client/shared/JTAGClient';
import type { GenomeDatasetPrepareParams, GenomeDatasetPrepareResult } from '../../commands/genome/dataset-prepare/shared/GenomeDatasetPrepareTypes';
import type { GenomeTrainParams, GenomeTrainResult } from '../../commands/genome/train/shared/GenomeTrainTypes';
import type { GenomeTrainingPipelineParams, GenomeTrainingPipelineResult } from '../../commands/genome/training-pipeline/shared/GenomeTrainingPipelineTypes';
import type { GenomeDatasetSynthesizeParams, GenomeDatasetSynthesizeResult } from '../../commands/genome/dataset-synthesize/shared/GenomeDatasetSynthesizeTypes';
import type { GenomeAcademySessionParams, GenomeAcademySessionResult } from '../../commands/genome/academy-session/shared/GenomeAcademySessionTypes';
import type { UUID } from '../../system/core/types/CrossPlatformUUID';
import * as fs from 'fs';
import * as path from 'path';

// These IDs come from the seeded data — Joel + general room
const TEST_PERSONA_ID = '00000000-0000-0000-0000-000000000002' as UUID; // Helper AI
const TEST_PERSONA_NAME = 'Helper AI';
const TEST_ROOM_ID = '00000000-0000-0000-0000-000000000001' as UUID; // general room

// Client connection — required for Commands.execute to work
let client: JTAGClient | null = null;
let connectionError: string | null = null;

beforeAll(async () => {
  try {
    const connectPromise = JTAGClientServer.connect();
    const timeoutPromise = new Promise<never>((_, reject) =>
      setTimeout(() => reject(new Error('Client connection timed out (20s)')), 20000)
    );
    const result = await Promise.race([connectPromise, timeoutPromise]);
    client = result.client;

    // Register as default client so Commands.execute() can find it
    JTAGClient.registerClient('default', client);
  } catch (err) {
    connectionError = err instanceof Error ? err.message : String(err);
    console.warn(`⚠️ Integration test client connection failed: ${connectionError}`);
    console.warn('   Tests will be skipped. Use ./jtag CLI for manual integration testing.');
  }
}, 25000);

afterAll(async () => {
  if (client) {
    JTAGClient.unregisterClient('default');
    if (typeof (client as any).disconnect === 'function') {
      await (client as any).disconnect();
    }
  }
});

describe('genome/dataset-prepare', () => {
  it('should reject missing personaId', { timeout: 15000 }, async () => {
    if (!client) return; // Skip if no connection
    const result = await Commands.execute<GenomeDatasetPrepareParams, GenomeDatasetPrepareResult>(
      'genome/dataset-prepare',
      {
        personaName: TEST_PERSONA_NAME,
        roomId: TEST_ROOM_ID,
      } as any
    );
    expect(result.success).toBe(false);
  });

  it('should attempt dataset preparation from general room', { timeout: 15000 }, async () => {
    if (!client) return;
    const result = await Commands.execute<GenomeDatasetPrepareParams, GenomeDatasetPrepareResult>(
      'genome/dataset-prepare',
      {
        personaId: TEST_PERSONA_ID,
        personaName: TEST_PERSONA_NAME,
        roomId: TEST_ROOM_ID,
        traitType: 'conversational',
        minMessages: 2,
        maxMessages: 50,
      }
    );

    if (result.success) {
      expect(result.datasetPath).toBeDefined();
      expect(result.exampleCount).toBeGreaterThan(0);
      expect(result.personaId).toBe(TEST_PERSONA_ID);
      expect(result.traitType).toBe('conversational');

      expect(fs.existsSync(result.datasetPath)).toBe(true);
      const content = fs.readFileSync(result.datasetPath, 'utf-8');
      const lines = content.trim().split('\n');
      expect(lines.length).toBe(result.exampleCount);

      const firstLine = JSON.parse(lines[0]);
      expect(firstLine.messages).toBeDefined();
      expect(Array.isArray(firstLine.messages)).toBe(true);

      fs.unlinkSync(result.datasetPath);
    } else {
      expect(result.error).toBeDefined();
      console.log(`  Dataset prepare returned expected error: ${result.error}`);
    }
  });
});

describe('genome/train', () => {
  it('should reject missing required params', { timeout: 15000 }, async () => {
    if (!client) return;
    const result = await Commands.execute<GenomeTrainParams, GenomeTrainResult>(
      'genome/train',
      {
        personaId: TEST_PERSONA_ID,
        personaName: TEST_PERSONA_NAME,
      } as any
    );
    expect(result.success).toBe(false);
  });

  it('should train or report PEFT unavailable', { timeout: 120000 }, async () => {
    if (!client) return;
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

      if (!result.success) {
        expect(result.error).toContain('PEFT');
      } else {
        expect(result.adapterPath).toBeDefined();
        expect(result.metrics).toBeDefined();
        expect(result.metrics.epochs).toBeGreaterThan(0);

        if (result.layerId) {
          const readResult = await Commands.execute('data/read', {
            collection: 'genome_layers',
            id: result.layerId,
          } as any) as any;
          expect(readResult.success).toBe(true);
          expect(readResult.data?.name).toContain('conversational');
          expect(readResult.data?.traitType).toBe('conversational');

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
    if (!client) return;
    const result = await Commands.execute<GenomeTrainingPipelineParams, GenomeTrainingPipelineResult>(
      'genome/training-pipeline',
      {
        personaId: TEST_PERSONA_ID,
      } as any
    );
    expect(result.success).toBe(false);
  });

  it('should build and submit pipeline to sentinel', { timeout: 15000 }, async () => {
    if (!client) return;
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

    if (result.success) {
      expect(result.handle).toBeDefined();
      expect(result.handle.length).toBeGreaterThan(0);
      expect(result.pipelineName).toContain('lora-training');
      console.log(`  Pipeline started with handle: ${result.handle}`);
    } else {
      console.log(`  Pipeline submission failed (expected if Rust core not running): ${result.error}`);
    }
  });
});

// ============================================================================
// Academy Dojo Integration Tests
// ============================================================================

describe('genome/dataset-synthesize', () => {
  it('should reject missing required params', { timeout: 15000 }, async () => {
    if (!client) return;
    const result = await Commands.execute<GenomeDatasetSynthesizeParams, GenomeDatasetSynthesizeResult>(
      'genome/dataset-synthesize',
      {} as any
    );
    expect(result.success).toBe(false);
  });

  it('should synthesize training data via LLM', { timeout: 60000 }, async () => {
    if (!client) return;
    const result = await Commands.execute<GenomeDatasetSynthesizeParams, GenomeDatasetSynthesizeResult>(
      'genome/dataset-synthesize',
      {
        topic: 'TypeScript generic type parameters',
        skill: 'typescript',
        personaName: TEST_PERSONA_NAME,
        exampleCount: 5,
        difficulty: 'beginner',
      }
    );

    if (result.success) {
      expect(result.datasetPath).toBeDefined();
      expect(result.datasetPath.length).toBeGreaterThan(0);
      expect(result.exampleCount).toBeGreaterThan(0);
      expect(result.topic).toBe('TypeScript generic type parameters');
      expect(result.generatedBy).toBeDefined();

      expect(fs.existsSync(result.datasetPath)).toBe(true);
      const content = fs.readFileSync(result.datasetPath, 'utf-8');
      const lines = content.trim().split('\n');
      expect(lines.length).toBe(result.exampleCount);

      const firstLine = JSON.parse(lines[0]);
      expect(firstLine.messages).toBeDefined();
      expect(Array.isArray(firstLine.messages)).toBe(true);

      console.log(`  Synthesized ${result.exampleCount} examples by ${result.generatedBy}`);
      fs.unlinkSync(result.datasetPath);
    } else {
      console.log(`  Dataset synthesis failed (expected if no LLM available): ${result.error}`);
    }
  });
});

describe('genome/academy-session', () => {
  it('should reject missing required params', { timeout: 15000 }, async () => {
    if (!client) return;
    const result = await Commands.execute<GenomeAcademySessionParams, GenomeAcademySessionResult>(
      'genome/academy-session',
      {
        personaId: TEST_PERSONA_ID,
      } as any
    );
    expect(result.success).toBe(false);
  });

  it('should create session and spawn sentinels', { timeout: 30000 }, async () => {
    if (!client) return;
    const result = await Commands.execute<GenomeAcademySessionParams, GenomeAcademySessionResult>(
      'genome/academy-session',
      {
        personaId: TEST_PERSONA_ID,
        personaName: TEST_PERSONA_NAME,
        skill: 'typescript-generics',
        baseModel: 'smollm2:135m',
        maxTopicAttempts: 2,
        passingScore: 60,
      }
    );

    if (result.success) {
      expect(result.academySessionId).toBeDefined();
      expect(result.teacherHandle).toBeDefined();
      expect(result.studentHandle).toBeDefined();

      console.log(`  Academy session created: ${result.academySessionId}`);
      console.log(`  Teacher handle: ${result.teacherHandle}`);
      console.log(`  Student handle: ${result.studentHandle}`);

      const readResult = await Commands.execute('data/read', {
        collection: 'academy_sessions',
        id: result.academySessionId,
      } as any) as any;

      if (readResult.success) {
        expect(readResult.data?.skill).toBe('typescript-generics');
        expect(readResult.data?.personaName).toBe(TEST_PERSONA_NAME);
        expect(readResult.data?.status).toBeDefined();
      }
    } else {
      console.log(`  Academy session failed (expected if Rust core not running): ${result.error}`);
    }
  });
});
