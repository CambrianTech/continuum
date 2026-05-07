#!/usr/bin/env tsx

import {
  formatAircBridgeChatText,
  parseAircBridgeMessage,
  roomFromAircChannel,
  summarizeBridgeResponse,
} from '../../../../../system/airc-bridge/shared/AircBridgeProtocol';

function assert(condition: boolean, message: string): void {
  if (!condition) {
    throw new Error(`Assertion failed: ${message}`);
  }
  console.log(`ok - ${message}`);
}

function testNormalChat(): void {
  const parsed = parseAircBridgeMessage('hello continuum', {
    senderNick: 'mac-codex',
    channel: '#general',
  });

  assert(parsed.action === 'chat', 'normal text maps to chat');
  assert(parsed.room === 'general', 'channel maps to room');
  assert(parsed.senderNick === 'mac-codex', 'sender preserved');
  assert(formatAircBridgeChatText(parsed) === '[airc:mac-codex] hello continuum', 'chat attribution rendered');
}

function testDirectives(): void {
  const exp = parseAircBridgeMessage('!continuum export cambriantech --last 25', { channel: '#general' });
  const assertion = parseAircBridgeMessage('!continuum assert seen marker-123 --room general --last 80');

  assert(parseAircBridgeMessage('!continuum ping').action === 'ping', 'ping directive parsed');
  assert(exp.action === 'export', 'export directive parsed');
  assert(exp.room === 'cambriantech', 'export room parsed');
  assert(exp.limit === 25, 'export limit parsed');
  assert(assertion.action === 'assert-seen', 'assert seen directive parsed');
  assert(assertion.marker === 'marker-123', 'assert marker parsed');
  assert(assertion.room === 'general', 'assert room flag parsed');
  assert(assertion.limit === 80, 'assert limit parsed');
}

function testQuotedChat(): void {
  const parsed = parseAircBridgeMessage('!continuum chat general "quoted body with spaces"', {
    senderNick: 'win-claude',
  });

  assert(parsed.action === 'chat', 'directive chat parsed');
  assert(parsed.room === 'general', 'directive chat room parsed');
  assert(parsed.message === 'quoted body with spaces', 'quoted message parsed');
}

function testSafetyHelpers(): void {
  assert(roomFromAircChannel('#cambriantech') === 'cambriantech', 'room strips #');
  assert(roomFromAircChannel('') === 'general', 'empty channel defaults');
  assert(summarizeBridgeResponse('x'.repeat(2000), 100).length <= 100, 'response summary bounds output');
}

testNormalChat();
testDirectives();
testQuotedChat();
testSafetyHelpers();
console.log('AircBridge protocol checks passed');
