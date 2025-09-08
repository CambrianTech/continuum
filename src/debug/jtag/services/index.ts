/**
 * Services - Business Logic Layer
 * 
 * Clean service layer that bridges API types with existing router/transport system.
 * Services provide high-level business operations without widgets needing to know
 * about daemon connections or transport details.
 * 
 * Architecture:
 * - Uses existing excellent router/transport system
 * - Implements clean API types from api/types/ and api/commands/  
 * - Provides dependency injection for widgets
 * - NO hardcoded daemon connections
 * - NO magic constants or operations
 */

// Base service infrastructure
export * from './shared/ServiceBase';

// Business logic services
export * from './chat/ChatService';
export * from './user/UserService';

// Service registry for dependency injection
export { ServiceRegistry } from './shared/ServiceBase';
export type { IServiceRegistry } from './shared/ServiceBase';