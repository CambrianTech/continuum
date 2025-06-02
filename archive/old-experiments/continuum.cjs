#!/usr/bin/env node
/**
 * CONTINUUM CLI - Main Entry Point
 * 
 * npm install -g continuum
 * continuum
 * 
 * Launches web interface with real Claude pool
 */

// Forward to the actual core implementation
const Continuum = require('./src/core/continuum.cjs');

// Create and start continuum instance
if (require.main === module) {
  const continuum = new Continuum();
  // Auto-start happens in constructor
}

module.exports = Continuum;