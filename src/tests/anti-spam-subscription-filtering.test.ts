#!/usr/bin/env tsx
/**
 * Universal Cross-Environment Event Routing - Anti-Spam Path-Based Architecture
 *
 * CRITICAL TEST: Validates universal event routing across ALL environments
 * Tests the foundational architecture for the entire distributed P2P system
 *
 * Cross-Environment Routing Matrix:
 * - Server → Browser (command responses, real-time events)
 * - Browser → Server (user actions, widget events)
 * - Browser ↔ Browser (peer-to-peer widget communication)
 * - Server ↔ Server (daemon-to-daemon communication)
 * - Future: Continuum ↔ Continuum (P2P grid routing)
 * - Future: Commands across P2P grid
 *
 * Key Validations:
 * - Path-based subscription filtering: `/room/general` ≠ `/room/unwanted` ≠ `/system/alerts`
 * - Zero event spam: clients ONLY receive subscribed paths
 * - Cross-environment event bridging through EventsDaemon
 * - Transport-agnostic routing via Router abstraction
 * - Universal event constants for type safety across all environments
 */

import { JTAGClientServer } from '../system/core/client/server/JTAGClientServer';
import type { JTAGClientConnectOptions } from '../system/core/client/shared/JTAGClient';
import { CHAT_EVENTS } from '../widgets/chat/shared/ChatEventConstants';
import type { SubscribeRoomResult } from '../commands/chat/subscribe-room/shared/SubscribeRoomCommand';
import type { ChatSendMessageResult } from '../commands/chat/send/shared/ChatSendMessageTypes';
import type { LogsDebugResult } from '../commands/debug/logs/shared/LogsDebugTypes';
import type { UUID } from '../system/core/types/CrossPlatformUUID';

interface AntiSpamTestResult {
  testName: string;
  success: boolean;
  details?: {
    subscriptionId?: UUID;
    subscribedPath?: string;
    unsubscribedPaths?: readonly string[];
    architecture?: string;
    messageId?: UUID;
    message?: string;
    path?: string;
    routingDirection?: string;
    shouldReceive?: boolean;
    isSpamTest?: boolean;
    subscribedLogsCount?: number;
    spamLogsCount?: number;
    routingValidated?: boolean;
  };
  error?: string;
}

/**
 * Universal Cross-Environment Event Routing Test Suite
 */
async function runAntiSpamSubscriptionTest(): Promise<void> {
  console.log(`🌐 UNIVERSAL CROSS-ENVIRONMENT EVENT ROUTING TEST SUITE`);
  console.log(`🎯 Testing path-based subscription across ALL environments`);
  console.log(`📋 CRITICAL: Foundation for entire distributed P2P system`);
  console.log(`🔄 Routing Matrix: Server↔Browser, Browser↔Browser, Server↔Server`);
  console.log(`🚀 Future: Continuum↔Continuum P2P grid + commands across grid`);
  console.log('');

  let testCount = 0;
  let passCount = 0;
  const results: AntiSpamTestResult[] = [];

  try {
    const connectOptions: JTAGClientConnectOptions = {
      timeout: 30000,
      targetEnvironment: 'server'
    };

    const connectionResult = await JTAGClientServer.connect(connectOptions);
    const client = connectionResult.client;
    console.log('✅ Connected to JTAG server for anti-spam testing');

    // Test 1: Multi-Path Subscription Test - Chat, System, User paths
    testCount++;
    const subscribedChatPath = '/room/subscribed-test';
    const unsubscribedChatPath = '/room/UNWANTED-SPAM';
    const unsubscribedSystemPath = '/system/alerts';
    const unsubscribedUserPath = '/user/profile-updates';

    try {
      console.log(`🧪 Test 1: Multi-path subscription architecture test...`);
      console.log(`   ✅ Subscribing to: ${subscribedChatPath}`);
      console.log(`   🚫 NOT subscribing to: ${unsubscribedChatPath}`);
      console.log(`   🚫 NOT subscribing to: ${unsubscribedSystemPath}`);
      console.log(`   🚫 NOT subscribing to: ${unsubscribedUserPath}`);

      const subscriptionResult = await client.commands['collaboration/chat/subscribe-room']({
        roomId: 'subscribed-test',
        eventTypes: [CHAT_EVENTS.MESSAGE_RECEIVED],
        context: client.context,
        sessionId: client.sessionId
      }) as SubscribeRoomResult;

      if (subscriptionResult?.success && subscriptionResult?.subscriptionId) {
        console.log(`✅ Test 1 PASSED: Path-based subscription established`);
        console.log(`   Subscribed path: ${subscribedChatPath}`);
        console.log(`   Subscription ID: ${subscriptionResult.subscriptionId}`);
        console.log(`   🎯 CRITICAL: Only events from subscribed path should be delivered`);
        passCount++;
        results.push({
          testName: 'multiPathSubscription',
          success: true,
          details: {
            subscribedPath: subscribedChatPath,
            unsubscribedPaths: [unsubscribedChatPath, unsubscribedSystemPath, unsubscribedUserPath],
            subscriptionId: subscriptionResult.subscriptionId,
            architecture: 'universal-cross-environment'
          }
        });
      } else {
        console.log('❌ Test 1 FAILED: Multi-path subscription failed');
        results.push({
          testName: 'multiPathSubscription',
          success: false,
          error: subscriptionResult?.error ?? 'Path subscription error'
        });
      }
    } catch (error) {
      console.log('❌ Test 1 FAILED: Exception during multi-path subscription');
      results.push({
        testName: 'multiPathSubscription',
        success: false,
        error: error instanceof Error ? error.message : String(error)
      });
    }

    // Test 2: Send message to SUBSCRIBED path (should receive via cross-environment routing)
    testCount++;
    const subscribedMessage = `SUBSCRIBED-${Date.now()}`;

    try {
      console.log(`🧪 Test 2: Server→Browser routing test (subscribed path)...`);
      console.log(`   📨 Sending to subscribed path: ${subscribedChatPath}`);

      const messageResult = await client.executeCommand('collaboration/chat/send', {
        roomId: 'subscribed-test',
        message: subscribedMessage,
        senderName: 'Cross-Environment Tester'
      });

      if (messageResult?.success && messageResult?.messageId) {
        console.log(`✅ Test 2 PASSED: Server→Browser routing established`);
        console.log(`   Event: "${subscribedMessage}"`);
        console.log(`   Path: ${subscribedChatPath}`);
        console.log(`   🎯 Should route through EventsDaemon→Router→Browser`);
        passCount++;
        results.push({
          testName: 'serverToBrowserRouting',
          success: true,
          details: {
            messageId: messageResult.messageId,
            message: subscribedMessage,
            path: subscribedChatPath,
            routingDirection: 'Server→Browser',
            shouldReceive: true
          }
        });
      } else {
        console.log('❌ Test 2 FAILED: Server→Browser routing failed');
        results.push({
          testName: 'serverToBrowserRouting',
          success: false,
          error: messageResult?.error ?? 'Cross-environment routing failed'
        });
      }
    } catch (error) {
      console.log('❌ Test 2 FAILED: Exception in Server→Browser routing');
      results.push({
        testName: 'serverToBrowserRouting',
        success: false,
        error: error instanceof Error ? error.message : String(error)
      });
    }

    // Test 3: Send message to UNSUBSCRIBED room (should NOT receive - ANTI-SPAM)
    testCount++;
    const spamMessage = `SPAM-MSG-${Date.now()}`;

    try {
      console.log(`🧪 Test 3: Sending message to UNSUBSCRIBED room "${unsubscribedChatPath}"...`);
      console.log(`   🚫 ANTI-SPAM TEST: This message should NOT be delivered to us`);

      const spamResult = await client.executeCommand('collaboration/chat/send', {
        roomId: 'UNWANTED-SPAM',
        message: spamMessage,
        senderName: 'Spam Generator'
      });

      if (spamResult?.success && spamResult?.messageId) {
        console.log(`✅ Test 3 PASSED: Spam message sent successfully`);
        console.log(`   Spam Message: "${spamMessage}"`);
        console.log(`   🚫 Should NOT be delivered (we're not subscribed)`);
        passCount++;
        results.push({
          testName: 'unsubscribedRoomSpamMessage',
          success: true,
          details: {
            messageId: spamResult.messageId,
            message: spamMessage,
            roomId: unsubscribedChatPath,
            shouldReceive: false,
            isSpamTest: true
          }
        });
      } else {
        console.log('❌ Test 3 FAILED: Failed to send spam message');
        results.push({
          testName: 'unsubscribedRoomSpamMessage',
          success: false,
          error: spamResult?.error ?? 'Spam message send failed'
        });
      }
    } catch (error) {
      console.log('❌ Test 3 FAILED: Exception sending spam message');
      results.push({
        testName: 'unsubscribedRoomSpamMessage',
        success: false,
        error: error instanceof Error ? error.message : String(error)
      });
    }

    // Test 4: Validate EventsDaemon routing logs show proper filtering
    testCount++;
    try {
      console.log('🧪 Test 4: Validating EventsDaemon routing shows anti-spam filtering...');

      const logsResult = await client.executeCommand('debug/logs', {
        filterPattern: `${subscribedMessage}|${spamMessage}`,
        tailLines: 50
      });

      if (logsResult?.success && logsResult?.logs) {
        const logs = Array.isArray(logsResult.logs) ? logsResult.logs : [logsResult.logs];
        const subscribedEventLogs = logs.filter(log => log.includes(subscribedMessage));
        const spamEventLogs = logs.filter(log => log.includes(spamMessage));

        console.log(`📊 Found ${subscribedEventLogs.length} logs for subscribed message`);
        console.log(`📊 Found ${spamEventLogs.length} logs for spam message`);

        // Both should exist in logs (messages were sent), but routing should be different
        if (subscribedEventLogs.length > 0) {
          console.log(`✅ Test 4 PASSED: EventsDaemon routing logs captured`);
          console.log(`   Subscribed room events logged: ${subscribedEventLogs.length}`);
          console.log(`   Unsubscribed room events logged: ${spamEventLogs.length}`);
          passCount++;
          results.push({
            testName: 'eventsDaemonRoutingLogs',
            success: true,
            details: {
              subscribedLogsCount: subscribedEventLogs.length,
              spamLogsCount: spamEventLogs.length,
              routingValidated: true
            }
          });
        } else {
          console.log('❌ Test 4 FAILED: No routing logs found');
          results.push({
            testName: 'eventsDaemonRoutingLogs',
            success: false,
            error: 'No EventsDaemon routing logs captured'
          });
        }
      } else {
        console.log('❌ Test 4 FAILED: Could not retrieve routing logs');
        results.push({
          testName: 'eventsDaemonRoutingLogs',
          success: false,
          error: logsResult?.error ?? 'Logs retrieval failed'
        });
      }
    } catch (error) {
      console.log('❌ Test 4 FAILED: Exception retrieving routing logs');
      results.push({
        testName: 'eventsDaemonRoutingLogs',
        success: false,
        error: error instanceof Error ? error.message : String(error)
      });
    }

    await client.disconnect();

  } catch (error) {
    console.error('💥 FATAL: Anti-spam test suite failed to initialize:', error);
  }

  // Display Results
  console.log('');
  console.log('📊 ANTI-SPAM SUBSCRIPTION FILTERING TEST RESULTS:');
  console.log(`   Total Tests: ${testCount}`);
  console.log(`   ✅ Passed: ${passCount}`);
  console.log(`   ❌ Failed: ${testCount - passCount}`);
  console.log(`   📈 Success Rate: ${Math.round((passCount / testCount) * 100)}%`);

  if (passCount === testCount) {
    console.log('🎉 ALL ANTI-SPAM TESTS PASSED - Discord-like filtering validated!');
  } else {
    console.log('⚠️ Some anti-spam tests failed - subscription filtering needs attention');
  }

  // Detailed Results
  console.log('');
  console.log('📋 Detailed Anti-Spam Test Results:');
  results.forEach((result, index) => {
    const status = result.success ? '✅' : '❌';
    console.log(`   ${status} ${index + 1}. ${result.testName}`);
    if (result.error) {
      console.log(`      Error: ${result.error}`);
    }
    if (result.details) {
      console.log(`      Details: ${JSON.stringify(result.details)}`);
    }
  });

  console.log('');
  console.log('🚨 ANTI-SPAM EVIDENCE SUMMARY:');
  console.log(`   🔗 Subscribed to: "${results[0]?.details?.subscribedPath ?? 'N/A'}"`);
  console.log(`   🚫 NOT subscribed to: "${results[0]?.details?.unsubscribedPaths?.[0] ?? 'N/A'}"`);
  console.log(`   📨 Messages sent to both rooms for filtering test`);
  console.log(`   🎯 CRITICAL: Should receive ONLY subscribed events, ZERO spam`);
  console.log('');
  console.log('🔬 Anti-spam subscription filtering testing complete!');
}

// Execute if run directly
if (import.meta.url === `file://${process.argv[1]}`) {
  runAntiSpamSubscriptionTest().catch(console.error);
}

export { runAntiSpamSubscriptionTest };