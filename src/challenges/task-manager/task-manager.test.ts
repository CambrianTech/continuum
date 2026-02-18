/**
 * TaskManager Tests
 *
 * Run with: npx tsx challenges/task-manager/task-manager.test.ts
 *
 * These tests verify the TaskManager module works correctly.
 * Currently some tests are FAILING — find and fix the bugs!
 */

import { TaskManager } from './task-manager';
import assert from 'node:assert';

let passed = 0;
let failed = 0;

function test(name: string, fn: () => void) {
  try {
    fn();
    console.log(`  ✅ ${name}`);
    passed++;
  } catch (err: any) {
    console.log(`  ❌ ${name}`);
    console.log(`     ${err.message}`);
    failed++;
  }
}

console.log('TaskManager Tests\n');

// ── Setup ──
const tm = new TaskManager();

// ── Test: Adding tasks ──
console.log('Adding tasks:');

const t1 = tm.add('Buy groceries', 'high');
test('first task gets id=1', () => {
  assert.strictEqual(t1.id, 1);
});

const t2 = tm.add('Write documentation', 'low');
const t3 = tm.add('Fix critical bug', 'high');
const t4 = tm.add('Update dependencies', 'medium');

test('four tasks added', () => {
  assert.strictEqual(tm.count, 4);
});

// ── Test: Completing tasks ──
console.log('\nCompleting tasks:');

test('complete existing task returns true', () => {
  assert.strictEqual(tm.complete(1), true);
});

test('complete non-existent task returns false', () => {
  assert.strictEqual(tm.complete(999), false);
});

// ── Test: Priority filtering ──
console.log('\nPriority filtering:');

test('getByPriority("high") returns only high-priority tasks', () => {
  const highTasks = tm.getByPriority('high');
  assert.strictEqual(highTasks.length, 2, `Expected 2 high-priority tasks, got ${highTasks.length}`);
  assert.ok(
    highTasks.every(t => t.priority === 'high'),
    `Not all returned tasks are high priority: ${highTasks.map(t => `${t.title}(${t.priority})`).join(', ')}`
  );
});

test('getByPriority("low") returns only low-priority tasks', () => {
  const lowTasks = tm.getByPriority('low');
  assert.strictEqual(lowTasks.length, 1, `Expected 1 low-priority task, got ${lowTasks.length}`);
  assert.strictEqual(lowTasks[0].title, 'Write documentation');
});

// ── Test: Pending/Completed filtering ──
console.log('\nPending/Completed filtering:');

test('getCompleted returns only completed tasks', () => {
  const completed = tm.getCompleted();
  assert.strictEqual(completed.length, 1, `Expected 1 completed task, got ${completed.length}`);
  assert.strictEqual(completed[0].title, 'Buy groceries');
});

test('getPending returns only non-completed tasks', () => {
  const pending = tm.getPending();
  assert.strictEqual(pending.length, 3, `Expected 3 pending tasks, got ${pending.length}`);
  assert.ok(
    pending.every(t => !t.completed),
    `Some returned tasks are completed: ${pending.filter(t => t.completed).map(t => t.title).join(', ')}`
  );
});

// ── Test: Remove completed ──
console.log('\nRemove completed:');

test('removeCompleted removes only completed tasks', () => {
  const removedCount = tm.removeCompleted();
  assert.strictEqual(removedCount, 1, `Expected 1 removed, got ${removedCount}`);
});

test('after removal, only pending tasks remain', () => {
  assert.strictEqual(tm.count, 3, `Expected 3 remaining tasks, got ${tm.count}`);
  const remaining = tm.getCompleted();
  assert.strictEqual(remaining.length, 0, 'No completed tasks should remain');
});

// ── Summary ──
console.log(`\n${'─'.repeat(40)}`);
console.log(`Results: ${passed} passed, ${failed} failed`);
if (failed > 0) {
  console.log('\n⚠️  Some tests failed! Find and fix the bugs in task-manager.ts');
  process.exit(1);
} else {
  console.log('\n🎉 All tests passed!');
}
