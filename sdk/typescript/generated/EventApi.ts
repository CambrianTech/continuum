// GENERATED from the Rust event registry (core/continuum-core sdk_codegen).
// DO NOT EDIT. Typed accessors — call api.on<Class>/emit<Class>, no string key.

import { Events } from '../Events';
import type { EventHandlers, SubscribeOptions, Subscription } from '../Events';
import type { EventMap } from './EventMap';

/**
* Typed event accessors. `onX` subscribes, `emitX` publishes — payloads +
* handlers typed off the generated EventMap, the class string baked in once.
*/
export class EventApi {
  constructor(private readonly events: Events) {}

  /** subscribe `contract:accepted` */
  onContractAccepted(handlers: EventHandlers<'contract:accepted'>, opts?: SubscribeOptions<'contract:accepted'>): Subscription {
    return this.events.subscribe('contract:accepted', handlers, opts);
  }

  /** emit `contract:accepted` */
  emitContractAccepted(payload: EventMap['contract:accepted']): Promise<void> {
    return this.events.emit('contract:accepted', payload);
  }

  /** subscribe `contract:bid` */
  onContractBid(handlers: EventHandlers<'contract:bid'>, opts?: SubscribeOptions<'contract:bid'>): Subscription {
    return this.events.subscribe('contract:bid', handlers, opts);
  }

  /** emit `contract:bid` */
  emitContractBid(payload: EventMap['contract:bid']): Promise<void> {
    return this.events.emit('contract:bid', payload);
  }

  /** subscribe `contract:delivered` */
  onContractDelivered(handlers: EventHandlers<'contract:delivered'>, opts?: SubscribeOptions<'contract:delivered'>): Subscription {
    return this.events.subscribe('contract:delivered', handlers, opts);
  }

  /** emit `contract:delivered` */
  emitContractDelivered(payload: EventMap['contract:delivered']): Promise<void> {
    return this.events.emit('contract:delivered', payload);
  }

  /** subscribe `contract:disputed` */
  onContractDisputed(handlers: EventHandlers<'contract:disputed'>, opts?: SubscribeOptions<'contract:disputed'>): Subscription {
    return this.events.subscribe('contract:disputed', handlers, opts);
  }

  /** emit `contract:disputed` */
  emitContractDisputed(payload: EventMap['contract:disputed']): Promise<void> {
    return this.events.emit('contract:disputed', payload);
  }

  /** subscribe `contract:executing` */
  onContractExecuting(handlers: EventHandlers<'contract:executing'>, opts?: SubscribeOptions<'contract:executing'>): Subscription {
    return this.events.subscribe('contract:executing', handlers, opts);
  }

  /** emit `contract:executing` */
  emitContractExecuting(payload: EventMap['contract:executing']): Promise<void> {
    return this.events.emit('contract:executing', payload);
  }

  /** subscribe `contract:paid` */
  onContractPaid(handlers: EventHandlers<'contract:paid'>, opts?: SubscribeOptions<'contract:paid'>): Subscription {
    return this.events.subscribe('contract:paid', handlers, opts);
  }

  /** emit `contract:paid` */
  emitContractPaid(payload: EventMap['contract:paid']): Promise<void> {
    return this.events.emit('contract:paid', payload);
  }

  /** subscribe `contract:proposed` */
  onContractProposed(handlers: EventHandlers<'contract:proposed'>, opts?: SubscribeOptions<'contract:proposed'>): Subscription {
    return this.events.subscribe('contract:proposed', handlers, opts);
  }

  /** emit `contract:proposed` */
  emitContractProposed(payload: EventMap['contract:proposed']): Promise<void> {
    return this.events.emit('contract:proposed', payload);
  }

  /** subscribe `contract:verified` */
  onContractVerified(handlers: EventHandlers<'contract:verified'>, opts?: SubscribeOptions<'contract:verified'>): Subscription {
    return this.events.subscribe('contract:verified', handlers, opts);
  }

  /** emit `contract:verified` */
  emitContractVerified(payload: EventMap['contract:verified']): Promise<void> {
    return this.events.emit('contract:verified', payload);
  }
}
