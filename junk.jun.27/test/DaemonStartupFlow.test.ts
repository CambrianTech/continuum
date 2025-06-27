/**
 * Daemon Startup Flow Test - 50 lines max
 * Tests the actual daemon registration flow during startup
 */

import { describe, it, expect, beforeEach, afterEach } from '@jest/globals';
import { WebSocketDaemon } from '../WebSocketDaemon.js';
import { RendererDaemon } from '../../../daemons/renderer/RendererDaemon.js';

describe('Daemon Startup Flow', () => {
  let webSocketDaemon: WebSocketDaemon;
  let rendererDaemon: RendererDaemon;

  beforeEach(async () => {
    webSocketDaemon = new WebSocketDaemon({ port: 9006 });
    rendererDaemon = new RendererDaemon();
    await webSocketDaemon.start();
    await rendererDaemon.start();
  });

  afterEach(async () => {
    if (webSocketDaemon) await webSocketDaemon.stop();
    if (rendererDaemon) await rendererDaemon.stop();
  });

  it('registerExternalDaemon should detect registerWithWebSocketDaemon method', async () => {
    const registerSpy = jest.spyOn(rendererDaemon, 'registerWithWebSocketDaemon');
    
    await webSocketDaemon.registerExternalDaemon('renderer', rendererDaemon);
    
    expect(registerSpy).toHaveBeenCalledWith(webSocketDaemon);
  });

  it('registerExternalDaemon should result in registered routes', async () => {
    await webSocketDaemon.registerExternalDaemon('renderer', rendererDaemon);
    
    const routeHandlers = (webSocketDaemon as any).routeHandlers;
    console.log('Routes after registerExternalDaemon:', [...routeHandlers.keys()]);
    
    expect(routeHandlers.size).toBeGreaterThan(0);
    expect(routeHandlers.has('/')).toBe(true);
    expect(routeHandlers.has('/src/*')).toBe(true);
  });

  it('BUG TEST: Production startup should have routes registered', async () => {
    // This test reproduces the production bug
    // In production, routes are not being registered despite daemon registration
    
    await webSocketDaemon.registerExternalDaemon('renderer', rendererDaemon);
    
    const routeHandlers = (webSocketDaemon as any).routeHandlers;
    const rootHandler = (webSocketDaemon as any).findRouteHandler('/');
    
    console.log('Available routes:', [...routeHandlers.keys()]);
    console.log('Root handler found:', !!rootHandler);
    
    // This should pass but fails in production
    expect(routeHandlers.size).toBeGreaterThan(0);
    expect(rootHandler).not.toBeNull();
  });
});