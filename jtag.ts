#!/usr/bin/env npx tsx

/**
 * JTAG - Clean AI Debugging Tool
 * ==============================
 * CLI wrapper for the modular JtagCLI implementation
 */

import { JtagCLI } from './src/commands/development/jtag-cli/JtagCLI';

const jtag = new JtagCLI();

const commands: Record<string, () => void | Promise<void>> = {
  help: () => jtag.help(),
  screenshot: async () => {
    const selector = process.argv[3] || 'body';
    const scale = process.argv[4] ? parseFloat(process.argv[4]) : 2.0;
    const result = await jtag.screenshot(selector, scale);
    if (result.success) {
      console.log('✅ Screenshot completed successfully');
    } else {
      console.log('❌ Screenshot failed');
    }
  },
  probe: async () => {
    const method = process.argv[3] || 'widgets';
    const result = await jtag.probe(method);
    if (result.success) {
      console.log('✅ Probe completed successfully');
    } else {
      console.log('❌ Probe failed');
    }
  },
  logs: async () => await jtag.logs(),
  errors: async () => await jtag.errors(),
  warnings: async () => await jtag.warnings(),
  session: async () => await jtag.session(),
  hotreload: async () => await jtag.hotreload(),
  health: async () => await jtag.health()
};

// Parse command line
const [,, command] = process.argv;

if (!command || command === 'help') {
  commands.help();
} else if (commands[command]) {
  await commands[command]();
} else {
  console.log(`❌ Unknown command: ${command}`);
  console.log('Run ./jtag help for available commands');
  process.exit(1);
}