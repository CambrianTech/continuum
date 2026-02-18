#!/usr/bin/env tsx

import { compareVersions, VersionAction, formatActionMessage } from '../../system/shared/VersionComparison';

console.log('🧪 Testing Version Comparison Utility\n');

const tests = [
  // Not installed
  { installed: null, rule: '>=4.0', expected: VersionAction.INSTALL },
  
  // Satisfied
  { installed: '5.0.0', rule: '>=4.0', expected: VersionAction.SATISFIED },
  { installed: '4.0.0', rule: '>=4.0', expected: VersionAction.SATISFIED },
  { installed: '4.5.2', rule: '^4.0.0', expected: VersionAction.SATISFIED },
  { installed: '4.5.2', rule: '~4.5.0', expected: VersionAction.SATISFIED },
  
  // Needs upgrade
  { installed: '3.9.9', rule: '>=4.0', expected: VersionAction.UPGRADE },
  { installed: '4.0.0', rule: '^5.0.0', expected: VersionAction.UPGRADE },
  { installed: '4.4.9', rule: '~4.5.0', expected: VersionAction.UPGRADE },
  
  // Would need downgrade
  { installed: '6.0.0', rule: '^5.0.0', expected: VersionAction.DOWNGRADE_REQUIRED },
  { installed: '4.6.0', rule: '~4.5.0', expected: VersionAction.DOWNGRADE_REQUIRED },
  
  // Invalid
  { installed: 'invalid', rule: '>=4.0', expected: VersionAction.INVALID_VERSION },
];

let passed = 0;
let failed = 0;

for (const test of tests) {
  const action = compareVersions(test.installed, test.rule);
  const message = formatActionMessage(action, test.installed, test.rule);
  const success = action === test.expected;
  
  if (success) {
    console.log(`✅ ${test.installed || 'null'} vs ${test.rule} => ${action}`);
    passed++;
  } else {
    console.log(`❌ ${test.installed || 'null'} vs ${test.rule} => ${action} (expected: ${test.expected})`);
    console.log(`   Message: ${message}`);
    failed++;
  }
}

console.log(`\n📊 Results: ${passed} passed, ${failed} failed`);
process.exit(failed > 0 ? 1 : 0);
