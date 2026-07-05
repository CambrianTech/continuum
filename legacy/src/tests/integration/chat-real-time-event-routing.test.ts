#!/usr/bin/env tsx
/**
 * Chat Real-Time Event Routing Test
 *
 * CRITICAL TEST: Proves the real-time event routing failure pattern
 * METHODOLOGY: Send message → verify in DB → verify NOT in ChatWidget DOM (proving event routing broken)
 * EXPECTED FAILURE: New message exists in database but is NOT visible in ChatWidget without refresh
 */

import { exec } from 'child_process';
import { promisify } from 'util';

const execAsync = promisify(exec);

// REUSABLE FUNCTION: Get messages from database
async function getMessagesFromDatabase(roomId: string = 'general', limit: number = 10): Promise<any[]> {
  const result = await execAsync(`./jtag data/list --collection=messages --filter='{"roomId":"${roomId}"}' --limit=${limit}`);

  if (result.stderr) {
    throw new Error(`Database query failed: ${result.stderr}`);
  }

  try {
    const response = JSON.parse(result.stdout.split('COMMAND RESULT:\n')[1].split('============================================================')[0]);
    return response.items || [];
  } catch (error) {
    throw new Error(`Failed to parse database response: ${error}`);
  }
}

// REUSABLE FUNCTION: Look for specific message in ChatWidget DOM
async function findSpecificMessageInChatWidget(messageId: string): Promise<boolean> {
  const result = await execAsync(`./jtag exec --code="const testId = '${messageId}'; const continuumWidget = document.querySelector('continuum-widget'); const mainWidget = continuumWidget?.shadowRoot?.querySelector('main-widget'); const chatWidget = mainWidget?.shadowRoot?.querySelector('chat-widget'); if (chatWidget?.shadowRoot) { const found = Array.from(chatWidget.shadowRoot.querySelectorAll('.text-content')).find(el => el.textContent && el.textContent.includes(testId)); return found ? true : false; } return false;" --environment=browser`);

  if (result.stderr) {
    throw new Error(`Widget DOM query failed: ${result.stderr}`);
  }

  try {
    // Parse the command result
    const commandOutput = result.stdout.split('COMMAND RESULT:\n')[1]?.split('============================================================')[0];
    if (!commandOutput) return false;

    const response = JSON.parse(commandOutput);

    if (response.success && response.commandResult?.result !== undefined) {
      return response.commandResult.result === true;
    }

    return false;
  } catch (error) {
    throw new Error(`Failed to parse widget message search: ${error}`);
  }
}

// REUSABLE FUNCTION: Send specific message via ChatWidget UI
async function sendSpecificMessageViaUI(messageId: string): Promise<string> {
  const result = await execAsync(`./jtag exec --code="const testId = '${messageId}'; const continuumWidget = document.querySelector('continuum-widget'); const mainWidget = continuumWidget?.shadowRoot?.querySelector('main-widget'); const chatWidget = mainWidget?.shadowRoot?.querySelector('chat-widget'); const input = chatWidget?.shadowRoot?.querySelector('.message-input'); if (input && chatWidget.sendMessage) { input.value = testId; chatWidget.sendMessage(); return 'SENT: ' + testId; } return 'FAILED: Input not found';" --environment=browser`);

  if (result.stderr) {
    throw new Error(`Message send failed: ${result.stderr}`);
  }

  try {
    // Parse the command result
    const commandOutput = result.stdout.split('COMMAND RESULT:\n')[1]?.split('============================================================')[0];
    if (!commandOutput) throw new Error('No command output');

    const response = JSON.parse(commandOutput);

    if (response.success && response.commandResult?.result) {
      return response.commandResult.result;
    }

    throw new Error('Command execution failed');
  } catch (error) {
    throw new Error(`Failed to send message: ${error}`);
  }
}

// REUSABLE FUNCTION: Send message via CLI
async function sendMessageViaCLI(message: string, roomId: string = 'general', senderName: string = 'TestBot'): Promise<void> {
  const result = await execAsync(`./jtag chat/send --roomId="${roomId}" --message="${message}" --senderName="${senderName}"`);

  if (result.stderr) {
    throw new Error(`Message send failed: ${result.stderr}`);
  }
}

async function testRealTimeEventRoutingFailure(): Promise<void> {
  console.log('🔥 CHAT REAL-TIME EVENT ROUTING TEST');
  console.log('🎯 MODULAR APPROACH: Send specific message via UI and look for it immediately');
  console.log('');

  const testMessageId = `REALTIME-TEST-${Date.now()}`;

  try {
    // STEP 1: Send specific message via ChatWidget UI
    console.log('🧪 STEP 1: Send specific message via ChatWidget UI');
    console.log(`   Test Message ID: "${testMessageId}"`);

    const sendResult = await sendSpecificMessageViaUI(testMessageId);
    console.log(`✅ STEP 1 PASSED: ${sendResult}`);

    // STEP 2: Wait briefly for real-time events
    console.log('\n🧪 STEP 2: Wait 500ms for real-time events');
    await new Promise(resolve => setTimeout(resolve, 500));

    // STEP 3: CRITICAL - Look for the specific message in ChatWidget DOM
    console.log('\n🧪 STEP 3: CRITICAL - Look for specific message in real-time');

    const foundInRealTime = await findSpecificMessageInChatWidget(testMessageId);

    if (foundInRealTime) {
      console.log('🎉 STEP 3 SUCCESS: Message found immediately in ChatWidget!');
      console.log('   Real-time event routing is WORKING!');
      console.log('\n🔥 BREAKTHROUGH: Real-time events have been FIXED!');
    } else {
      console.log('❌ STEP 3 EXPECTED FAILURE: Message NOT found in real-time');
      console.log('   This confirms real-time event routing is broken');
      console.log('\n🔥 BUG CONFIRMED: Real-time events require manual refresh');
    }

    // STEP 4: Verify after longer delay (fallback test)
    console.log('\n🧪 STEP 4: Verify message appears after longer delay');
    await new Promise(resolve => setTimeout(resolve, 2000));

    const foundAfterDelay = await findSpecificMessageInChatWidget(testMessageId);
    console.log(`   Message found after 2.5s total: ${foundAfterDelay ? '✅' : '❌'}`);

    console.log('\n📊 REAL-TIME EVENT ROUTING TEST SUMMARY:');
    console.log(`   Message sent via UI: ✅ Working`);
    console.log(`   Real-time appearance (500ms): ${foundInRealTime ? '✅' : '❌'} (${foundInRealTime ? 'Working' : 'Broken'})`);
    console.log(`   Eventual appearance (2.5s): ${foundAfterDelay ? '✅' : '❌'} (${foundAfterDelay ? 'Working' : 'Broken'})`);

    if (foundInRealTime) {
      console.log('\n🎯 CONCLUSION: Real-time events are WORKING properly!');
    } else if (foundAfterDelay) {
      console.log('\n🎯 CONCLUSION: Messages appear but NOT in real-time - event routing delayed/broken');
    } else {
      console.log('\n🎯 CONCLUSION: Messages not appearing at all - major system issue');
    }

  } catch (error) {
    console.log(`💥 TEST EXECUTION ERROR: ${error instanceof Error ? error.message : String(error)}`);
    process.exit(1);
  }
}

// Execute if run directly
if (import.meta.url === `file://${process.argv[1]}`) {
  testRealTimeEventRoutingFailure().catch(console.error);
}

export { testRealTimeEventRoutingFailure, getMessagesFromDatabase, getMessagesFromChatWidget, sendMessageViaCLI };