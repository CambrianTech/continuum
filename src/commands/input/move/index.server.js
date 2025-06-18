import { createRequire } from 'module';
const require = createRequire(import.meta.url);

const MoveCommand = require('./MoveCommand.cjs');

export default {
  name: 'move',
  command: new MoveCommand()
};