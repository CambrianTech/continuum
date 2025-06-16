/**
 * InfoCommand - Parent class for information display commands
 * Provides common functionality for help, agents, and other info commands
 */

const BaseCommand = require('../../BaseCommand.cjs');

class InfoCommand extends BaseCommand {
  // Common helper methods for info commands
  
  static getDefinition() {
    return {
      name: 'info',
      description: 'Base information command (override in subclasses)',
      icon: 'ℹ️',
      category: 'system',
      parameters: {}
    };
  }
  
  static getVersion() {
    try {
      const fs = require('fs');
      const path = require('path');
      const packagePath = path.join(process.cwd(), 'package.json');
      const packageJson = JSON.parse(fs.readFileSync(packagePath, 'utf8'));
      return packageJson.version;
    } catch (error) {
      return 'unknown';
    }
  }
  
  static displayCommandRegistry() {
    try {
      const CommandRegistry = require('../CommandRegistry.cjs');
      const registry = new CommandRegistry();
      
      console.log('\n🎓 COMMAND REFERENCE:');
      console.log('  Available Commands (dynamically loaded):');
      
      for (const [name, definition] of registry.definitions.entries()) {
        console.log(`    ${definition.icon} ${name} - ${definition.description}`);
        if (definition.examples && definition.examples.length > 0) {
          console.log(`      Example: ${definition.examples[0]}`);
        }
      }
    } catch (error) {
      console.log('\n  Commands: Run continuum to see dynamically loaded command list');
    }
  }
  
  static displayHeader(title, subtitle = null) {
    const version = this.getVersion();
    console.log(`\n${title} v${version}${subtitle ? ' - ' + subtitle : ''}\n`);
  }
  
  static displaySection(title, content) {
    console.log(`${title}:`);
    console.log(content);
    console.log('');
  }
  
  // Standard sections that both help and agents commands use
  static getBuiltInTestingSection() {
    return `🧪 BUILT-IN SYSTEM TESTING:
  continuum.diagnostics()                     # Run complete system test with fresh logs
  continuum.diagnostics('screenshot')         # Test screenshot system only
  continuum.diagnostics('isolated')           # Run in isolated subdirectories
  # Tests create unique subdirs, fresh logs, and can't be fooled by old files`;
  }
  
  static getTrustTheProcessSection() {
    return `🚨 CRITICAL: TRUST THE PROCESS - Follow this exactly:
  cd python-client && python trust_the_process.py    # Single command does everything!

📋 BABY STEPS DEVELOPMENT CYCLE:
  1️⃣  Clear old data: Avoid confusion/cheating
  2️⃣  Make small change: Max 50 lines, one file only  
  3️⃣  Bump version: Auto-increment for tracking
  4️⃣  Test immediately: Screenshot + console + validation ← AUTOMATED
  5️⃣  Fix ANY errors: Zero tolerance for breaking system
  6️⃣  Commit when stable: Only when everything works`;
  }
  
  static getSafetyRulesSection() {
    return `🛡️ SAFETY RULES (Never Break These):
  • NEVER break the system (immediate rollback if anything fails)
  • NEVER commit broken code (test everything first)
  • ALWAYS increase stability (every commit improves system)
  • ALWAYS follow surgical precision (small, careful changes)
  • ALWAYS edit existing files (avoid creating new files)`;
  }
  
  static getSuccessCriteriaSection() {
    return `🎯 SUCCESS CRITERIA (All Must Pass):
  • All tests pass ✅
  • No console errors ✅
  • Screenshots capture correctly ✅
  • Version numbers match ✅
  • System more stable than before ✅`;
  }
}

module.exports = InfoCommand;