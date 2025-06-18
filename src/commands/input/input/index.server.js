import { createRequire } from 'module';
const require = createRequire(import.meta.url);

const InputCommand = require('./InputCommand.cjs');

export default {
  name: 'input',
  command: new InputCommand()
};