import { createRequire } from 'module';
const require = createRequire(import.meta.url);

const CursorCommand = require('./CursorCommand.cjs');

export default {
  name: 'cursor',
  command: new CursorCommand()
};