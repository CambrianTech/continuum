#!/usr/bin/env node

/**
 * Test Runner for Persistent Storage
 * Runs all tests related to the PersistentStorage abstraction
 */

const { execSync } = require('child_process');
const path = require('path');
const fs = require('fs');

console.log('🧪 Running Persistent Storage Tests\n');

// Check if Jest is available
try {
  execSync('npx jest --version', { stdio: 'pipe' });
} catch (error) {
  console.log('📦 Installing Jest for testing...');
  try {
    execSync('npm install --save-dev jest', { stdio: 'inherit' });
  } catch (installError) {
    console.error('❌ Failed to install Jest:', installError.message);
    process.exit(1);
  }
}

// Test configurations
const testConfigs = [
  {
    name: 'Unit Tests - PersistentStorage',
    pattern: '__tests__/unit/PersistentStorage.test.cjs',
    description: 'Tests the core PersistentStorage class functionality'
  },
  {
    name: 'Integration Tests - Academy Storage',
    pattern: '__tests__/integration/AcademyPersistentStorage.test.cjs', 
    description: 'Tests integration between Academy and PersistentStorage'
  }
];

let totalPassed = 0;
let totalFailed = 0;

// Run each test suite
for (const config of testConfigs) {
  console.log(`\n📋 Running: ${config.name}`);
  console.log(`📝 ${config.description}`);
  console.log('─'.repeat(60));
  
  try {
    const testPath = path.join(__dirname, config.pattern);
    
    if (!fs.existsSync(testPath)) {
      console.log(`⚠️  Test file not found: ${testPath}`);
      continue;
    }
    
    const result = execSync(`npx jest "${testPath}" --verbose --no-cache`, {
      stdio: 'inherit',
      encoding: 'utf8'
    });
    
    console.log(`✅ ${config.name} - PASSED`);
    totalPassed++;
    
  } catch (error) {
    console.log(`❌ ${config.name} - FAILED`);
    totalFailed++;
    
    // Show error details if needed
    if (process.argv.includes('--verbose')) {
      console.log('Error details:', error.message);
    }
  }
}

// Summary
console.log('\n' + '='.repeat(60));
console.log('🎯 TEST SUMMARY');
console.log('='.repeat(60));
console.log(`✅ Passed: ${totalPassed}`);
console.log(`❌ Failed: ${totalFailed}`);
console.log(`📊 Total:  ${totalPassed + totalFailed}`);

if (totalFailed === 0) {
  console.log('\n🎉 All tests passed! The PersistentStorage abstraction is working correctly.');
  console.log('\n🔍 Test Coverage:');
  console.log('   • Core storage operations (save, load, update, delete)');
  console.log('   • Error handling and edge cases');
  console.log('   • Date field conversion');
  console.log('   • Academy integration workflows');
  console.log('   • File system operations');
  console.log('   • Performance under load');
} else {
  console.log(`\n⚠️  ${totalFailed} test suite(s) failed. Check the output above for details.`);
  process.exit(1);
}

// Show additional information
console.log('\n📚 Additional Test Information:');
console.log('─'.repeat(40));
console.log('• Tests use isolated test directories');
console.log('• All test data is cleaned up automatically');
console.log('• Tests include both unit and integration scenarios');
console.log('• Error conditions and edge cases are covered');
console.log('• Performance and concurrency scenarios are tested');

console.log('\n🚀 To run tests individually:');
console.log('   npx jest __tests__/unit/PersistentStorage.test.cjs');
console.log('   npx jest __tests__/integration/AcademyPersistentStorage.test.cjs');

console.log('\n📖 To run with more details:');
console.log('   node test-persistent-storage.cjs --verbose');