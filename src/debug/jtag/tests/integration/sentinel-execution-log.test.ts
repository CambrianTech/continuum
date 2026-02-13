/**
 * Sentinel Execution Log Test
 *
 * Tests streaming execution logging for sentinel execution.
 */

import {
  ExecutionLogBuilder,
  formatExecutionLog,
  registerExecution,
  unregisterExecution,
  getExecutionSnapshot,
  listActiveExecutions,
  type SentinelEvent,
} from '../../system/sentinel/SentinelExecutionLog';

async function testBasicLogging(): Promise<void> {
  console.log('\n=== Test: Basic Logging ===\n');

  // Constructor params: handle, sentinelType, goal, eventEmitter?
  const builder = new ExecutionLogBuilder('test-basic', 'build', 'Fix compilation errors');
  builder.setWorkspace({ workingDir: '/tmp/test' });

  // Record some actions
  builder.recordAction({
    type: 'build',
    intent: 'Run TypeScript compilation',
    operation: 'npm run build',
    result: 'success',
    durationMs: 1234,
  });

  builder.recordAction({
    type: 'fix',
    intent: 'Fix missing import',
    operation: 'edit file.ts',
    result: 'success',
  });

  // Record file changes
  builder.recordFileChange('/tmp/test/file.ts', 'modified');
  builder.recordFileChange('/tmp/test/new-file.ts', 'created');

  // Complete returns the log
  const log = builder.complete('success');

  console.log('Log built:');
  console.log(`  handle: ${log.handle}`);
  console.log(`  sentinelType: ${log.sentinelType}`);
  console.log(`  actions: ${log.actions.length}`);
  console.log(`  fileChanges: ${log.fileChanges.length}`);
  console.log(`  status: ${log.status}`);
  console.log(`  summary: ${log.summary}`);

  // Verify
  if (log.actions.length !== 2) throw new Error('Expected 2 actions');
  if (log.fileChanges.length !== 2) throw new Error('Expected 2 file changes');
  if (log.status !== 'success') throw new Error('Expected success status');

  console.log('\n✅ Basic logging test passed!');
}

async function testEventStreaming(): Promise<void> {
  console.log('\n=== Test: Event Streaming ===\n');

  const events: SentinelEvent[] = [];

  // Constructor params: handle, sentinelType, goal, eventEmitter?
  const builder = new ExecutionLogBuilder('test-streaming', 'build', 'Fix errors', async (event) => {
    events.push(event);
    console.log(`  Event: ${event.type} - ${JSON.stringify(event.payload).substring(0, 50)}...`);
  });
  builder.setWorkspace({ workingDir: '/tmp/test' });

  await new Promise(r => setTimeout(r, 10)); // Let async event fire

  builder.recordAction({
    type: 'build',
    intent: 'Run build',
    result: 'success',
  });
  await new Promise(r => setTimeout(r, 10));

  builder.recordFileChange('/tmp/test/file.ts', 'modified');
  await new Promise(r => setTimeout(r, 10));

  builder.complete('success');
  await new Promise(r => setTimeout(r, 10));

  console.log(`\nTotal events captured: ${events.length}`);

  // Should have: status (started), action, file-change, status (completed)
  if (events.length < 4) throw new Error(`Expected at least 4 events, got ${events.length}`);

  const statusEvents = events.filter(e => e.type === 'status');
  const actionEvents = events.filter(e => e.type === 'action');
  const fileEvents = events.filter(e => e.type === 'file-change');

  console.log(`  Status events: ${statusEvents.length}`);
  console.log(`  Action events: ${actionEvents.length}`);
  console.log(`  File events: ${fileEvents.length}`);

  console.log('\n✅ Event streaming test passed!');
}

async function testSnapshotAndRegistry(): Promise<void> {
  console.log('\n=== Test: Snapshot & Registry ===\n');

  // Verify nothing registered initially
  let active = listActiveExecutions();
  console.log(`Initial active executions: ${active.length}`);

  // Create and register
  const builder = new ExecutionLogBuilder('test-snapshot', 'build', 'Test build');
  builder.setWorkspace({ workingDir: '/tmp/test' });
  registerExecution(builder);  // Takes just the builder

  active = listActiveExecutions();
  console.log(`After register: ${active.length}`);

  if (active.length === 0 || !active.includes('test-snapshot')) {
    throw new Error('Expected test-snapshot to be registered');
  }

  // Add some data
  builder.recordAction({ type: 'build', intent: 'Run tsc', result: 'success' });

  // Get snapshot (like "joining" a running process)
  const snapshot = getExecutionSnapshot('test-snapshot');
  console.log('\nSnapshot:');
  console.log(`  handle: ${snapshot?.handle}`);
  console.log(`  inProgress: ${snapshot?.inProgress}`);
  console.log(`  actions: ${snapshot?.actions?.length}`);

  if (!snapshot) throw new Error('Expected snapshot');
  if (!snapshot.inProgress) throw new Error('Expected inProgress: true');

  // Complete and unregister
  builder.complete('success');
  unregisterExecution('test-snapshot');

  active = listActiveExecutions();
  console.log(`\nAfter unregister: ${active.length}`);

  if (active.includes('test-snapshot')) {
    throw new Error('test-snapshot should have been unregistered');
  }

  console.log('\n✅ Snapshot & Registry test passed!');
}

async function testFormatting(): Promise<void> {
  console.log('\n=== Test: Log Formatting ===\n');

  const builder = new ExecutionLogBuilder('test-format', 'build', 'Build the project');
  builder.setWorkspace({ workingDir: '/tmp/test' });
  builder.recordAction({ type: 'build', intent: 'Run TypeScript build', operation: 'tsc --build', result: 'success', durationMs: 5432 });
  builder.recordAction({ type: 'fix', intent: 'Add missing import', result: 'success' });
  builder.recordFileChange('/tmp/test/src/index.ts', 'modified');

  const log = builder.complete('success');
  const formatted = formatExecutionLog(log);

  console.log('Formatted log:\n');
  console.log(formatted);

  // Basic formatting checks
  if (!formatted.includes('test-format')) throw new Error('Should include handle');
  if (!formatted.includes('SUCCESS')) throw new Error('Should indicate success');
  if (!formatted.includes('Run TypeScript build')) throw new Error('Should include intent');

  console.log('\n✅ Log formatting test passed!');
}

async function main(): Promise<void> {
  console.log('Sentinel Execution Log Tests');
  console.log('============================\n');

  try {
    await testBasicLogging();
    await testEventStreaming();
    await testSnapshotAndRegistry();
    await testFormatting();

    console.log('\n\n==================================');
    console.log('✅ All execution log tests passed!');
    console.log('==================================\n');
  } catch (err) {
    console.error('\n❌ Test failed:', err);
    process.exit(1);
  }
}

main();
