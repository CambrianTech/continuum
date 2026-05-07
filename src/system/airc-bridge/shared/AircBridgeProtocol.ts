/**
 * AIRC <-> Continuum bridge protocol.
 *
 * AIRC carries normal chat text or explicit development directives. This
 * parser stays transport-agnostic so it can be tested without a live mesh.
 */

export type AircBridgeAction =
  | 'chat'
  | 'ping'
  | 'status'
  | 'rooms'
  | 'export'
  | 'assert-seen'
  | 'activity-list'
  | 'unknown';

export interface ParsedAircBridgeMessage {
  action: AircBridgeAction;
  originalText: string;
  senderNick: string;
  channel: string;
  room: string;
  isDirective: boolean;
  message?: string;
  marker?: string;
  limit?: number;
  error?: string;
}

export interface ParseAircBridgeOptions {
  senderNick?: string;
  channel?: string;
  room?: string;
  commandPrefix?: string;
  defaultRoom?: string;
}

interface ParseContext {
  originalText: string;
  senderNick: string;
  channel: string;
  room: string;
}

const DEFAULT_PREFIX = '!continuum';
const DEFAULT_ROOM = 'general';
const DEFAULT_SENDER = 'airc-peer';
const DEFAULT_LIMIT = 50;

export function roomFromAircChannel(channel?: string, fallback = DEFAULT_ROOM): string {
  const normalized = (channel ?? '').trim().replace(/^#/, '');
  return normalized || fallback;
}

export function parseAircBridgeMessage(
  text: string,
  options: ParseAircBridgeOptions = {},
): ParsedAircBridgeMessage {
  const prefix = options.commandPrefix ?? DEFAULT_PREFIX;
  const context = createParseContext(text, options);
  const trimmed = text.trim();

  if (!trimmed.startsWith(prefix)) {
    return createParsed(context, 'chat', { isDirective: false, message: text });
  }

  return parseDirective(context, tokenize(trimmed.slice(prefix.length).trim()), prefix);
}

export function formatAircBridgeChatText(parsed: ParsedAircBridgeMessage): string {
  const body = parsed.message ?? parsed.originalText;
  return `[airc:${parsed.senderNick}] ${body}`;
}

export function summarizeBridgeResponse(text: string, maxChars = 1600): string {
  const normalized = text.replace(/\r\n/g, '\n').trim();
  if (normalized.length <= maxChars) return normalized;
  return `${normalized.slice(0, maxChars - 32).trimEnd()}\n... [truncated]`;
}

function createParseContext(text: string, options: ParseAircBridgeOptions): ParseContext {
  const fallbackRoom = options.defaultRoom ?? DEFAULT_ROOM;
  const senderNick = nonEmpty(options.senderNick) ?? DEFAULT_SENDER;
  const explicitRoom = nonEmpty(options.room);
  return {
    originalText: text,
    senderNick,
    channel: roomFromAircChannel(options.channel, fallbackRoom),
    room: explicitRoom ?? roomFromAircChannel(options.channel, fallbackRoom),
  };
}

function nonEmpty(value: string | undefined): string | undefined {
  const trimmed = value?.trim();
  return trimmed && trimmed.length > 0 ? trimmed : undefined;
}

function parseDirective(context: ParseContext, tokens: string[], prefix: string): ParsedAircBridgeMessage {
  const verb = (tokens.shift() ?? '').toLowerCase();
  if (!verb) {
    return createParsed(context, 'unknown', { error: `Missing directive after ${prefix}` });
  }

  const handlers: Record<string, (ctx: ParseContext, rest: string[]) => ParsedAircBridgeMessage> = {
    ping: ctx => createParsed(ctx, 'ping'),
    status: ctx => createParsed(ctx, 'status'),
    rooms: parseRooms,
    activity: parseActivity,
    export: parseExport,
    assert: parseAssert,
    chat: parseChat,
  };

  return handlers[verb]?.(context, tokens) ?? createParsed(context, 'unknown', {
    error: `Unknown directive: ${verb}`,
  });
}

function parseRooms(context: ParseContext, tokens: string[]): ParsedAircBridgeMessage {
  return createParsed(context, 'rooms', { limit: readIntFlag(tokens, 'limit') ?? DEFAULT_LIMIT });
}

function parseActivity(context: ParseContext, tokens: string[]): ParsedAircBridgeMessage {
  const subcommand = (tokens.shift() ?? '').toLowerCase();
  if (subcommand !== 'list') {
    return createParsed(context, 'unknown', { error: 'Expected: !continuum activity list' });
  }
  return createParsed(context, 'activity-list', { limit: readIntFlag(tokens, 'limit') ?? DEFAULT_LIMIT });
}

function parseExport(context: ParseContext, tokens: string[]): ParsedAircBridgeMessage {
  return createParsed(context, 'export', {
    room: readRoomArg(tokens) ?? context.room,
    limit: readIntFlag(tokens, 'last') ?? readIntFlag(tokens, 'limit') ?? DEFAULT_LIMIT,
  });
}

function parseAssert(context: ParseContext, tokens: string[]): ParsedAircBridgeMessage {
  const assertion = (tokens.shift() ?? '').toLowerCase();
  const marker = tokens.shift();
  if (assertion !== 'seen' || !marker) {
    return createParsed(context, 'unknown', { error: 'Expected: !continuum assert seen <marker>' });
  }
  return createParsed(context, 'assert-seen', {
    marker,
    room: readStringFlag(tokens, 'room') ?? context.room,
    limit: readIntFlag(tokens, 'last') ?? readIntFlag(tokens, 'limit') ?? DEFAULT_LIMIT,
  });
}

function parseChat(context: ParseContext, tokens: string[]): ParsedAircBridgeMessage {
  const targetRoom = tokens.length > 1 && !tokens[0].startsWith('--') ? tokens.shift() : context.room;
  const message = tokens.join(' ').trim();
  if (!message) {
    return createParsed(context, 'unknown', { error: 'Expected: !continuum chat [room] <message>' });
  }
  return createParsed(context, 'chat', { room: targetRoom, message });
}

function createParsed(
  context: ParseContext,
  action: AircBridgeAction,
  overrides: Partial<ParsedAircBridgeMessage> = {},
): ParsedAircBridgeMessage {
  return {
    action,
    originalText: context.originalText,
    senderNick: context.senderNick,
    channel: context.channel,
    room: context.room,
    isDirective: true,
    ...overrides,
  };
}

function tokenize(input: string): string[] {
  const tokens: string[] = [];
  let current = '';
  let quote: '"' | "'" | null = null;
  let escaping = false;

  for (const char of input) {
    const handled = consumeTokenChar({ char, tokens, current, quote, escaping });
    current = handled.current;
    quote = handled.quote;
    escaping = handled.escaping;
  }

  if (current) tokens.push(current);
  return tokens;
}

function consumeTokenChar(state: {
  char: string;
  tokens: string[];
  current: string;
  quote: '"' | "'" | null;
  escaping: boolean;
}): { current: string; quote: '"' | "'" | null; escaping: boolean } {
  if (state.escaping) return { current: state.current + state.char, quote: state.quote, escaping: false };
  if (state.char === '\\') return { current: state.current, quote: state.quote, escaping: true };

  if (state.quote) {
    return state.char === state.quote
      ? { current: state.current, quote: null, escaping: false }
      : { current: state.current + state.char, quote: state.quote, escaping: false };
  }

  if (state.char === '"' || state.char === "'") {
    return { current: state.current, quote: state.char, escaping: false };
  }

  if (/\s/.test(state.char)) {
    if (state.current) state.tokens.push(state.current);
    return { current: '', quote: null, escaping: false };
  }

  return { current: state.current + state.char, quote: null, escaping: false };
}

function readRoomArg(tokens: string[]): string | undefined {
  const roomFlag = readStringFlag(tokens, 'room');
  if (roomFlag) return roomFlag;
  if (tokens.length > 0 && !tokens[0].startsWith('--')) return tokens.shift();
  return undefined;
}

function readStringFlag(tokens: string[], name: string): string | undefined {
  const prefix = `--${name}=`;
  const inline = tokens.findIndex(token => token.startsWith(prefix));
  if (inline >= 0) {
    const [token] = tokens.splice(inline, 1);
    return token.slice(prefix.length);
  }

  const split = tokens.findIndex(token => token === `--${name}`);
  if (split >= 0 && tokens[split + 1]) {
    tokens.splice(split, 1);
    const [value] = tokens.splice(split, 1);
    return value;
  }

  return undefined;
}

function readIntFlag(tokens: string[], name: string): number | undefined {
  const raw = readStringFlag(tokens, name);
  if (!raw) return undefined;
  const parsed = Number.parseInt(raw, 10);
  return Number.isFinite(parsed) && parsed > 0 ? parsed : undefined;
}
