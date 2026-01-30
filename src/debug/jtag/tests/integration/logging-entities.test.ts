/**
 * Integration Test: Logging Entities
 *
 * Tests the three new logging entities for Phase 2 cognition observability:
 * - ToolExecutionLogEntity
 * - AdapterDecisionLogEntity
 * - ResponseGenerationLogEntity
 *
 * Verifies:
 * 1. Entity validation
 * 2. Database CRUD operations
 * 3. Collection registration
 */

import { describe, it, expect, beforeAll } from 'vitest';
import { Commands } from '../../system/core/shared/Commands';
import { ToolExecutionLogEntity } from '../../system/data/entities/ToolExecutionLogEntity';
import { AdapterDecisionLogEntity } from '../../system/data/entities/AdapterDecisionLogEntity';
import { ResponseGenerationLogEntity } from '../../system/data/entities/ResponseGenerationLogEntity';
import { COLLECTIONS } from '../../system/shared/Constants';
import { generateUUID } from '../../system/core/types/CrossPlatformUUID';
import type { UUID } from '../../system/core/types/CrossPlatformUUID';
import type { DataListParams, DataListResult } from '../../commands/data/list/shared/DataListTypes';
import type { DataCreateParams, DataCreateResult } from '../../commands/data/create/shared/DataCreateTypes';
import { DATA_COMMANDS } from '@commands/data/shared/DataCommandConstants';

import { DataCreate } from '../../commands/data/create/shared/DataCreateTypes';
import { DataList } from '../../commands/data/list/shared/DataListTypes';
describe('Logging Entities Integration Test', () => {
  let testPersonaId: UUID;
  let testContextId: UUID;

  beforeAll(() => {
    testPersonaId = generateUUID();
    testContextId = generateUUID();
  });

  describe('ToolExecutionLogEntity', () => {
    it('should validate correctly', () => {
      const entity = new ToolExecutionLogEntity();
      entity.id = generateUUID();
      entity.personaId = testPersonaId;
      entity.personaName = 'Test Persona';
      entity.toolName = 'test/command';
      entity.toolParams = { key: 'value' };
      entity.executionStatus = 'success';
      entity.durationMs = 42;
      entity.startedAt = Date.now() - 42;
      entity.completedAt = Date.now();
      entity.domain = 'chat';
      entity.contextId = testContextId;
      entity.sequenceNumber = 1;

      const validation = entity.validate();
      expect(validation.success).toBe(true);
    });

    it('should create and retrieve from database', async () => {
      const entity = new ToolExecutionLogEntity();
      entity.id = generateUUID();
      entity.personaId = testPersonaId;
      entity.personaName = 'Test Persona';
      entity.toolName = 'screenshot';
      entity.toolParams = { querySelector: 'body' };
      entity.executionStatus = 'success';
      entity.toolResult = { success: true, filename: 'test.png' };
      entity.durationMs = 125;
      entity.startedAt = Date.now() - 125;
      entity.completedAt = Date.now();
      entity.domain = 'chat';
      entity.contextId = testContextId;
      entity.sequenceNumber = 1;

      // Create
      const createResult = await DataCreate.execute({
        collection: COLLECTIONS.TOOL_EXECUTION_LOGS,
        data: entity
      });
      expect(createResult.success).toBe(true);

      // Retrieve
      const listResult = await DataList.execute<ToolExecutionLogEntity>({
        collection: COLLECTIONS.TOOL_EXECUTION_LOGS,
        filter: { personaId: testPersonaId },
        limit: 10
      });
      expect(listResult.success).toBe(true);
      expect(listResult.items).toBeDefined();
      expect(listResult.items!.length).toBeGreaterThan(0);

      const retrieved = listResult.items![0];
      expect(retrieved.toolName).toBe('screenshot');
      expect(retrieved.executionStatus).toBe('success');
    });
  });

  describe('AdapterDecisionLogEntity', () => {
    it('should validate correctly', () => {
      const entity = new AdapterDecisionLogEntity();
      entity.id = generateUUID();
      entity.personaId = testPersonaId;
      entity.personaName = 'Test Persona';
      entity.adapterName = 'FastPathAdapter';
      entity.decision = 'RESPOND';
      entity.confidence = 0.9;
      entity.reasoning = 'User mentioned persona name';
      entity.decisionContext = {
        messageText: 'Hello test',
        isMentioned: true,
        senderIsHuman: true
      };
      entity.evaluationDurationMs = 5;
      entity.timestamp = Date.now();
      entity.domain = 'chat';
      entity.contextId = testContextId;
      entity.sequenceNumber = 1;

      const validation = entity.validate();
      expect(validation.success).toBe(true);
    });

    it('should create and retrieve from database', async () => {
      const entity = new AdapterDecisionLogEntity();
      entity.id = generateUUID();
      entity.personaId = testPersonaId;
      entity.personaName = 'Test Persona';
      entity.adapterName = 'ThermalAdapter';
      entity.decision = 'SILENT';
      entity.confidence = 0.7;
      entity.reasoning = 'Cognitive load too high';
      entity.decisionContext = {
        messageText: 'What do you think?',
        cognitiveLoad: 0.85,
        recentMessageCount: 15
      };
      entity.evaluationDurationMs = 12;
      entity.timestamp = Date.now();
      entity.domain = 'chat';
      entity.contextId = testContextId;
      entity.sequenceNumber = 2;

      // Create
      const createResult = await DataCreate.execute({
        collection: COLLECTIONS.ADAPTER_DECISION_LOGS,
        data: entity
      });
      expect(createResult.success).toBe(true);

      // Retrieve
      const listResult = await DataList.execute<AdapterDecisionLogEntity>({
        collection: COLLECTIONS.ADAPTER_DECISION_LOGS,
        filter: { personaId: testPersonaId },
        limit: 10
      });
      expect(listResult.success).toBe(true);
      expect(listResult.items).toBeDefined();
      expect(listResult.items!.length).toBeGreaterThan(0);

      const retrieved = listResult.items![0];
      expect(retrieved.adapterName).toBe('ThermalAdapter');
      expect(retrieved.decision).toBe('SILENT');
    });
  });

  describe('ResponseGenerationLogEntity', () => {
    it('should validate correctly', () => {
      const entity = new ResponseGenerationLogEntity();
      entity.id = generateUUID();
      entity.personaId = testPersonaId;
      entity.personaName = 'Test Persona';
      entity.provider = 'anthropic';
      entity.model = 'claude-sonnet-4-5-20250929';
      entity.promptSummary = 'System prompt + user message...';
      entity.promptTokens = 1500;
      entity.completionTokens = 500;
      entity.totalTokens = 2000;
      entity.estimatedCost = 0.015;
      entity.responseSummary = 'Hello! I can help you with...';
      entity.durationMs = 2500;
      entity.status = 'success';
      entity.temperature = 0.7;
      entity.timestamp = Date.now();
      entity.domain = 'chat';
      entity.contextId = testContextId;
      entity.sequenceNumber = 1;

      const validation = entity.validate();
      expect(validation.success).toBe(true);
    });

    it('should create and retrieve from database', async () => {
      const entity = new ResponseGenerationLogEntity();
      entity.id = generateUUID();
      entity.personaId = testPersonaId;
      entity.personaName = 'Test Persona';
      entity.provider = 'openai';
      entity.model = 'gpt-4';
      entity.promptSummary = 'You are a helpful assistant...';
      entity.promptTokens = 800;
      entity.completionTokens = 300;
      entity.totalTokens = 1100;
      entity.estimatedCost = 0.008;
      entity.responseSummary = 'I understand your question...';
      entity.durationMs = 1800;
      entity.status = 'success';
      entity.temperature = 0.7;
      entity.timestamp = Date.now();
      entity.domain = 'chat';
      entity.contextId = testContextId;
      entity.sequenceNumber = 3;

      // Create
      const createResult = await DataCreate.execute({
        collection: COLLECTIONS.RESPONSE_GENERATION_LOGS,
        data: entity
      });
      expect(createResult.success).toBe(true);

      // Retrieve
      const listResult = await DataList.execute<ResponseGenerationLogEntity>({
        collection: COLLECTIONS.RESPONSE_GENERATION_LOGS,
        filter: { personaId: testPersonaId },
        limit: 10
      });
      expect(listResult.success).toBe(true);
      expect(listResult.items).toBeDefined();
      expect(listResult.items!.length).toBeGreaterThan(0);

      const retrieved = listResult.items![0];
      expect(retrieved.provider).toBe('openai');
      expect(retrieved.model).toBe('gpt-4');
      expect(retrieved.status).toBe('success');
    });
  });

  describe('Collection Registration', () => {
    it('should have all three collections available', async () => {
      // Verify collections are registered by attempting to list (even if empty)
      const toolLogs = await DataList.execute<ToolExecutionLogEntity>({
        collection: COLLECTIONS.TOOL_EXECUTION_LOGS,
        limit: 1
      });
      expect(toolLogs.collection).toBe(COLLECTIONS.TOOL_EXECUTION_LOGS);

      const adapterLogs = await DataList.execute<AdapterDecisionLogEntity>({
        collection: COLLECTIONS.ADAPTER_DECISION_LOGS,
        limit: 1
      });
      expect(adapterLogs.collection).toBe(COLLECTIONS.ADAPTER_DECISION_LOGS);

      const responseLogs = await DataList.execute<ResponseGenerationLogEntity>({
        collection: COLLECTIONS.RESPONSE_GENERATION_LOGS,
        limit: 1
      });
      expect(responseLogs.collection).toBe(COLLECTIONS.RESPONSE_GENERATION_LOGS);
    });
  });
});
