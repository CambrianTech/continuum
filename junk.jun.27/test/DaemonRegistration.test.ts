/**
 * Daemon Registration Test - Tests route registration bug
 * Verifies that RendererDaemon routes are actually registered with WebSocketDaemon
 */

import { describe, it, expect, beforeEach, afterEach } from '@jest/globals';
import { WebSocketDaemon } from '../WebSocketDaemon.js';
import { RendererDaemon } from '../../../daemons/renderer/RendererDaemon.js';

describe('Daemon Registration Bug Fix', () => {
  let webSocketDaemon: WebSocketDaemon;
  let rendererDaemon: RendererDaemon;

  beforeEach(async () => {
    webSocketDaemon = new WebSocketDaemon({ port: 9004 });
    rendererDaemon = new RendererDaemon();
    await webSocketDaemon.start();
    await rendererDaemon.start();
  });

  afterEach(async () => {
    if (webSocketDaemon) await webSocketDaemon.stop();
    if (rendererDaemon) await rendererDaemon.stop();
  });

  it('RendererDaemon should have registerWithWebSocketDaemon method', () => {
    expect(rendererDaemon.registerWithWebSocketDaemon).toBeDefined();
    expect(typeof rendererDaemon.registerWithWebSocketDaemon).toBe('function');
  });

  it('BUG: registerWithWebSocketDaemon should register routes', () => {
    // Call the registration method directly
    rendererDaemon.registerWithWebSocketDaemon(webSocketDaemon);
    
    // Check if routes were actually registered
    const routeHandlers = (webSocketDaemon as any).routeHandlers;
    
    console.log('Registered routes:', [...routeHandlers.keys()]);
    
    expect(routeHandlers.has('/')).toBe(true);
    expect(routeHandlers.has('/src/*')).toBe(true);
    expect(routeHandlers.has('/dist/*')).toBe(true);
  });

  it('BUG: registerExternalDaemon should call registerWithWebSocketDaemon', async () => {
    // Register the renderer daemon with WebSocket daemon
    await webSocketDaemon.registerExternalDaemon('renderer', rendererDaemon);
    
    // Verify routes were registered
    const routeHandlers = (webSocketDaemon as any).routeHandlers;
    
    console.log('Routes after external registration:', [...routeHandlers.keys()]);
    
    expect(routeHandlers.size).toBeGreaterThan(0);
    expect(routeHandlers.has('/')).toBe(true);
    expect(routeHandlers.has('/src/*')).toBe(true);
  });

  it('FIXED: route lookup should find registered routes', async () => {
    await webSocketDaemon.registerExternalDaemon('renderer', rendererDaemon);
    
    const rootHandler = (webSocketDaemon as any).findRouteHandler('/');
    const srcHandler = (webSocketDaemon as any).findRouteHandler('/src/ui/continuum.js');
    
    expect(rootHandler).not.toBeNull();
    expect(srcHandler).not.toBeNull();
    expect(srcHandler.daemon).toBe(rendererDaemon);
  });
});