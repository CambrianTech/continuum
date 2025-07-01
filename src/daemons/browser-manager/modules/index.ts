/**
 * Browser Manager Modules - Focused modular components
 * Clean separation of concerns for browser management functionality
 */

// Core interfaces
export type { IBrowserModule } from './IBrowserModule';

// Browser modules
export { ChromeBrowserModule } from './ChromeBrowserModule';

// Focused management modules  
export { BrowserLauncher, BrowserPaths } from './BrowserLauncher';
export { BrowserRegistry } from './BrowserRegistry';
export { ResourceManager, ProcessStats, ResourceMetrics, OptimizationResult } from './ResourceManager';
export { 
  SessionManager, 
  PlacementStrategy, 
  SessionState 
} from './SessionManager';
export { 
  PortManager, 
  PortAllocation, 
  PortConflictResolution 
} from './PortManager';