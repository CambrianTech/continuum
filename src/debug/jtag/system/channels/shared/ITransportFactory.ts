/**
 * Transport Factory Interface - Typed contract for environment-specific transport creation
 * 
 * Forces compile-time implementation of transport creation methods.
 * Each environment implements this interface with their specific transports.
 */

import type { JTAGContext } from '../../core/types/JTAGTypes';
import type { JTAGTransport, TransportConfig } from './TransportTypes';

export interface ITransportFactory {
  createTransport(
    environment: JTAGContext['environment'], 
    config: TransportConfig
  ): Promise<JTAGTransport>;
  
  createWebSocketTransport(
    environment: JTAGContext['environment'],
    config: TransportConfig
  ): Promise<JTAGTransport>;
}