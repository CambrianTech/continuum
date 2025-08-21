#!/usr/bin/env npx tsx
/**
 * Get Active Example Logs Script
 * 
 * Returns the correct logs path for the currently active example
 */

import { getActiveExamplePath } from '../system/shared/ExampleConfig';

function getActiveExampleLogsPath(): string {
  try {
    const activeExamplePath = getActiveExamplePath();
    return `${activeExamplePath}/.continuum/jtag/currentUser/logs/server.log`;
  } catch (error) {
    // Fallback to test-bench if configuration fails - NO CONSOLE OUTPUT
    return 'examples/test-bench/.continuum/jtag/currentUser/logs/server.log';
  }
}

// If called directly, output the path
if (require.main === module) {
  console.log(getActiveExampleLogsPath());
}