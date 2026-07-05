#!/usr/bin/env npx tsx

/**
 * Test script for new logging entities
 *
 * Tests:
 * 1. ToolExecutionLogEntity validation
 * 2. AdapterDecisionLogEntity validation
 * 3. ResponseGenerationLogEntity validation
 */

import { ToolExecutionLogEntity } from '../system/data/entities/ToolExecutionLogEntity';
import { AdapterDecisionLogEntity } from '../system/data/entities/AdapterDecisionLogEntity';
import { ResponseGenerationLogEntity } from '../system/data/entities/ResponseGenerationLogEntity';
import { generateUUID } from '../system/core/types/CrossPlatformUUID';

console.log('🧪 Testing new logging entities...\n');

// Test 1: ToolExecutionLogEntity validation
console.log('1️⃣ Testing ToolExecutionLogEntity validation...');
const toolLog = new ToolExecutionLogEntity();
toolLog.id = generateUUID();
toolLog.personaId = generateUUID();
toolLog.personaName = 'Test Persona';
toolLog.toolName = 'test/command';
toolLog.toolParams = { key: 'value' };
toolLog.executionStatus = 'success';
toolLog.durationMs = 42;
toolLog.startedAt = Date.now() - 42;
toolLog.completedAt = Date.now();
toolLog.domain = 'chat';
toolLog.contextId = generateUUID();
toolLog.sequenceNumber = 1;

const toolValidation = toolLog.validate();
console.log(toolValidation.success ? '✅ ToolExecutionLogEntity validation passed' : `❌ ${toolValidation.error}`);

// Test 2: AdapterDecisionLogEntity validation
console.log('\n2️⃣ Testing AdapterDecisionLogEntity validation...');
const adapterLog = new AdapterDecisionLogEntity();
adapterLog.id = generateUUID();
adapterLog.personaId = generateUUID();
adapterLog.personaName = 'Test Persona';
adapterLog.adapterName = 'FastPathAdapter';
adapterLog.decision = 'RESPOND';
adapterLog.confidence = 0.9;
adapterLog.reasoning = 'User mentioned persona name';
adapterLog.decisionContext = {
  messageText: 'Hello test',
  isMentioned: true,
  senderIsHuman: true
};
adapterLog.evaluationDurationMs = 5;
adapterLog.timestamp = Date.now();
adapterLog.domain = 'chat';
adapterLog.contextId = generateUUID();
adapterLog.sequenceNumber = 1;

const adapterValidation = adapterLog.validate();
console.log(adapterValidation.success ? '✅ AdapterDecisionLogEntity validation passed' : `❌ ${adapterValidation.error}`);

// Test 3: ResponseGenerationLogEntity validation
console.log('\n3️⃣ Testing ResponseGenerationLogEntity validation...');
const responseLog = new ResponseGenerationLogEntity();
responseLog.id = generateUUID();
responseLog.personaId = generateUUID();
responseLog.personaName = 'Test Persona';
responseLog.provider = 'anthropic';
responseLog.model = 'claude-sonnet-4-5-20250929';
responseLog.promptSummary = 'System prompt + user message...';
responseLog.promptTokens = 1500;
responseLog.completionTokens = 500;
responseLog.totalTokens = 2000;
responseLog.estimatedCost = 0.015;
responseLog.responseSummary = 'Hello! I can help you with...';
responseLog.durationMs = 2500;
responseLog.status = 'success';
responseLog.temperature = 0.7;
responseLog.timestamp = Date.now();
responseLog.domain = 'chat';
responseLog.contextId = generateUUID();
responseLog.sequenceNumber = 1;

const responseValidation = responseLog.validate();
console.log(responseValidation.success ? '✅ ResponseGenerationLogEntity validation passed' : `❌ ${responseValidation.error}`);

// Summary
console.log('\n📊 Validation Summary:');
console.log(`   ToolExecutionLogEntity: ${toolValidation.success ? '✅ PASS' : '❌ FAIL'}`);
console.log(`   AdapterDecisionLogEntity: ${adapterValidation.success ? '✅ PASS' : '❌ FAIL'}`);
console.log(`   ResponseGenerationLogEntity: ${responseValidation.success ? '✅ PASS' : '❌ FAIL'}`);

const allPassed = toolValidation.success && adapterValidation.success && responseValidation.success;
console.log(allPassed ? '\n✅ All entity validations passed!' : '\n❌ Some validations failed');
process.exit(allPassed ? 0 : 1);
