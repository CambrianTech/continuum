/**
 * JTAG Symmetric Daemon System
 * 
 * Export all components of the symmetric daemon architecture following
 * the middle-out patterns for unified client/server development.
 */

// Core interfaces and base classes
export { 
  BaseDaemon 
} from './shared/MessageSubscriber';
export type { 
  MessageSubscriber, 
  DaemonMessage, 
  DaemonResponse
} from './shared/MessageSubscriber';

// Router with automatic /client and /server prefixing
export { JTAGRouter } from './shared/JTAGRouter';
export type { RouterContext } from './shared/JTAGRouter';

// Import classes for factory function
import { JTAGRouter as JTAGRouterClass } from './shared/JTAGRouter';
import { CommandProcessorDaemon } from './CommandProcessorDaemon';
import { ConsoleDaemon } from './ConsoleDaemon';
import { DaemonMessage } from './shared/MessageSubscriber';

// Symmetric daemon implementations
export { 
  CommandProcessorDaemon,
  createServerCommandProcessor,
  createClientCommandProcessor
} from './CommandProcessorDaemon';
export type {
  CommandMessage,
  CommandResult
} from './CommandProcessorDaemon';

export {
  ConsoleDaemon,
  createServerConsoleDaemon,
  createClientConsoleDaemon
} from './ConsoleDaemon';
export type {
  ConsoleMessage,
  ConsoleFilter
} from './ConsoleDaemon';

// Convenience factory for complete JTAG daemon system
export function createJTAGDaemonSystem(context: 'client' | 'server' | 'universal' = 'universal') {
  const router = new JTAGRouterClass({ environment: context });
  
  // Create daemons based on context
  const daemons: (CommandProcessorDaemon | ConsoleDaemon)[] = [];
  
  if (context === 'server' || context === 'universal') {
    daemons.push(new CommandProcessorDaemon('server'));
    daemons.push(new ConsoleDaemon('server'));
  }
  
  if (context === 'client' || context === 'universal') {
    daemons.push(new CommandProcessorDaemon('client'));
    daemons.push(new ConsoleDaemon('client'));
  }
  
  // Register all daemons with router
  const registerDaemons = async () => {
    for (const daemon of daemons) {
      await daemon.registerWithRouter(router);
      console.log(`✅ Registered daemon with JTAG router`);
    }
  };
  
  return {
    router,
    daemons,
    registerDaemons,
    
    // Convenience methods
    async sendMessage(message: DaemonMessage) {
      return await router.routeMessage(message);
    },
    
    getRegisteredEndpoints() {
      return router.getRegisteredEndpoints();
    },
    
    getSystemInfo() {
      return {
        context,
        daemonCount: daemons.length,
        registeredEndpoints: router.getRegisteredEndpoints(),
        routerType: 'JTAGRouter (Symmetric)',
        architecture: 'middle-out'
      };
    }
  };
}

// Re-export from shared for convenience
export { JTAGRouter as Router } from './shared/JTAGRouter';