/**
 * JTAGRouterDynamic - Experimental Dynamic Transport Router
 * 
 * This is the experimental version that will evolve to support:
 * - Dynamic transport registration (not hardcoded enum)
 * - P2P mesh networking
 * - Plugin-based transport strategies
 * 
 * EXTENDS: Same JTAGRouterBase as the working JTAGRouter
 * STARTS WITH: Skeleton that will be filled by copying working logic
 * EVOLVES TO: Dynamic transport management
 */

import { JTAGRouterBase } from './JTAGRouterBase';

/**
 * JTAGRouterDynamic - Experimental version with dynamic transport management
 * 
 * CURRENT STATE: Skeleton - not functional yet
 * NEXT STEP: Copy working JTAGRouter implementation method by method
 * EVOLUTION PLAN: Replace hardcoded TRANSPORT_TYPES with dynamic registration
 */
export abstract class JTAGRouterDynamic extends JTAGRouterBase {
  
  constructor() {
    // TODO: Copy constructor from working JTAGRouter
    throw new Error('JTAGRouterDynamic: Not implemented yet - this is just a skeleton');
  }

  // TODO: Copy all methods from working JTAGRouter.ts
  // Then gradually evolve to dynamic transport management
}