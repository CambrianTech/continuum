/**
 * Integration Parsers Index
 * Auto-registers all standard parsers for BaseCommand
 */

import { IntegrationParserRegistry } from './IntegrationParser';
import { CLIIntegrationParser } from './CLIIntegrationParser';
import { 
  JSONWithArgsIntegrationParser, 
  PureJSONIntegrationParser, 
  StringJSONIntegrationParser 
} from './JSONIntegrationParser';
import { PersonaMeshParser } from './PersonaMeshParser';
import { MCPIntegrationParser } from './MCPIntegrationParser';

// Auto-register all parsers (order matters - higher priority first)
IntegrationParserRegistry.register(new CLIIntegrationParser());         // Priority 100 - CLI tools
IntegrationParserRegistry.register(new PersonaMeshParser());            // Priority 90 - AI collaboration
IntegrationParserRegistry.register(new MCPIntegrationParser());         // Priority 85 - MCP ecosystem
IntegrationParserRegistry.register(new JSONWithArgsIntegrationParser()); // Priority 80 - Hybrid JSON
IntegrationParserRegistry.register(new StringJSONIntegrationParser());  // Priority 50 - String JSON
IntegrationParserRegistry.register(new PureJSONIntegrationParser());    // Priority 0 - Fallback

// Export everything for extensibility
export * from './IntegrationParser';
export * from './CLIIntegrationParser';
export * from './JSONIntegrationParser';
export * from './PersonaMeshParser';
export * from './MCPIntegrationParser';