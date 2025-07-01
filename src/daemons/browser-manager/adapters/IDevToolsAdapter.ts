/**
 * DevTools Adapter Interface - Universal DevTools control abstraction
 * 
 * Provides consistent interface across different browser DevTools protocols:
 * - Chrome DevTools Protocol (CDP) for Chrome/Edge/Opera
 * - Firefox Remote Protocol for Firefox
 * - Safari Remote Inspector for Safari
 * 
 * Each adapter handles the browser-specific protocol details while exposing
 * a common interface for automation and debugging tasks.
 * 
 * All interfaces now imported from shared types to eliminate duplication.
 */

// All DevTools interfaces now imported from shared types
export * from '../types/devtools.js';