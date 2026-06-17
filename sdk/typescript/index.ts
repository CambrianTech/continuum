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
 *   continuum.events.subscribe('data:users:created', { onEvent: (u) => … });
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
  Subscription,
  Registration,
  RawCommandHandler,
  RawEventHandlers,
} from './transport';
export { buildCommandUri, buildEventTopic } from './transport';
export type { CommandMap, CommandName, EventMap, EventClass } from './generated/CommandMap';
