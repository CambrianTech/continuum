/**
 * Session Command Module Exports
 * Low-level OS-like session management for Continuum
 */

const SessionCommand = require('./SessionCommand.cjs');

module.exports = {
  command: SessionCommand,
  name: 'session',
  category: 'Core'
};