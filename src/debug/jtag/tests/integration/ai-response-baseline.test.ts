#!/usr/bin/env npx tsx
/**
 * AI Response Baseline Test - Phase 1, Commit 1.0
 *
 * Purpose: Document EXACT AI response behavior before refactoring PersonaUser.
 *
 * This test establishes baseline metrics that MUST be maintained after every commit:
 * - 5+ AIs evaluate messages
 * - 1+ AIs respond
 * - Rate limiting triggers after rapid messages
 *
 * If these tests fail after refactoring, the commit MUST be reverted.
 */

import { describe, it, expect, beforeAll } from '@jest/globals';
import { jtag } from '../../server-index';
import * as fs from 'fs';
import type { JTAGClient } from '../../core/JTAGClient';

describe('AI Response Baseline (Phase 1)', () => {
  let client: JTAGClient | null = null;
  let generalRoomId: string;

  beforeAll(async () => {
    // Connect to JTAG system
    client = await jtag.connect();

    // Verify system health
    const pingResult = await client.commands['ping']({});
    expect(pingResult.success).toBe(true);

    // Get general room
    const roomsResult = await client.commands['data/list']({
      collection: 'rooms',
      filter: { uniqueId: 'general' },
      limit: 1
    });

    expect(roomsResult.success).toBe(true);
    expect(roomsResult.items).toBeDefined();
    expect(roomsResult.items.length).toBeGreaterThan(0);

    generalRoomId = roomsResult.items[0].id;
  }, 30000);

  afterAll(async () => {
    if (client) {
      await client.disconnect();
    }
  });

  it('should have 5+ AIs evaluate normal messages', async () => {
    const testMessage = `Baseline test: Normal message - ${Date.now()}`;

    // Send test message
    const sendResult = await client!.commands['debug/chat-send']({
      roomId: generalRoomId,
      message: testMessage
    });

    expect(sendResult.success).toBe(true);

    // Wait for AI evaluation (ThoughtStream decision window is 10-20s)
    await new Promise(resolve => setTimeout(resolve, 25000));

    // Check AI decision log for evaluations
    const logPath = '.continuum/jtag/sessions/system/00000000-0000-0000-0000-000000000000/logs/ai-decisions.log';

    if (!fs.existsSync(logPath)) {
      throw new Error(`AI decisions log not found at: ${logPath}`);
    }

    const logContents = fs.readFileSync(logPath, 'utf-8');
    const recentLines = logContents.split('\n').slice(-100).join('\n');

    // Count evaluations for this test message
    const evaluationMatches = recentLines.match(/Worker evaluated.*for message/g) || [];

    console.log(`📊 Baseline Metric: ${evaluationMatches.length} AIs evaluated message`);

    // BASELINE: 5+ AIs should evaluate
    expect(evaluationMatches.length).toBeGreaterThanOrEqual(5);

  }, 60000);

  it('should have 1+ AIs respond to normal messages', async () => {
    const testMessage = `Baseline test: Response check - ${Date.now()}`;

    // Send test message
    const sendResult = await client!.commands['debug/chat-send']({
      roomId: generalRoomId,
      message: testMessage
    });

    expect(sendResult.success).toBe(true);

    // Wait for AI response (includes evaluation + generation + rate limiting)
    await new Promise(resolve => setTimeout(resolve, 25000));

    // Check for AI responses
    const logPath = '.continuum/jtag/sessions/system/00000000-0000-0000-0000-000000000000/logs/ai-decisions.log';
    const logContents = fs.readFileSync(logPath, 'utf-8');
    const recentLines = logContents.split('\n').slice(-100).join('\n');

    // Count responses (AIs that decided to respond)
    const responseMatches = recentLines.match(/AI-RESPONSE.*decided to respond/g) || [];

    console.log(`📊 Baseline Metric: ${responseMatches.length} AIs responded`);

    // BASELINE: 1+ AIs should respond
    expect(responseMatches.length).toBeGreaterThanOrEqual(1);

  }, 60000);

  it('should enforce rate limiting on rapid messages', async () => {
    console.log('🔄 Testing rate limiting (sending 5 rapid messages)...');

    // Send 5 messages rapidly (2 seconds apart)
    for (let i = 0; i < 5; i++) {
      await client!.commands['debug/chat-send']({
        roomId: generalRoomId,
        message: `Rapid message ${i + 1} - ${Date.now()}`
      });

      await new Promise(resolve => setTimeout(resolve, 2000));
    }

    // Wait for processing
    await new Promise(resolve => setTimeout(resolve, 15000));

    // Check for rate limit logs
    const logPath = '.continuum/jtag/sessions/system/00000000-0000-0000-0000-000000000000/logs/ai-decisions.log';
    const logContents = fs.readFileSync(logPath, 'utf-8');
    const recentLines = logContents.split('\n').slice(-200).join('\n');

    // Look for rate limiting messages (PersonaUser logs "⏸️  Rate limited in room")
    const rateLimitMatches = recentLines.match(/Rate limited in room/g) || [];

    console.log(`📊 Baseline Metric: ${rateLimitMatches.length} rate limit triggers`);

    // BASELINE: Rate limiting should trigger at least once
    expect(rateLimitMatches.length).toBeGreaterThan(0);

  }, 90000);
});
