/**
 * Transport Handler Interface - Payload-based transport protocol compliance
 * 
 * FOLLOWS EXISTING PAYLOAD ARCHITECTURE:
 * - MessageSubscriber: handleMessage(JTAGMessage) → Promise<JTAGResponsePayload>
 * - EventsInterface: emit(eventName, data), on(eventName, listener)  
 * - TransportHandler: handleTransportMessage(JTAGMessage) → Promise<JTAGResponsePayload>
 * 
 * SAME PATTERNS EVERYWHERE:
 * - Payload in, payload out
 * - Promises for async coordination  
 * - TypeScript interface contracts (no runtime validation needed)
 * - JTAGMessage and JTAGEvent are very similar payload-based architectures
 * 
 * ENFORCEMENT:
 * - TypeScript enforces interface compliance at compile time
 * - Types ARE the validation - no runtime checks needed
 * - Same simple pattern as all other JTAG interfaces
 */

import type { UUID } from '@shared/CrossPlatformUUID';
import type { JTAGMessage } from '@shared/JTAGTypes';
import type { JTAGResponsePayload } from '../../../shared/ResponseTypes';

/**
 * Transport Handler - handles transport protocol messages using payload architecture
 * Same pattern as MessageSubscriber but for transport-specific messages
 */
export interface ITransportHandler {
  /**
   * Handle transport protocol messages
   * Same signature as MessageSubscriber.handleMessage - payload in, payload out
   */
  handleTransportMessage(message: JTAGMessage): Promise<JTAGResponsePayload>;
  
  /**
   * Transport identifier for routing
   */
  readonly transportId: UUID;
}

/**
 * Base implementation of ITransportHandler 
 * Transport handlers can extend this or implement ITransportHandler directly
 */
export abstract class TransportHandlerBase implements ITransportHandler {
  public readonly transportId: UUID;
  
  constructor(transportId: UUID) {
    this.transportId = transportId;
  }
  
  /**
   * Abstract transport message handler - subclasses MUST implement
   * Same pattern as daemon handleMessage methods
   */
  abstract handleTransportMessage(message: JTAGMessage): Promise<JTAGResponsePayload>;
}