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

// Auto-register standard parsers
IntegrationParserRegistry.register(new CLIIntegrationParser());
IntegrationParserRegistry.register(new JSONWithArgsIntegrationParser());
IntegrationParserRegistry.register(new PureJSONIntegrationParser());
IntegrationParserRegistry.register(new StringJSONIntegrationParser());

// Export everything for extensibility
export * from './IntegrationParser';
export * from './CLIIntegrationParser';
export * from './JSONIntegrationParser';

// Future parsers can be added here:
// import { PersonaIntegrationParser } from './PersonaIntegrationParser';
// IntegrationParserRegistry.register(new PersonaIntegrationParser());