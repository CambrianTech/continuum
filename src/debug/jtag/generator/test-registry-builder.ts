/**
 * Test RegistryBuilder - Verify registry generation
 */

import { RegistryBuilder } from './core/RegistryBuilder';
import type { RegistryConfig } from './core/RegistryBuilder';
import type { EntryInfo } from './core/EntryExtractor';
import * as path from 'path';
import { readFileSync } from 'fs';

const rootPath = path.join(__dirname, '..');

// Sample entries
const commandEntries: EntryInfo[] = [
  { name: 'ping', className: 'PingCommand', importPath: './commands/ping/server/PingServer' },
  { name: 'screenshot', className: 'ScreenshotCommand', importPath: './commands/screenshot/browser/ScreenshotBrowser' }
];

const widgetEntries: EntryInfo[] = [
  { name: 'chat', className: 'ChatWidget', importPath: './widgets/chat/browser/ChatWidget' },
  { name: 'main', className: 'MainWidget', importPath: './widgets/main/browser/MainWidget' }
];

// Sample config for browser registry
const browserConfig: RegistryConfig = {
  environment: 'browser',
  outputFile: '/tmp/test-browser-registry.ts',
  typeImports: {
    'CommandEntry': 'system/commands/types/CommandTypes',
    'WidgetEntry': 'system/widgets/types/WidgetTypes'
  },
  entryTypes: [
    {
      name: 'command',
      pluralName: 'commands',
      typeScriptTypeName: 'CommandEntry',
      arrayName: '{ENV}_COMMANDS',
      entryTemplate: '  { name: \'{name}\', handler: {className} }'
    },
    {
      name: 'widget',
      pluralName: 'widgets',
      typeScriptTypeName: 'WidgetEntry',
      arrayName: '{ENV}_WIDGETS',
      entryTemplate: '  { name: \'{name}\', component: {className} }'
    }
  ]
};

const entries = new Map<string, EntryInfo[]>();
entries.set('command', commandEntries);
entries.set('widget', widgetEntries);

console.log('🧪 Testing RegistryBuilder...\n');

const builder = new RegistryBuilder(rootPath);
builder.generate(browserConfig, entries);

console.log('\n📄 Generated registry file:');
console.log('='.repeat(80));
const content = readFileSync('/tmp/test-browser-registry.ts', 'utf-8');
console.log(content);
console.log('='.repeat(80));

console.log('\n✅ RegistryBuilder test complete!');
