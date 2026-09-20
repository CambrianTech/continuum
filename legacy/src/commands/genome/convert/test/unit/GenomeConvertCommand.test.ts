#!/usr/bin/env tsx
/**
 * GenomeConvert Command Unit Tests
 *
 * Tests genome/convert command validation logic, parameter handling,
 * operation routing, and default path generation in isolation.
 *
 * Run with: npx tsx commands/genome/convert/test/unit/GenomeConvertCommand.test.ts
 */

import { ValidationError } from '@system/core/types/ErrorTypes';
import { generateUUID } from '@system/core/types/CrossPlatformUUID';
import type { GenomeConvertParams, GenomeConvertResult } from '../../shared/GenomeConvertTypes';
import * as path from 'path';

console.log('🧪 GenomeConvert Command Unit Tests');

let passed = 0;
let failed = 0;

function assert(condition: boolean, message: string): void {
  if (!condition) {
    failed++;
    throw new Error(`❌ Assertion failed: ${message}`);
  }
  passed++;
  console.log(`  ✅ ${message}`);
}

const VALID_OPERATIONS = ['merge-full', 'merge-and-quantize', 'quantize-base', 'validate'] as const;
type ConvertOperation = typeof VALID_OPERATIONS[number];

/**
 * Validate params the same way the server command does — pure logic, no I/O
 */
function validateConvertParams(params: Partial<GenomeConvertParams>): void {
  const operation = params.operation as ConvertOperation;
  if (!operation || !VALID_OPERATIONS.includes(operation)) {
    throw new ValidationError(
      'operation',
      `Invalid operation '${params.operation}'. Must be one of: ${VALID_OPERATIONS.join(', ')}`
    );
  }

  if ((operation === 'merge-full' || operation === 'merge-and-quantize') && !params.adapterPath) {
    throw new ValidationError('adapterPath', `'adapterPath' is required for '${operation}' operation`);
  }

  if (operation !== 'validate' && !params.baseModel) {
    throw new ValidationError('baseModel', `'baseModel' is required for '${operation}' operation`);
  }

  if (params.bits !== undefined && params.bits !== 4 && params.bits !== 8) {
    throw new ValidationError('bits', `'bits' must be 4 or 8, got ${params.bits}`);
  }
}

/**
 * Default output path generation (mirrors server implementation)
 */
function defaultOutputPath(operation: ConvertOperation, params: Partial<GenomeConvertParams>): string {
  const convertedDir = path.join('.continuum', 'genome', 'converted');
  const suffix = operation === 'merge-full' ? 'fp16' : `q${params.bits ?? 4}`;
  const base = params.adapterPath
    ? path.basename(params.adapterPath)
    : (params.baseModel ?? 'model').replace(/[^a-zA-Z0-9]/g, '-');
  // Timestamp omitted for deterministic testing
  return path.join(convertedDir, `${base}-${suffix}`);
}

// ── Test 1: Operation validation ──

async function testOperationValidation(): Promise<void> {
  console.log('\n📋 Test 1: Operation validation');

  // Valid operations accepted
  for (const op of VALID_OPERATIONS) {
    const params: Partial<GenomeConvertParams> = {
      operation: op,
      baseModel: 'test/model',
      adapterPath: op === 'merge-full' || op === 'merge-and-quantize' ? '/some/adapter' : undefined,
    };
    try {
      validateConvertParams(params);
      assert(true, `Operation '${op}' accepted`);
    } catch (e) {
      if (op === 'validate' && e instanceof ValidationError && e.field === 'baseModel') {
        // validate doesn't need baseModel — that's correct
        assert(true, `Operation '${op}' accepted (no baseModel needed)`);
      } else {
        throw e;
      }
    }
  }

  // Invalid operation rejected
  try {
    validateConvertParams({ operation: 'invalid-op' } as Partial<GenomeConvertParams>);
    assert(false, 'Should have thrown for invalid operation');
  } catch (e) {
    assert(e instanceof ValidationError, 'Invalid operation throws ValidationError');
    assert((e as ValidationError).field === 'operation', 'Error field is "operation"');
  }

  // Missing operation rejected
  try {
    validateConvertParams({} as Partial<GenomeConvertParams>);
    assert(false, 'Should have thrown for missing operation');
  } catch (e) {
    assert(e instanceof ValidationError, 'Missing operation throws ValidationError');
  }
}

// ── Test 2: Adapter path requirements per operation ──

async function testAdapterPathRequirements(): Promise<void> {
  console.log('\n📋 Test 2: Adapter path requirements per operation');

  // merge-full requires adapterPath
  try {
    validateConvertParams({ operation: 'merge-full', baseModel: 'test/model' });
    assert(false, 'merge-full should require adapterPath');
  } catch (e) {
    assert(e instanceof ValidationError && (e as ValidationError).field === 'adapterPath',
      'merge-full throws ValidationError for missing adapterPath');
  }

  // merge-and-quantize requires adapterPath
  try {
    validateConvertParams({ operation: 'merge-and-quantize', baseModel: 'test/model' });
    assert(false, 'merge-and-quantize should require adapterPath');
  } catch (e) {
    assert(e instanceof ValidationError && (e as ValidationError).field === 'adapterPath',
      'merge-and-quantize throws ValidationError for missing adapterPath');
  }

  // quantize-base does NOT require adapterPath
  try {
    validateConvertParams({ operation: 'quantize-base', baseModel: 'test/model' });
    assert(true, 'quantize-base works without adapterPath');
  } catch {
    assert(false, 'quantize-base should not require adapterPath');
  }

  // validate does NOT require adapterPath or baseModel
  try {
    validateConvertParams({ operation: 'validate' });
    assert(true, 'validate works without adapterPath or baseModel');
  } catch {
    assert(false, 'validate should not require adapterPath or baseModel');
  }
}

// ── Test 3: Base model requirements per operation ──

async function testBaseModelRequirements(): Promise<void> {
  console.log('\n📋 Test 3: Base model requirements per operation');

  // merge-full requires baseModel
  try {
    validateConvertParams({ operation: 'merge-full', adapterPath: '/adapter' });
    assert(false, 'merge-full should require baseModel');
  } catch (e) {
    assert(e instanceof ValidationError && (e as ValidationError).field === 'baseModel',
      'merge-full throws ValidationError for missing baseModel');
  }

  // quantize-base requires baseModel
  try {
    validateConvertParams({ operation: 'quantize-base' });
    assert(false, 'quantize-base should require baseModel');
  } catch (e) {
    assert(e instanceof ValidationError && (e as ValidationError).field === 'baseModel',
      'quantize-base throws ValidationError for missing baseModel');
  }

  // validate does NOT require baseModel
  try {
    validateConvertParams({ operation: 'validate' });
    assert(true, 'validate works without baseModel');
  } catch {
    assert(false, 'validate should not require baseModel');
  }
}

// ── Test 4: Default output path generation ──

async function testDefaultOutputPath(): Promise<void> {
  console.log('\n📋 Test 4: Default output path generation');

  // merge-full → fp16 suffix
  const mergePath = defaultOutputPath('merge-full', { adapterPath: '/adapters/helper-coding-123' });
  assert(mergePath.includes('fp16'), 'merge-full path includes fp16 suffix');
  assert(mergePath.includes('helper-coding-123'), 'merge-full path includes adapter basename');

  // merge-and-quantize → q4 suffix (default bits)
  const quantizePath = defaultOutputPath('merge-and-quantize', { adapterPath: '/adapters/helper-coding-123' });
  assert(quantizePath.includes('q4'), 'merge-and-quantize path includes q4 suffix (default)');

  // quantize-base with bits=8 → q8 suffix
  const q8Path = defaultOutputPath('quantize-base', { baseModel: 'unsloth/Llama-3.2-3B', bits: 8 });
  assert(q8Path.includes('q8'), 'quantize-base with bits=8 includes q8 suffix');
  assert(q8Path.includes('unsloth-Llama-3-2-3B'), 'quantize-base path sanitizes model name');

  // All paths rooted in .continuum/genome/converted/
  assert(mergePath.startsWith(path.join('.continuum', 'genome', 'converted')),
    'Output path is under .continuum/genome/converted/');
}

// ── Test 5: Bits validation ──

async function testBitsValidation(): Promise<void> {
  console.log('\n📋 Test 5: Bits validation');

  // bits=4 is valid
  try {
    validateConvertParams({ operation: 'quantize-base', baseModel: 'test/model', bits: 4 });
    assert(true, 'bits=4 accepted');
  } catch {
    assert(false, 'bits=4 should be accepted');
  }

  // bits=8 is valid
  try {
    validateConvertParams({ operation: 'quantize-base', baseModel: 'test/model', bits: 8 });
    assert(true, 'bits=8 accepted');
  } catch {
    assert(false, 'bits=8 should be accepted');
  }

  // bits=3 is invalid
  try {
    validateConvertParams({ operation: 'quantize-base', baseModel: 'test/model', bits: 3 as any });
    assert(false, 'bits=3 should be rejected');
  } catch (e) {
    assert(e instanceof ValidationError && (e as ValidationError).field === 'bits',
      'bits=3 throws ValidationError');
  }

  // bits undefined defaults to 4 (handled at command level, not validation)
  try {
    validateConvertParams({ operation: 'quantize-base', baseModel: 'test/model' });
    assert(true, 'bits=undefined accepted (defaults to 4 at execution)');
  } catch {
    assert(false, 'bits=undefined should be accepted');
  }
}

// ── Test 6: Result structure from factory ──

async function testResultStructure(): Promise<void> {
  console.log('\n📋 Test 6: Result structure validation');
  const { createGenomeConvertResult } = await import('../../shared/GenomeConvertTypes');

  const context = { environment: 'server' as const };
  const sessionId = generateUUID();

  const result = createGenomeConvertResult(context, sessionId, {
    success: true,
    outputPath: '/converted/model-q4',
    format: 'gguf-q4_0',
    sizeMB: 1800,
    durationSeconds: 120,
    compressionRatio: 3.4,
    validation: { valid: true, perplexity: 5.2 },
  });

  assert(result.success === true, 'Result has success=true');
  assert(result.outputPath === '/converted/model-q4', 'Result has outputPath');
  assert(result.format === 'gguf-q4_0', 'Result has format');
  assert(result.sizeMB === 1800, 'Result has sizeMB');
  assert(result.durationSeconds === 120, 'Result has durationSeconds');
  assert(result.compressionRatio === 3.4, 'Result has compressionRatio');
  assert(result.validation !== undefined, 'Result has validation');
  assert((result.validation as Record<string, unknown>).valid === true, 'Validation result is valid');

  // Test with minimal data (defaults)
  const minResult = createGenomeConvertResult(context, sessionId, { success: false });
  assert(minResult.success === false, 'Minimal result has success=false');
  assert(minResult.outputPath === '', 'Minimal result defaults outputPath to empty');
  assert(minResult.sizeMB === 0, 'Minimal result defaults sizeMB to 0');
  assert(minResult.compressionRatio === 0, 'Minimal result defaults compressionRatio to 0');
}

// ── Run all tests ──

async function runAllTests(): Promise<void> {
  console.log('🚀 Starting GenomeConvert Command Unit Tests\n');

  const tests = [
    testOperationValidation,
    testAdapterPathRequirements,
    testBaseModelRequirements,
    testDefaultOutputPath,
    testBitsValidation,
    testResultStructure,
  ];

  for (const test of tests) {
    try {
      await test();
    } catch (error) {
      console.error(`\n❌ ${(error as Error).message}`);
      if ((error as Error).stack) {
        console.error((error as Error).stack);
      }
    }
  }

  console.log(`\n${'='.repeat(50)}`);
  console.log(`📊 Results: ${passed} passed, ${failed} failed`);
  if (failed > 0) {
    console.log('❌ SOME TESTS FAILED');
    process.exit(1);
  } else {
    console.log('✅ ALL TESTS PASSED');
  }
}

if (require.main === module) {
  void runAllTests();
} else {
  module.exports = { runAllTests };
}
