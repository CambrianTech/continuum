import { createRequire } from 'module';
const require = createRequire(import.meta.url);

const TypeCommand = require('./TypeCommand.cjs');

export default {
  name: 'type',
  command: new TypeCommand()
};