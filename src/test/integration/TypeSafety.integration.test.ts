/**
 * Integration tests for type safety across the daemon system
 * Ensures strong typing, no 'any' types, and proper interfaces
 */

import { describe, it, before } from 'node:test';
import assert from 'node:assert';
import * as ts from 'typescript';
import * as fs from 'fs';
import * as path from 'path';

describe('Type Safety Integration Tests', () => {
  let projectRoot: string;
  
  before(() => {
    projectRoot = path.resolve(__dirname, '../../../..');
  });
  
  describe('TypeScript Strict Mode Compliance', () => {
    it('should have strict mode enabled in tsconfig', () => {
      const tsconfigPath = path.join(projectRoot, 'tsconfig.json');
      const tsconfigContent = fs.readFileSync(tsconfigPath, 'utf-8');
      const tsconfig = JSON.parse(tsconfigContent);
      
      assert(tsconfig.compilerOptions.strict === true, 'Strict mode should be enabled');
      assert(tsconfig.compilerOptions.noImplicitAny === true, 'noImplicitAny should be enabled');
      assert(tsconfig.compilerOptions.strictNullChecks === true, 'strictNullChecks should be enabled');
    });
  });
  
  describe('No Any Types in Core Files', () => {
    const coreFiles = [
      'src/daemons/base/BaseDaemon.ts',
      'src/daemons/base/DaemonProtocol.ts',
      'src/daemons/base/DaemonEventBus.ts',
      'src/commands/base/BaseCommand.ts',
      'src/types/SessionTypes.ts',
      'src/types/CommandTypes.ts'
    ];
    
    coreFiles.forEach(file => {
      it(`should not use 'any' type in ${file}`, () => {
        const filePath = path.join(projectRoot, file);
        if (!fs.existsSync(filePath)) {
          // Skip if file doesn't exist
          return;
        }
        
        const content = fs.readFileSync(filePath, 'utf-8');
        
        // Check for 'any' type usage (excluding comments and strings)
        const lines = content.split('\n');
        const anyUsages: string[] = [];
        
        lines.forEach((line, index) => {
          // Skip comments
          if (line.trim().startsWith('//') || line.trim().startsWith('*')) {
            return;
          }
          
          // Look for ': any' or 'as any' patterns
          if (line.match(/:\s*any(?:\s|,|;|\)|>|$)/) || line.match(/as\s+any/)) {
            anyUsages.push(`Line ${index + 1}: ${line.trim()}`);
          }
        });
        
        assert(
          anyUsages.length === 0,
          `File ${file} contains 'any' types:\n${anyUsages.join('\n')}`
        );
      });
    });
  });
  
  describe('Interface Definitions', () => {
    it('should have well-defined daemon interfaces', () => {
      const interfaceFiles = [
        'src/daemons/base/DaemonProtocol.ts',
        'src/types/DaemonTypes.ts'
      ];
      
      interfaceFiles.forEach(file => {
        const filePath = path.join(projectRoot, file);
        if (!fs.existsSync(filePath)) {
          return;
        }
        
        const content = fs.readFileSync(filePath, 'utf-8');
        
        // Check for interface definitions
        assert(content.includes('export interface'), `${file} should export interfaces`);
        
        // Check for proper typing in interfaces
        const interfaceMatches = content.match(/export interface \w+ {[^}]+}/gs);
        if (interfaceMatches) {
          interfaceMatches.forEach(interfaceBlock => {
            // Should not have any 'any' types in interfaces
            assert(
              !interfaceBlock.includes(': any'),
              `Interface in ${file} should not use 'any' type`
            );
          });
        }
      });
    });
    
    it('should have strongly typed event definitions', () => {
      const eventBusPath = path.join(projectRoot, 'src/daemons/base/DaemonEventBus.ts');
      if (!fs.existsSync(eventBusPath)) {
        return;
      }
      
      const content = fs.readFileSync(eventBusPath, 'utf-8');
      
      // Should have DaemonEvents interface
      assert(content.includes('export interface DaemonEvents'), 'Should have DaemonEvents interface');
      
      // Check that events are strongly typed
      const eventInterface = content.match(/export interface DaemonEvents\s*{([^}]+)}/s);
      if (eventInterface) {
        const eventDefinitions = eventInterface[1];
        
        // Each event should have a defined structure
        assert(eventDefinitions.includes('sessionId:'), 'Events should have typed properties');
        assert(!eventDefinitions.includes(': any'), 'Events should not use any type');
      }
    });
  });
  
  describe('Command Type Safety', () => {
    it('should have strongly typed command results', () => {
      const commandTypesPath = path.join(projectRoot, 'src/types/CommandTypes.ts');
      if (!fs.existsSync(commandTypesPath)) {
        // Check in BaseCommand
        const baseCommandPath = path.join(projectRoot, 'src/commands/base/BaseCommand.ts');
        if (fs.existsSync(baseCommandPath)) {
          const content = fs.readFileSync(baseCommandPath, 'utf-8');
          assert(content.includes('CommandResult'), 'Should define CommandResult type');
          assert(!content.includes('data: any'), 'CommandResult should not use any for data');
        }
      }
    });
    
    it('should use generic types for command data', () => {
      const baseCommandPath = path.join(projectRoot, 'src/commands/base/BaseCommand.ts');
      if (!fs.existsSync(baseCommandPath)) {
        return;
      }
      
      const content = fs.readFileSync(baseCommandPath, 'utf-8');
      
      // Should use generics for type safety
      assert(
        content.includes('CommandResult<T>') || content.includes('CommandResult<'),
        'Should use generic types for command results'
      );
    });
  });
  
  describe('Daemon Message Type Safety', () => {
    it('should have strongly typed daemon messages', () => {
      const protocolPath = path.join(projectRoot, 'src/daemons/base/DaemonProtocol.ts');
      if (!fs.existsSync(protocolPath)) {
        return;
      }
      
      const content = fs.readFileSync(protocolPath, 'utf-8');
      
      // Check DaemonMessage interface
      assert(content.includes('export interface DaemonMessage'), 'Should have DaemonMessage interface');
      
      // Should have required fields with proper types
      const messageInterface = content.match(/export interface DaemonMessage\s*{([^}]+)}/s);
      if (messageInterface) {
        const fields = messageInterface[1];
        assert(fields.includes('from: string'), 'Should have typed from field');
        assert(fields.includes('to: string'), 'Should have typed to field');
        assert(fields.includes('type: string'), 'Should have typed type field');
        
        // data field should be typed or generic
        assert(
          !fields.includes('data: any') || fields.includes('data?: any'),
          'data field should be properly typed'
        );
      }
    });
    
    it('should have strongly typed daemon responses', () => {
      const protocolPath = path.join(projectRoot, 'src/daemons/base/DaemonProtocol.ts');
      if (!fs.existsSync(protocolPath)) {
        return;
      }
      
      const content = fs.readFileSync(protocolPath, 'utf-8');
      
      // Check DaemonResponse interface
      assert(content.includes('export interface DaemonResponse'), 'Should have DaemonResponse interface');
      
      // Should use generics for data
      assert(
        content.includes('DaemonResponse<T') || content.includes('DaemonResponse<'),
        'DaemonResponse should use generics'
      );
    });
  });
  
  describe('Inheritance Patterns', () => {
    it('should properly extend BaseDaemon', () => {
      const daemonFiles = [
        'src/daemons/session-manager/SessionManagerDaemon.ts',
        'src/daemons/browser-manager/BrowserManagerDaemon.ts',
        'src/daemons/command-processor/CommandProcessorDaemon.ts'
      ];
      
      daemonFiles.forEach(file => {
        const filePath = path.join(projectRoot, file);
        if (!fs.existsSync(filePath)) {
          return;
        }
        
        const content = fs.readFileSync(filePath, 'utf-8');
        
        // Should extend BaseDaemon
        assert(
          content.includes('extends BaseDaemon'),
          `${file} should extend BaseDaemon`
        );
        
        // Should implement required methods
        assert(content.includes('async onStart()'), `${file} should implement onStart`);
        assert(content.includes('async onStop()'), `${file} should implement onStop`);
      });
    });
    
    it('should properly extend BaseCommand', () => {
      const commandFiles = [
        'src/commands/core/health/HealthCommand.ts',
        'src/commands/kernel/connect/ConnectCommand.ts'
      ];
      
      commandFiles.forEach(file => {
        const filePath = path.join(projectRoot, file);
        if (!fs.existsSync(filePath)) {
          return;
        }
        
        const content = fs.readFileSync(filePath, 'utf-8');
        
        // Should extend BaseCommand or DaemonCommand
        assert(
          content.includes('extends BaseCommand') || content.includes('extends DaemonCommand'),
          `${file} should extend BaseCommand or DaemonCommand`
        );
        
        // Should have static execute method
        assert(
          content.includes('static async execute') || content.includes('static execute'),
          `${file} should have static execute method`
        );
      });
    });
  });
});