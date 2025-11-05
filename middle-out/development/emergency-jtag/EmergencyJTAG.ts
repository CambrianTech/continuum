/**
 * Emergency JTAG - Universal Debugging System
 * 
 * Main export following middle-out module conventions.
 * Provides identical API across browser and server contexts.
 * 
 * Usage:
 *   import { JTAG } from './middle-out/development/emergency-jtag/EmergencyJTAG';
 *   
 *   // Works identically in browser and server
 *   JTAG.log('Component', 'message');
 *   JTAG.critical('Component', 'critical_event', data);
 *   JTAG.trace('Component', 'functionName', 'ENTER');
 *   JTAG.probe('Component', 'state_name', stateData);
 */

import { EmergencyJTAGBase } from './shared/EmergencyJTAGBase';

/**
 * Universal Emergency JTAG - Auto-detects context and provides unified API
 */
export const JTAG = EmergencyJTAGBase;

// Legacy export for compatibility
export { EmergencyJTAGBase as UniversalEmergencyJTAG };

// Type exports
export * from './shared/EmergencyJTAGTypes';