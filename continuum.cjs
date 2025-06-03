#!/usr/bin/env node
/**
 * CONTINUUM CLI - Main Entry Point
 * 
 * npm install -g continuum
 * continuum
 * 
 * Launches web interface with real Claude pool
 */

// Forward to the new modular core implementation
const Continuum = require('./src/core/continuum-core.cjs');

// Create and start continuum instance
if (require.main === module) {
  const continuum = new Continuum();
  continuum.start().catch(error => {
    console.error('❌ Failed to start Continuum:', error);
    process.exit(1);
  });
}

module.exports = Continuum;