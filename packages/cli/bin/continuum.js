#!/usr/bin/env node

// This is the entry point for the CLI
// It imports the compiled JS code and runs it

// Ensures the script runs with the correct Node.js version
const currentNodeVersion = process.versions.node;
const semver = currentNodeVersion.split('.');
const major = parseInt(semver[0], 10);

if (major < 18) {
  console.error(
    'You are running Node ' +
      currentNodeVersion +
      '.\n' +
      'The Continuum CLI requires Node 18 or higher. \n' +
      'Please update your version of Node.'
  );
  process.exit(1);
}

// Import the compiled CLI code
require('../dist/index');