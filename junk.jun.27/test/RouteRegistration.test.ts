/**
 * Route Registration Test - 50 lines max
 * Tests WebSocketDaemon route registration and lookup
 */

import { describe, it, expect, beforeEach, afterEach } from '@jest/globals';
import { WebSocketDaemon } from '../WebSocketDaemon.js';

describe('Route Registration', () => {
  let daemon: WebSocketDaemon;

  beforeEach(async () => {
    daemon = new WebSocketDaemon({ port: 9003 });
    await daemon.start();
  });

  afterEach(async () => {
    if (daemon) {
      await daemon.stop();
    }
  });

  it('should register and find exact route match', () => {
    let handlerCalled = false;
    const testHandler = () => { handlerCalled = true; };
    
    daemon.registerRouteHandler('/', { name: 'test' }, testHandler);
    
    // Test the private findRouteHandler method
    const routeHandler = (daemon as any).findRouteHandler('/');
    
    expect(routeHandler).toBeDefined();
    expect(routeHandler.handler).toBe(testHandler);
    expect(routeHandler.daemon.name).toBe('test');
  });

  it('should register and find pattern route match', () => {
    let handlerCalled = false;
    const testHandler = () => { handlerCalled = true; };
    
    daemon.registerRouteHandler('/src/*', { name: 'test' }, testHandler);
    
    const routeHandler = (daemon as any).findRouteHandler('/src/ui/continuum.js');
    
    expect(routeHandler).toBeDefined();
    expect(routeHandler.handler).toBe(testHandler);
  });

  it('should return null for non-matching routes', () => {
    const routeHandler = (daemon as any).findRouteHandler('/nonexistent');
    expect(routeHandler).toBeNull();
  });

  it('should prioritize exact matches over patterns', () => {
    const exactHandler = () => 'exact';
    const patternHandler = () => 'pattern';
    
    daemon.registerRouteHandler('/src/*', { name: 'pattern' }, patternHandler);
    daemon.registerRouteHandler('/src/exact', { name: 'exact' }, exactHandler);
    
    const exactMatch = (daemon as any).findRouteHandler('/src/exact');
    const patternMatch = (daemon as any).findRouteHandler('/src/other');
    
    expect(exactMatch.handler).toBe(exactHandler);
    expect(patternMatch.handler).toBe(patternHandler);
  });
});