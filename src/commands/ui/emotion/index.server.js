import { createRequire } from 'module';
const require = createRequire(import.meta.url);

const EmotionCommand = require('./EmotionCommand.cjs');

export default {
  name: 'emotion',
  command: new EmotionCommand()
};