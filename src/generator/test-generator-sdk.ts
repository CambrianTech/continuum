/**
 * Generator SDK Test — validates the unified interface works across all types
 */

import * as path from 'path';
import { createGeneratorRegistry } from './GeneratorSDKFactory';
import type { GeneratorAuditSummary } from './GeneratorSDK';

const rootPath = path.join(__dirname, '..');
const registry = createGeneratorRegistry(rootPath);

let passed = 0;
let failed = 0;

function assert(condition: boolean, message: string): void {
  if (condition) {
    console.log(`  PASS: ${message}`);
    passed++;
  } else {
    console.log(`  FAIL: ${message}`);
    failed++;
  }
}

// ── Registry Tests ──────────────────────────────────────────────────

console.log('\n1. Registry');
assert(registry.typeNames.length === 4, 'Has 4 registered types');
assert(registry.has('command'), 'Has command type');
assert(registry.has('entity'), 'Has entity type');
assert(registry.has('daemon'), 'Has daemon type');
assert(registry.has('widget'), 'Has widget type');
assert(!registry.has('nonexistent'), 'Does not have nonexistent type');

try {
  registry.get('nonexistent');
  assert(false, 'Should throw on unknown type');
} catch (e) {
  assert(true, 'Throws on unknown type');
}

// ── Audit Tests ─────────────────────────────────────────────────────

console.log('\n2. Audit — Command');
const cmdAudit = registry.get('command').audit();
assert(cmdAudit.type === 'command', 'Audit type is command');
assert(cmdAudit.total > 200, `Found ${cmdAudit.total} commands (expected >200)`);
assert(cmdAudit.withSpecs > 30, `${cmdAudit.withSpecs} with specs (expected >30)`);
assert(cmdAudit.checkSummary['has-spec'] !== undefined, 'Has has-spec check');
assert(cmdAudit.checkSummary['static-accessor'] !== undefined, 'Has static-accessor check');

console.log('\n3. Audit — Entity');
const entityAudit = registry.get('entity').audit();
assert(entityAudit.type === 'entity', 'Audit type is entity');
assert(entityAudit.total > 30, `Found ${entityAudit.total} entities (expected >30)`);
assert(entityAudit.checkSummary['extends-base-entity'] !== undefined, 'Has extends-base-entity check');
assert(entityAudit.checkSummary['has-validate'] !== undefined, 'Has has-validate check');

console.log('\n4. Audit — Daemon');
const daemonAudit = registry.get('daemon').audit();
assert(daemonAudit.type === 'daemon', 'Audit type is daemon');
assert(daemonAudit.total > 10, `Found ${daemonAudit.total} daemons (expected >10)`);
assert(daemonAudit.checkSummary['has-shared'] !== undefined, 'Has has-shared check');

console.log('\n5. Audit — Widget');
const widgetAudit = registry.get('widget').audit();
assert(widgetAudit.type === 'widget', 'Audit type is widget');
assert(widgetAudit.total > 15, `Found ${widgetAudit.total} widgets (expected >15)`);
assert(widgetAudit.checkSummary['has-widget-class'] !== undefined, 'Has has-widget-class check');

// ── AuditAll Tests ──────────────────────────────────────────────────

console.log('\n6. AuditAll');
const allAudits = registry.auditAll();
assert(allAudits.size === 4, 'AuditAll returns 4 summaries');
let totalModules = 0;
for (const summary of allAudits.values()) {
  totalModules += summary.total;
}
assert(totalModules > 350, `Total modules: ${totalModules} (expected >350)`);

// ── Help Tests ──────────────────────────────────────────────────────

console.log('\n7. Help');
const cmdHelp = registry.get('command').help();
assert(cmdHelp.full.length > 500, `Command help is ${cmdHelp.full.length} chars`);
assert(cmdHelp.availableTopics.includes('spec'), 'Has spec topic');

const entityHelp = registry.get('entity').help();
assert(entityHelp.full.length > 100, `Entity help is ${entityHelp.full.length} chars`);

// ── Template Tests ──────────────────────────────────────────────────

console.log('\n8. Templates');
const cmdTemplate = registry.get('command').templateSpec('minimal');
assert((cmdTemplate as Record<string, unknown>).name !== undefined, 'Command template has name');

const entityTemplate = registry.get('entity').templateSpec('standard');
assert((entityTemplate as Record<string, unknown>).name !== undefined, 'Entity template has name');
assert((entityTemplate as Record<string, unknown>).fields !== undefined, 'Entity template has fields');

const cmdVariants = registry.get('command').templateVariants();
assert(cmdVariants.includes('minimal'), 'Command has minimal variant');
assert(cmdVariants.includes('rust-ipc'), 'Command has rust-ipc variant');

// ── Reverse Engineer Tests ──────────────────────────────────────────

console.log('\n9. Reverse Engineer');
const pingSpec = registry.get('command').reverseEngineer('commands/ping');
assert(pingSpec !== null, 'Can reverse-engineer ping command');
if (pingSpec) {
  assert((pingSpec as Record<string, unknown>).name === 'ping', 'Reversed spec name is ping');
}

// ── AuditOne Tests ──────────────────────────────────────────────────

console.log('\n10. AuditOne');
const pingAudit = registry.get('command').auditOne('ping');
assert(pingAudit.name === 'ping', 'AuditOne finds ping');
assert(typeof pingAudit.checks['has-spec'] === 'boolean', 'Has has-spec check result');

// ── Summary ─────────────────────────────────────────────────────────

console.log(`\n${'='.repeat(40)}`);
console.log(`  ${passed} passed, ${failed} failed`);
console.log(`${'='.repeat(40)}\n`);

process.exit(failed > 0 ? 1 : 0);
