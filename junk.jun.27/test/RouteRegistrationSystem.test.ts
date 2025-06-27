/**
 * Route Registration System Test - 50 lines max
 * Tests each component of the route registration system modularly
 */

import { describe, it, expect, beforeEach, afterEach } from '@jest/globals';
import { WebSocketDaemon } from '../WebSocketDaemon.js';
import { RendererDaemon } from '../../../daemons/renderer/RendererDaemon.js';

describe('Route Registration System', () => {
  let webSocketDaemon: WebSocketDaemon;
  let rendererDaemon: RendererDaemon;

  beforeEach(async () => {
    webSocketDaemon = new WebSocketDaemon({ port: 9005 });
    rendererDaemon = new RendererDaemon();
    await webSocketDaemon.start();
    await rendererDaemon.start();
  });

  afterEach(async () => {
    if (webSocketDaemon) await webSocketDaemon.stop();
    if (rendererDaemon) await rendererDaemon.stop();
  });

  it('WebSocketDaemon should have registerRouteHandler method', () => {
    expect(webSocketDaemon.registerRouteHandler).toBeDefined();
    expect(typeof webSocketDaemon.registerRouteHandler).toBe('function');
  });

  it('RendererDaemon should have registerWithWebSocketDaemon method', () => {
    expect(rendererDaemon.registerWithWebSocketDaemon).toBeDefined();
    expect(typeof rendererDaemon.registerWithWebSocketDaemon).toBe('function');
  });

  it('registerRouteHandler should store routes in routeHandlers Map', () => {
    const testHandler = () => 'test';
    webSocketDaemon.registerRouteHandler('/', { name: 'test' }, testHandler);
    
    const routeHandlers = (webSocketDaemon as any).routeHandlers;
    expect(routeHandlers.size).toBe(1);
    expect(routeHandlers.has('/')).toBe(true);
    expect(routeHandlers.get('/').handler).toBe(testHandler);
  });

  it('RendererDaemon registerWithWebSocketDaemon should call registerRouteHandler', () => {
    const registerSpy = jest.spyOn(webSocketDaemon, 'registerRouteHandler');
    
    rendererDaemon.registerWithWebSocketDaemon(webSocketDaemon);
    
    expect(registerSpy).toHaveBeenCalledWith('/', rendererDaemon, expect.any(Function));
    expect(registerSpy).toHaveBeenCalledWith('/src/*', rendererDaemon, expect.any(Function));
    expect(registerSpy).toHaveBeenCalledWith('/dist/*', rendererDaemon, expect.any(Function));
  });

  it('findRouteHandler should find registered routes', () => {
    rendererDaemon.registerWithWebSocketDaemon(webSocketDaemon);
    
    const rootHandler = (webSocketDaemon as any).findRouteHandler('/');
    const srcHandler = (webSocketDaemon as any).findRouteHandler('/src/ui/test.js');
    
    expect(rootHandler).not.toBeNull();
    expect(srcHandler).not.toBeNull();
    expect(rootHandler.daemon).toBe(rendererDaemon);
    expect(srcHandler.daemon).toBe(rendererDaemon);
  });
});