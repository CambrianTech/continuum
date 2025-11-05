/**
 * Channel System - Public API Exports
 * 
 * Clean module interface for the channel system. Provides access to all
 * channel factories, types, utilities, and endpoint management through a single import point.
 * 
 * MIGRATION NOTE: This is the new structure for what was previously called "transports".
 * Channels represent the communication infrastructure, while adapters are protocol implementations.
 */

// Core channel system  
export { TransportBase as ChannelBase, TRANSPORT_EVENTS as CHANNEL_EVENTS, TRANSPORT_TYPES as CHANNEL_TYPES, TRANSPORT_ROLES as CHANNEL_ROLES } from './shared';

// Channel factory interface - implemented by environment-specific factories
export type { ITransportFactory as IChannelFactory } from './shared/ITransportFactory';
export type { 
  JTAGTransport as JTAGChannel, 
  TransportConfig as ChannelConfig,
  TransportSendResult as ChannelSendResult, 
  TransportRole as ChannelRole,
  TransportProtocol as ChannelProtocol 
} from './shared';

// Channel handler interface - payload-based architecture
export type { 
  ITransportHandler as IChannelHandler
} from './shared/ITransportHandler';
export { TransportHandlerBase as ChannelHandlerBase } from './shared/ITransportHandler';

// Channel endpoint management
export { TransportEndpointBase as ChannelEndpointBase } from './shared/TransportEndpoint';
export type { TransportEndpoint as ChannelEndpoint, TransportEndpointStatus as ChannelEndpointStatus, TransportEndpointConfig as ChannelEndpointConfig } from './shared/TransportEndpoint';

// Re-export transport types for backward compatibility during migration
export type { 
  ITransportFactory, 
  JTAGTransport, 
  TransportConfig,
  TransportSendResult, 
  TransportRole,
  TransportProtocol,
  ITransportHandler
} from './shared';
export { TransportBase, TRANSPORT_EVENTS, TRANSPORT_TYPES, TRANSPORT_ROLES, TransportHandlerBase } from './shared';
export { TransportEndpointBase } from './shared/TransportEndpoint';
export type { TransportEndpoint, TransportEndpointStatus, TransportEndpointConfig } from './shared/TransportEndpoint';