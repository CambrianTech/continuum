#!/usr/bin/env npx tsx
/**
 * Chat Roundtrip Test - Precommit Validation (#1186)
 *
 * Sends a probe message into #general and asserts that at least one
 * persona produces a reply within a short window. The point is to
 * make precommit fail when the persona reply path is broken at
 * commit time rather than after canary lands and a human notices the
 * personas have gone silent.
 *
 * This is the "raise the bar past server-didn't-crash" test that
 * Joel called out 2026-05-14: "browser ping is pretty low bar".
 *
 * Pass criteria:
 *   - At least one online persona user exists in the seeded set
 *   - Probe message is accepted by collaboration/chat/send
 *   - Within REPLY_WINDOW_MS, a new message appears in the room
 *     authored by an online persona
 *
 * Fail modes (each one is the kind of regression this test catches):
 *   - No personas seeded (BUG-105 family)
 *   - chat/send rejects the probe (room missing, attribution broken)
 *   - chat/export missing the probe (write path broken)
 *   - probe written but no persona reply within window (cognition
 *     pipeline silently broken — the highest-value catch)
 */

import { jtag } from '../../server-index';

// Bound the test latency while still allowing the loaded local-inference
// path to prove itself. Backpressure on developer machines has produced
// valid persona replies after the old 55s window; the hook gives this
// single smoke test a larger cap so the test can fail with diagnostics
// instead of being killed by the runner.
const REPLY_WINDOW_MS = 105_000;
const POLL_INTERVAL_MS = 2_000;
const PROBE_ROOM = 'general';

interface ChatMessageRow {
  readonly id?: string;
  readonly senderId?: string;
  readonly senderName?: string;
  readonly senderType?: string;
  readonly roomId?: string;
  readonly content?: { readonly text?: string };
  readonly timestamp?: number | string;
}

interface CommandResult {
  readonly success?: boolean;
  readonly items?: readonly unknown[];
  readonly shortId?: string;
  readonly messageId?: string;
}

interface JtagClient {
  readonly commands: Record<string, (params: Record<string, unknown>) => Promise<CommandResult>>;
  readonly disconnect?: () => Promise<void>;
}

interface ChatUser {
  readonly id?: string;
  readonly displayName?: string;
  readonly type?: string;
  readonly status?: string;
  readonly provider?: string | null;
  readonly capabilities?: unknown;
}

interface ProbeRecord {
  readonly text: string;
  readonly sentAtMs: number;
  readonly responderCount: number;
  readonly responderIds: ReadonlySet<string>;
  readonly responderNames: readonly string[];
}

function probeText(): string {
  // Unique tag for finding our own message in the chat log + an
  // explicit ask. Locally-running personas filter messages they don't
  // think need a reply (sensible default; saves Metal cycles), so a
  // bare "precommit-probe-XYZ" string sometimes goes unanswered. A
  // direct question with the unique tag inside it consistently triggers
  // a reply because it reads as addressed to the room.
  const tag = `precommit-probe-${Date.now()}-${Math.floor(Math.random() * 1e6)}`;
  return `${tag} — precommit gate is verifying chat works end to end. Any persona, please reply OK so I know the cognition pipeline is live.`;
}

async function sleep(ms: number): Promise<void> {
  return new Promise(resolve => setTimeout(resolve, ms));
}

async function listReplyCapablePersonas(client: JtagClient): Promise<readonly ChatUser[]> {
  const usersResult = await client.commands['data/list']({
    collection: 'users'
  });
  if (!usersResult?.success) {
    throw new Error('data/list users failed: ' + JSON.stringify(usersResult));
  }
  const users = (usersResult.items ?? []) as readonly ChatUser[];
  const responders = users.filter(isReplyCapablePersona);
  if (responders.length === 0) {
    throw new Error(
      `No online persona responders found in seeded data. ` +
      `Found ${users.length} users total. ` +
      `Persona seed/status step likely broke. ` +
      `Persona summary: ${summarizePersonaUsers(users)}`
    );
  }
  console.log(
    `✅ Found ${responders.length} reply-capable persona(s) — ` +
    `${users.length} users total`
  );
  console.log(`   ${responders.map(formatResponder).join(', ')}\n`);
  return responders;
}

async function sendProbe(client: JtagClient, responders: readonly ChatUser[]): Promise<ProbeRecord> {
  const text = probeText();
  const sentAtMs = Date.now();
  console.log(`📤 Sending probe: "${text}"`);
  const sendResult = await client.commands['collaboration/chat/send']({
    room: PROBE_ROOM,
    message: text
  });
  if (!sendResult?.success) {
    throw new Error(
      `collaboration/chat/send rejected the probe: ` +
      JSON.stringify(sendResult)
    );
  }
  const probeMessageId = sendResult.shortId ?? sendResult.messageId ?? null;
  console.log(`✅ Probe accepted (id=${probeMessageId})\n`);
  return {
    text,
    sentAtMs,
    responderCount: responders.length,
    responderIds: new Set(responders.map(r => r.id).filter((id): id is string => typeof id === 'string')),
    responderNames: responders.map(r => r.displayName ?? r.id ?? 'unknown')
  };
}

function findProbe(messages: readonly ChatMessageRow[], probe: ProbeRecord): ChatMessageRow | undefined {
  return messages.find(m => m.content?.text === probe.text);
}

function findReply(
  messages: readonly ChatMessageRow[],
  probe: ProbeRecord,
  probeSenderId: string,
  probeRoomId: string,
  probeTimestampMs: number
): ChatMessageRow | undefined {
  return messages.find(m =>
    m.roomId === probeRoomId &&
    m.senderId !== undefined &&
    m.senderId !== probeSenderId &&
    probe.responderIds.has(m.senderId) &&
    toMs(m.timestamp) >= probeTimestampMs &&
    (m.content?.text?.length ?? 0) > 0 &&
    m.content?.text !== probe.text
  );
}

function logReply(reply: ChatMessageRow): void {
  const preview = (reply.content?.text ?? '').slice(0, 80).replace(/\s+/g, ' ');
  console.log(`✅ Persona reply received from ${reply.senderName ?? reply.senderId}: "${preview}…"`);
  console.log('🎉 CHAT ROUNDTRIP TEST: PASSED');
  console.log('=================================\n');
}

async function pollForReply(client: JtagClient, probe: ProbeRecord): Promise<void> {
  console.log(`👂 Polling chat_messages for a persona reply (window=${REPLY_WINDOW_MS / 1000}s)...`);
  const deadline = probe.sentAtMs + REPLY_WINDOW_MS;
  let probeSenderId: string | undefined;
  let probeRoomId: string | undefined;
  let probeTimestampMs = 0;
  let lastSeenCount = 0;
  let lastMessages: readonly ChatMessageRow[] = [];

  while (Date.now() < deadline) {
    await sleep(POLL_INTERVAL_MS);
    const listResult = await client.commands['data/list']({
      collection: 'chat_messages',
      orderBy: [{ field: 'timestamp', direction: 'desc' }],
      limit: 50
    });
    if (!listResult?.success) continue;
    const messages = (listResult.items ?? []) as readonly ChatMessageRow[];
    lastMessages = messages;
    if (messages.length !== lastSeenCount) {
      console.log(`   …${messages.length} chat_messages rows visible`);
      lastSeenCount = messages.length;
    }

    const probeMsg = findProbe(messages, probe);
    if (probeMsg && !probeSenderId) {
      probeSenderId = probeMsg.senderId;
      probeRoomId = probeMsg.roomId;
      probeTimestampMs = toMs(probeMsg.timestamp);
    }
    if (!probeSenderId || !probeRoomId) continue;

    const reply = findReply(messages, probe, probeSenderId, probeRoomId, probeTimestampMs);
    if (reply) {
      logReply(reply);
      return;
    }
  }

  throw new Error(
    `No persona reply received within ${REPLY_WINDOW_MS / 1000}s window. ` +
    `Probe was sent and ${probeSenderId ? 'observed' : 'NOT observed'} in chat_messages. ` +
    `${probe.responderCount} online persona responder(s): ${probe.responderNames.join(', ')}. ` +
    `Recent messages after probe: ${summarizeRecentMessages(lastMessages, probe.sentAtMs)}. ` +
    `Cognition / response pipeline is silently broken or too backpressured to meet the smoke-test budget.`
  );
}

async function testChatRoundtrip(): Promise<void> {
  console.log('💬 CHAT ROUNDTRIP TEST (#1186)');
  console.log('=================================');

  let client: JtagClient | undefined;

  try {
    console.log('🔗 Connecting to JTAG system...');
    client = await jtag.connect() as JtagClient;
    console.log('✅ Connected\n');

    // 1. There must be at least one online persona, otherwise no one
    //    can reply to the probe and the test would just be vacuously
    //    failing instead of catching a pipeline regression. Old seeded
    //    `autoResponds=true` users can be offline; the runtime responder
    //    contract is an online persona in chat.
    console.log('🤖 Verifying at least one online persona responder is seeded...');
    const responders = await listReplyCapablePersonas(client);

    // 2. Send the probe. Capture the timestamp so we can scope the
    //    reply check to messages written AFTER our send (avoids false
    //    positives from any pre-existing reply in the room).
    const probe = await sendProbe(client, responders);

    // 3. Poll chat_messages for a reply. We're looking for any
    //    message with a timestamp >= probe and a senderId that
    //    belongs to one of the online personas. We use data/list directly
    //    rather than collaboration/chat/export because export returns
    //    a single rendered markdown blob; structured rows give us
    //    cleaner field access (senderId, senderType, roomId UUID).
    await pollForReply(client, probe);
    process.exitCode = 0;
  } catch (error) {
    console.error('\n❌ Chat roundtrip test failed:', error);
    console.error('❌ Error details:', {
      message: error instanceof Error ? error.message : String(error),
      stack: error instanceof Error ? error.stack : undefined
    });
    console.log('=================================\n');
    process.exitCode = 1;
  } finally {
    if (client?.disconnect) {
      await client.disconnect();
    }
  }

  process.exit(process.exitCode ?? 0);
}

function toMs(ts: number | string | undefined): number {
  if (typeof ts === 'number') return ts;
  if (typeof ts === 'string') {
    const parsed = Date.parse(ts);
    return Number.isFinite(parsed) ? parsed : 0;
  }
  return 0;
}

function isReplyCapablePersona(user: ChatUser): boolean {
  if (typeof user.id !== 'string') return false;
  if (user.status === 'offline') return false;
  return user.type === 'persona' || capabilityFlag(user.capabilities, 'autoResponds') === true;
}

function capabilityFlag(capabilities: unknown, key: string): boolean | undefined {
  const parsed = parseCapabilities(capabilities);
  const value = parsed?.[key];
  return typeof value === 'boolean' ? value : undefined;
}

function parseCapabilities(capabilities: unknown): Record<string, unknown> | undefined {
  if (capabilities && typeof capabilities === 'object' && !Array.isArray(capabilities)) {
    return capabilities as Record<string, unknown>;
  }
  if (typeof capabilities !== 'string') return undefined;
  try {
    const parsed: unknown = JSON.parse(capabilities);
    return parsed && typeof parsed === 'object' && !Array.isArray(parsed)
      ? parsed as Record<string, unknown>
      : undefined;
  } catch {
    return undefined;
  }
}

function formatResponder(user: ChatUser): string {
  const name = user.displayName ?? user.id ?? 'unknown';
  const provider = user.provider ? `/${user.provider}` : '';
  return `${name}(${user.status ?? 'unknown'}${provider})`;
}

function summarizePersonaUsers(users: readonly ChatUser[]): string {
  const personas = users.filter(user => user.type === 'persona' || capabilityFlag(user.capabilities, 'autoResponds') === true);
  if (personas.length === 0) return 'none';
  return personas.map(formatResponder).slice(0, 12).join(', ');
}

function summarizeRecentMessages(messages: readonly ChatMessageRow[], sentAtMs: number): string {
  const recent = messages
    .filter(message => toMs(message.timestamp) >= sentAtMs)
    .slice(0, 8)
    .map(message => {
      const sender = message.senderName ?? message.senderId ?? 'unknown';
      const type = message.senderType ?? 'unknown';
      const ageSeconds = Math.round((toMs(message.timestamp) - sentAtMs) / 1000);
      const preview = (message.content?.text ?? '').slice(0, 40).replace(/\s+/g, ' ');
      return `${sender}/${type}@+${ageSeconds}s "${preview}"`;
    });
  return recent.length > 0 ? recent.join('; ') : 'none';
}

void testChatRoundtrip();
