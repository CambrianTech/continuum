/**
 * Browser Manager Modules - Focused modular components
 * Clean separation of concerns for browser management functionality
 */

// Core interfaces
export type { IBrowserModule } from './IBrowserModule';

// Browser modules
export { ChromeBrowserModule } from './ChromeBrowserModule';

// Focused management modules  
export { BrowserLauncher } from './BrowserLauncher';
export { BrowserRegistry } from './BrowserRegistry';
export { ResourceManager } from './ResourceManager';
export type { ProcessStats, ResourceMetrics, OptimizationResult } from './ResourceManager';
export { SessionManager } from './SessionManager';
export type { PlacementStrategy, SessionState } from './SessionManager';
export { PortManager } from './PortManager';
export type { PortAllocation, PortConflictResolution } from './PortManager';