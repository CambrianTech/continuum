/**
 * Command Processor Shared Types - Symmetric Daemon Architecture
 * 
 * Universal types and protocols for command processing across client and server contexts.
 * Enables consistent development patterns and symmetric daemon architecture.
 */

// ✅ COMMAND TYPES - Core command execution interfaces
export * from './CommandTypes';

// ✅ CARE VALIDATION - Phase Omega pattern of care validation
export * from './CareValidation';

// ✅ COMMAND PROTOCOL - Daemon message contracts and communication
export * from './CommandProtocol';

// ✅ RE-EXPORT CONVENIENCE TYPES FROM CORRECT MODULES
export type {
  // Core command interfaces  
  TypedCommandRequest,
  CommandRouting,
  CommandImplementation,
  CommandExecution,
  CommandCost,
  CommandExecutionStatus,
  CommandProvider,
  CommandQuality
} from './CommandTypes';

export type {
  // Care validation
  CareValidation,
  CareMetrics,
  CareLevel
} from './CareValidation';

export type {
  // Protocol messages
  CommandProtocolMessage,
  CommandProtocolResponse,
  CommandExecuteMessage,
  CommandRouteMessage,
  CommandStatusMessage,
  ExecuteCommandMessage,
  HandleApiMessage,
  HandleWidgetApiMessage
} from './CommandProtocol';

// ✅ RE-EXPORT FACTORIES AND UTILITIES
export {
  CommandExecutionFactory
} from './CommandTypes';

export {
  CareValidationFactory,
  CareValidationBuilder
} from './CareValidation';

export {
  CommandProtocolFactory,
  COMMAND_MESSAGE_TYPES
} from './CommandProtocol';

// ✅ RE-EXPORT TYPE GUARDS
export {
  isTypedCommandRequest,
  isCommandExecution,
  isCommandImplementation
} from './CommandTypes';

export {
  isCareValidation,
  isCareMetrics
} from './CareValidation';

export {
  isCommandExecuteMessage,
  isCommandRouteMessage,
  isCommandStatusMessage,
  isExecuteCommandMessage,
  isHandleApiMessage,
  isHandleWidgetApiMessage
} from './CommandProtocol';