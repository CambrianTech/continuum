/**
 * Example Configuration Browser - Browser implementation
 *
 * BROWSER-ONLY: Uses window.location and document APIs.
 * Server code should NOT import this file.
 */

import type { ExampleDefinition } from '../shared/ExampleConfigTypes';

/**
 * Get active example configuration from browser environment
 * Derives configuration from current URL and document metadata
 */
export function getActiveExample(): ExampleDefinition {
  if (typeof window === 'undefined') {
    throw new Error('ExampleConfigBrowser can only be used in browser environment');
  }

  // Derive configuration from current port
  const currentPort = parseInt(window.location.port) || 9000;
  // WebSocket port is HTTP_PORT + 1 (e.g., HTTP=9000, WS=9001)
  const websocketPort = currentPort + 1;

  // Determine configuration based on URL path and document metadata
  const isWidgetUI = window.location.pathname.includes('widget') ||
                    document.title?.includes('Widget');

  return {
    name: isWidgetUI ? 'JTAG Widget Development UI' : 'JTAG Test Bench',
    description: isWidgetUI ? 'Focused widget development environment' : 'Full-featured testing environment',
    ports: {
      http_server: currentPort,
      websocket_server: websocketPort
    },
    paths: {
      directory: isWidgetUI ? 'examples/widget-ui' : 'examples/test-bench',
      html_file: isWidgetUI ? 'index.html' : 'public/demo.html',
      build_output: 'dist'
    },
    features: {}
  };
}

/**
 * Get active example ports
 */
export function getActivePorts(): Record<string, number> {
  const example = getActiveExample();
  return example.ports;
}

/**
 * Get active example name
 */
export function getActiveExampleName(): string {
  const example = getActiveExample();
  return example.name;
}
