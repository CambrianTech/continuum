#!/usr/bin/env tsx
/**
 * Generate Logger Daemon
 *
 * Generates the LoggerDaemon using DaemonGenerator
 * Establishes the Rust-backed daemon pattern for future daemons
 */

import { DaemonGenerator } from './DaemonGenerator';
import { loggerDaemonSpec } from './specs/logger-daemon-spec';
import * as path from 'path';

const generator = new DaemonGenerator(__dirname);
const outputDir = path.join(__dirname, '..', 'daemons', 'logger-daemon');

console.log('🦀 Generating Logger Daemon (Rust-backed pattern)...\n');

generator.generate(loggerDaemonSpec, outputDir, { force: true });

console.log('\n✅ Logger Daemon generated!');
console.log('\n📝 Next steps:');
console.log('   1. Implement Rust worker connection in daemons/logger-daemon/server/LoggerDaemonServer.ts');
console.log('   2. Connect to /tmp/jtag-logger-worker.sock');
console.log('   3. Add health check and reconnection logic');
console.log('   4. Test with ./jtag logger/health-check\n');
console.log('\n🦀 This establishes the pattern for future Rust-backed daemons (Training, Inference, etc.)\n');
