/**
 * Persistent Storage Module - Server Entry Point
 */

import { createRequire } from 'module';
const require = createRequire(import.meta.url);

const PersistentStorage = require('./PersistentStorage.cjs');

export default {
  storage: PersistentStorage,
  initialize: (environment) => {
    console.log(`📦 PersistentStorage module (${environment}) initialized`);
    return true;
  }
};