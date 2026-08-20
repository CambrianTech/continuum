#!/usr/bin/env tsx
/**
 * Generate Archive Daemon
 *
 * Generates the ArchiveDaemon using DaemonGenerator
 */

import { DaemonGenerator } from './DaemonGenerator';
import { archiveDaemonSpec } from './specs/archive-daemon-spec';
import * as path from 'path';

const generator = new DaemonGenerator(__dirname);
const outputDir = path.join(__dirname, '..', 'daemons', 'archive-daemon');

console.log('🏗️  Generating Archive Daemon...\n');

generator.generate(archiveDaemonSpec, outputDir, { force: false });

console.log('\n✅ Archive Daemon generated!');
console.log('\n📝 Next steps:');
console.log('   1. Implement archive logic in daemons/archive-daemon/server/ArchiveDaemonServer.ts');
console.log('   2. Add @Archive() decorator to system/data/decorators/FieldDecorators.ts');
console.log('   3. Register daemon in system orchestration');
console.log('   4. Test with ./jtag debug/archive-stats\n');
