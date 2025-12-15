#!/usr/bin/env npx tsx
/**
 * Generate data/vacuum command
 */

import * as path from 'path';
import { CommandGenerator } from './CommandGenerator';
import type { CommandSpec } from './CommandNaming';

const spec: CommandSpec = {
  name: 'data/vacuum',
  description: 'Reclaim disk space after bulk deletes (VACUUM for SQLite, OPTIMIZE for MySQL, etc.)',
  category: 'data',
  params: [
    {
      name: 'dbHandle',
      type: 'string',
      optional: true,
      description: 'Database handle to vacuum (defaults to primary database)'
    }
  ],
  results: [
    {
      name: 'success',
      type: 'boolean',
      description: 'Whether vacuum completed successfully'
    },
    {
      name: 'dbHandle',
      type: 'string',
      description: 'Database handle that was vacuumed'
    },
    {
      name: 'beforeSize',
      type: 'number',
      description: 'Database size before vacuum (bytes)'
    },
    {
      name: 'afterSize',
      type: 'number',
      description: 'Database size after vacuum (bytes)'
    },
    {
      name: 'duration',
      type: 'number',
      description: 'Duration of vacuum operation (ms)'
    },
    {
      name: 'timestamp',
      type: 'string',
      description: 'When vacuum completed'
    }
  ],
  serverOnly: true,
  examples: [
    {
      description: 'Vacuum primary database',
      code: 'await Commands.execute(DATA_COMMANDS.VACUUM, {})'
    },
    {
      description: 'Vacuum specific database handle',
      code: 'await Commands.execute(DATA_COMMANDS.VACUUM, { dbHandle: "archive" })'
    }
  ]
};

async function main() {
  console.log('🏗️  Generating data/vacuum command...\n');

  const rootPath = path.join(__dirname, '..');
  const generator = new CommandGenerator(rootPath);

  generator.generate(spec, undefined, { force: false });

  console.log('\n✅ Command generated successfully!');
  console.log('\n📋 Next steps:');
  console.log('   1. Add VACUUM to DATA_COMMANDS in commands/data/shared/DataCommandConstants.ts');
  console.log('   2. Implement vacuum logic in DataVacuumServerCommand.ts');
  console.log('   3. Test with: ./jtag data/vacuum');
}

main().catch(console.error);
