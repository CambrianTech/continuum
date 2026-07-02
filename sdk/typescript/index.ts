/**
 * @continuum/sdk-typescript — the TypeScript SDK barrel.
 *
 * THIS is how the SDKs work: a thin typed skin over the headless Rust core's facade
 * (Commands + Events + Handle, the two primitives × two directions), generated
 * types, zero logic. The native SDKs (swift/kotlin/flutter) present the same shape.
 *
 * Usage:
 *   import { Continuum } from '@continuum/sdk-typescript';
 *   const continuum = Continuum.connect(transport);        // transport = the facade binding
 *   const users = await continuum.commands.execute('data/list', { collection: 'users' });
 *   continuum.events.subscribe('contract:proposed', { onEvent: (p) => … });
 *   continuum.commands.provide('interface/screenshot', async (p) => webCapture(p));
 *   const conn = await continuum.open('live/connect', { room }, { env: 'web' });
 */

export { Continuum } from './Continuum';
export { Commands } from './Commands';
export { Events } from './Events';
export type { EventHandlers, EventMeta, SubscribeOptions } from './Events';
export { Handle, handleFrom } from './Handle';
export type { HandleEventHandlers } from './Handle';
export type {
  Transport,
  Target,
  SessionIdentity,
  Subscription,
  Registration,
  RawCommandHandler,
  RawEventHandlers,
} from './transport';
export { buildCommandUri, buildEventTopic, stampContext } from './transport';
export { WebSocketTransport } from './WebSocketTransport';
export type { WebSocketLike, WebSocketCtor } from './WebSocketTransport';
export type { CommandMap, CommandName } from './generated/CommandMap';
export type { EventMap, EventClass } from './generated/EventMap';
/** Typed, string-free accessors generated from the Rust command/event specs. */
export { CommandApi } from './generated/CommandApi';
export { EventApi } from './generated/EventApi';
