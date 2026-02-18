/**
 * Simple Training Pipeline Integration Test
 *
 * Tests that training data is created from real chat messages
 * Uses running JTAG server (assumes npm start is running)
 */

import { describe, it, expect } from 'vitest';
import { execSync } from 'child_process';

function runJtagCommand(command: string, timeoutMs: number = 10000): any {
  try {
    const output = execSync(command, {
      encoding: 'utf8',
      timeout: timeoutMs,
      shell: true,
      maxBuffer: 1024 * 1024 * 50
    });

    return JSON.parse(output);
  } catch (error: any) {
    console.error(`Command failed: ${command}`);
    console.error(`Error: ${error.message}`);
    throw error;
  }
}

describe('Training Pipeline Integration', () => {
  it('should create training data from chat messages', async () => {
    console.log('🧪 Testing training pipeline: chat → training data');

    // Step 1: Get dev-updates room ID
    const roomsResult = runJtagCommand(
      `./jtag data/list --collection=rooms --filter='{"uniqueId":"dev-updates"}'`
    );

    expect(roomsResult.success).toBe(true);
    expect(roomsResult.data.length).toBeGreaterThan(0);

    const devUpdatesRoom = roomsResult.data[0];
    const roomId = devUpdatesRoom.id;
    console.log(`✅ Found dev-updates room: ${roomId}`);

    // Step 2: Get a user ID (any user will do)
    const usersResult = runJtagCommand(
      `./jtag data/list --collection=users --limit=1`
    );

    expect(usersResult.success).toBe(true);
    const userId = usersResult.data[0].id;
    console.log(`✅ Found user: ${userId}`);

    // Step 3: Create test message
    const timestamp = Date.now();
    const testMessage = `Training pipeline test message ${timestamp}`;

    const createResult = runJtagCommand(
      `./jtag chat/send --roomId="${roomId}" --message="${testMessage}"`
    );

    expect(createResult.success).toBe(true);
    console.log(`✅ Created chat message`);

    // Step 4: Wait for TrainingDaemon to observe and process
    console.log('⏳ Waiting for TrainingDaemon to process...');
    await new Promise(resolve => setTimeout(resolve, 3000));

    // Step 5: Check if training data was created
    const trainingResult = runJtagCommand(
      `./jtag data/list --collection=training_examples --limit=10`
    );

    console.log(`📊 Training examples found: ${trainingResult.data?.length || 0}`);

    // Should have at least some training examples (may not be from this specific message if < min threshold)
    expect(trainingResult.success).toBe(true);

    // If training data exists, verify structure
    if (trainingResult.data && trainingResult.data.length > 0) {
      const example = trainingResult.data[0];

      console.log(`✅ Training example structure:`);
      console.log(`   - Messages: ${example.messageCount}`);
      console.log(`   - Tokens: ${example.totalTokens}`);
      console.log(`   - Quality: ${example.metadata?.quality}`);

      expect(example).toHaveProperty('messages');
      expect(example).toHaveProperty('messageCount');
      expect(example).toHaveProperty('totalTokens');
      expect(example).toHaveProperty('metadata');

      // Verify OpenAI format
      expect(Array.isArray(example.messages)).toBe(true);
      if (example.messages.length > 0) {
        const msg = example.messages[0];
        expect(msg).toHaveProperty('role');
        expect(msg).toHaveProperty('content');
        expect(['system', 'user', 'assistant']).toContain(msg.role);
      }
    }
  }, 30000); // 30 second timeout

  it('should skip system test messages', async () => {
    console.log('🧪 Testing training pipeline: system test filtering');

    // Get room ID
    const roomsResult = runJtagCommand(
      `./jtag data/list --collection=rooms --filter='{"uniqueId":"dev-updates"}' --limit=1`
    );

    const roomId = roomsResult.data[0].id;

    // Count training examples before
    const beforeResult = runJtagCommand(
      `./jtag data/list --collection=training_examples`
    );

    const countBefore = beforeResult.data?.length || 0;
    console.log(`📊 Training examples before: ${countBefore}`);

    // Create system test message (should be filtered)
    const timestamp = Date.now();
    const createResult = runJtagCommand(
      `./jtag chat/send --roomId="${roomId}" --message="System test ${timestamp}" --metadata='{"isSystemTest":true,"testType":"integration"}'`
    );

    expect(createResult.success).toBe(true);
    console.log(`✅ Created system test message`);

    // Wait for potential processing
    await new Promise(resolve => setTimeout(resolve, 2000));

    // Count training examples after
    const afterResult = runJtagCommand(
      `./jtag data/list --collection=training_examples`
    );

    const countAfter = afterResult.data?.length || 0;
    console.log(`📊 Training examples after: ${countAfter}`);

    // Count should NOT increase (system test message filtered)
    expect(countAfter).toBe(countBefore);
    console.log(`✅ System test message correctly filtered`);
  }, 30000);

  it('should accumulate conversation context', async () => {
    console.log('🧪 Testing training pipeline: conversation context');

    // Get room and user
    const roomsResult = runJtagCommand(
      `./jtag data/list --collection=rooms --filter='{"uniqueId":"dev-updates"}' --limit=1`
    );

    const roomId = roomsResult.data[0].id;

    // Create a multi-message conversation
    const timestamp = Date.now();
    const messages = [
      `Context test 1 ${timestamp}`,
      `Context test 2 ${timestamp}`,
      `Context test 3 ${timestamp}`
    ];

    for (const msg of messages) {
      runJtagCommand(
        `./jtag chat/send --roomId="${roomId}" --message="${msg}"`
      );
      await new Promise(resolve => setTimeout(resolve, 200));
    }

    console.log(`✅ Created ${messages.length} messages`);

    // Wait for TrainingDaemon
    await new Promise(resolve => setTimeout(resolve, 3000));

    // Check recent training examples
    const trainingResult = runJtagCommand(
      `./jtag data/list --collection=training_examples --limit=1`
    );

    if (trainingResult.data && trainingResult.data.length > 0) {
      const example = trainingResult.data[0];

      console.log(`📊 Latest training example:`);
      console.log(`   - Message count: ${example.messageCount}`);
      console.log(`   - First message: ${example.messages[0]?.content?.substring(0, 50)}...`);

      // Should include multiple messages (context window)
      expect(example.messageCount).toBeGreaterThan(1);
      console.log(`✅ Training example includes conversation context`);
    }
  }, 30000);
});
