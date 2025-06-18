/**
 * Unit tests for command processing to prevent commands leaking into conversation
 */

const assert = require('assert');
const CommandProcessor = require('../src/core/CommandProcessor.cjs');

console.log('🧪 Testing Command Processing...\\n');

// Test 1: Proper command parsing
console.log('1️⃣  Testing proper command parsing...');
try {
  const processor = new CommandProcessor();
  
  // Test proper protocol format - should parse
  const properResponse = `
[STATUS] Processing request
[CMD:EXEC] echo "test"
[CHAT] Here is the result
`;
  
  const parsed = processor.parseAIProtocol(properResponse);
  assert.strictEqual(parsed.commands.length, 1, 'Should parse one command');
  assert.strictEqual(parsed.commands[0].command, 'EXEC', 'Should parse EXEC command');
  assert.strictEqual(parsed.commands[0].params, 'echo "test"', 'Should parse command params');
  
  console.log('✅ Proper command parsing tests passed');
} catch (error) {
  console.error('❌ Proper command parsing test failed:', error.message);
  process.exit(1);
}

// Test 2: Conversational mentions should NOT execute
console.log('\\n2️⃣  Testing conversational mentions do NOT execute...');
try {
  const processor = new CommandProcessor();
  
  // Test conversational mentions - should NOT parse as commands
  const conversationalResponse = `
Hello! I can help you with testing. Let me take a look at the repository context first.
GIT_STATUS
Okay, it looks like the repository is in a good state. I can help with:
- Running tests with EXEC commands
- Reading files with FILE_READ
- Checking status with GIT_STATUS
Just let me know what you'd like me to do!
`;
  
  const parsed = processor.parseAIProtocol(conversationalResponse);
  assert.strictEqual(parsed.commands.length, 0, 'Should NOT parse conversational mentions as commands');
  
  console.log('✅ Conversational mention prevention tests passed');
} catch (error) {
  console.error('❌ Conversational mention test failed:', error.message);
  process.exit(1);
}

// Test 3: Mixed proper and improper formats
console.log('\\n3️⃣  Testing mixed proper and improper formats...');
try {
  const processor = new CommandProcessor();
  
  const mixedResponse = `
Let me check the git status for you.
[CMD:EXEC] git status
I mentioned GIT_STATUS above but that's just conversation.
[STATUS] Checking repository
Only the [CMD:EXEC] format should execute, not the conversational GIT_STATUS mention.
`;
  
  const parsed = processor.parseAIProtocol(mixedResponse);
  assert.strictEqual(parsed.commands.length, 1, 'Should parse only properly formatted commands');
  assert.strictEqual(parsed.commands[0].command, 'EXEC', 'Should parse only the EXEC command');
  
  console.log('✅ Mixed format tests passed');
} catch (error) {
  console.error('❌ Mixed format test failed:', error.message);
  process.exit(1);
}

// Test 4: Edge cases
console.log('\\n4️⃣  Testing edge cases...');
try {
  const processor = new CommandProcessor();
  
  // Test various edge cases that should not execute
  const edgeCaseResponse = `
- You can use GIT_STATUS to check repository status
- Commands like FILE_READ: /some/file are available  
- Try WEBFETCH: https://example.com for web content
- Random GIT_STATUS mentions in text
- Even "GIT_STATUS" in quotes should not execute
- Mentioning EXEC or FILE_WRITE in conversation should not execute
`;
  
  const parsed = processor.parseAIProtocol(edgeCaseResponse);
  assert.strictEqual(parsed.commands.length, 0, 'Should not parse conversational mentions as commands');
  
  console.log('✅ Edge case tests passed');
} catch (error) {
  console.error('❌ Edge case test failed:', error.message);
  process.exit(1);
}

// Test 5: Command parameter extraction
console.log('\\n5️⃣  Testing command parameter extraction...');
try {
  const processor = new CommandProcessor();
  
  const parameterResponse = `
[CMD:FILE_READ] /path/to/file.txt
[CMD:WEBFETCH] https://example.com/api/data
[CMD:EXEC] ls -la /some/directory
`;
  
  const parsed = processor.parseAIProtocol(parameterResponse);
  assert.strictEqual(parsed.commands.length, 3, 'Should parse all three commands');
  assert.strictEqual(parsed.commands[0].params, '/path/to/file.txt', 'Should extract file path');
  assert.strictEqual(parsed.commands[1].params, 'https://example.com/api/data', 'Should extract URL');
  assert.strictEqual(parsed.commands[2].params, 'ls -la /some/directory', 'Should extract shell command');
  
  console.log('✅ Parameter extraction tests passed');
} catch (error) {
  console.error('❌ Parameter extraction test failed:', error.message);
  process.exit(1);
}

console.log('\\n🎉 All command processing tests passed!');
console.log('📊 Summary:');
console.log('  - Proper command parsing: ✅ Works correctly');
console.log('  - Conversational mentions: ✅ Prevented from executing');
console.log('  - Mixed formats: ✅ Only proper formats execute');
console.log('  - Edge cases: ✅ Handled safely');
console.log('  - Parameter extraction: ✅ Works correctly');
console.log('');
console.log('🛡️  Command leakage prevention is working correctly!');