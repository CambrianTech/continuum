#!/usr/bin/env python3

"""
JTAG Network Service Worker - Isolated network monitoring
=========================================================
Creates a service worker for network monitoring that runs in its own process,
capturing network events without affecting the main application performance.
"""

import asyncio
import json
from typing import Dict, Any, Optional
from .AsyncExecutor import AsyncExecutor

class NetworkServiceWorker:
    """Service worker-based network monitoring that won't crash the main app"""
    
    def __init__(self, executor: AsyncExecutor):
        self.executor = executor
        self.worker_installed = False
    
    async def install_service_worker_async(self) -> Dict[str, Any]:
        """Install the network monitoring service worker"""
        # First create the service worker JavaScript code
        service_worker_code = '''
        // JTAG Network Monitor Service Worker
        // Runs isolated from main thread - won't crash app
        
        const JTAG_NETWORK_CACHE = 'jtag-network-v1';
        let networkEvents = [];
        let maxEvents = 1000;
        
        // Network event capture
        self.addEventListener('fetch', function(event) {
            const startTime = Date.now();
            const request = event.request;
            
            // Don't monitor service worker requests
            if (request.url.includes('sw.js') || request.url.includes('service-worker')) {
                return;
            }
            
            event.respondWith(
                fetch(request.clone())
                    .then(response => {
                        const endTime = Date.now();
                        
                        // Capture network event
                        const networkEvent = {
                            id: Math.random().toString(36).substr(2, 9),
                            url: request.url,
                            method: request.method,
                            status: response.status,
                            statusText: response.statusText,
                            duration: endTime - startTime,
                            timestamp: new Date().toISOString(),
                            size: parseInt(response.headers.get('content-length') || '0'),
                            contentType: response.headers.get('content-type') || '',
                            cached: response.headers.get('x-cache') === 'HIT',
                            success: response.ok,
                            headers: {
                                request: Object.fromEntries(request.headers.entries()),
                                response: Object.fromEntries(response.headers.entries())
                            }
                        };
                        
                        // Store event (non-blocking)
                        networkEvents.push(networkEvent);
                        
                        // Keep only recent events
                        if (networkEvents.length > maxEvents) {
                            networkEvents = networkEvents.slice(-maxEvents);
                        }
                        
                        // Notify main thread (if needed)
                        self.clients.matchAll().then(clients => {
                            clients.forEach(client => {
                                client.postMessage({
                                    type: 'jtag-network-event',
                                    event: networkEvent
                                });
                            });
                        });
                        
                        return response;
                    })
                    .catch(error => {
                        const endTime = Date.now();
                        
                        const errorEvent = {
                            id: Math.random().toString(36).substr(2, 9),
                            url: request.url,
                            method: request.method,
                            status: 0,
                            statusText: 'Network Error',
                            duration: endTime - startTime,
                            timestamp: new Date().toISOString(),
                            size: 0,
                            success: false,
                            error: error.message || 'Unknown network error'
                        };
                        
                        networkEvents.push(errorEvent);
                        
                        if (networkEvents.length > maxEvents) {
                            networkEvents = networkEvents.slice(-maxEvents);
                        }
                        
                        throw error;
                    })
            );
        });
        
        // Handle messages from main thread
        self.addEventListener('message', function(event) {
            const { type, data } = event.data;
            
            switch (type) {
                case 'get-network-events':
                    const filterParams = data || {};
                    let filteredEvents = networkEvents;
                    
                    // Apply filters
                    if (filterParams.since) {
                        const sinceTime = new Date(filterParams.since);
                        filteredEvents = filteredEvents.filter(e => new Date(e.timestamp) > sinceTime);
                    }
                    
                    if (filterParams.status) {
                        filteredEvents = filteredEvents.filter(e => e.status === filterParams.status);
                    }
                    
                    if (filterParams.method) {
                        filteredEvents = filteredEvents.filter(e => e.method === filterParams.method);
                    }
                    
                    if (filterParams.domain) {
                        filteredEvents = filteredEvents.filter(e => e.url.includes(filterParams.domain));
                    }
                    
                    // Send response
                    event.ports[0].postMessage({
                        type: 'network-events-response',
                        events: filteredEvents.slice(-100), // Last 100 events
                        total: filteredEvents.length,
                        summary: generateSummary(filteredEvents)
                    });
                    break;
                    
                case 'clear-network-events':
                    networkEvents = [];
                    event.ports[0].postMessage({
                        type: 'network-events-cleared',
                        success: true
                    });
                    break;
                    
                case 'get-network-summary':
                    event.ports[0].postMessage({
                        type: 'network-summary-response',
                        summary: generateSummary(networkEvents),
                        eventCount: networkEvents.length
                    });
                    break;
            }
        });
        
        function generateSummary(events) {
            if (events.length === 0) {
                return {
                    totalRequests: 0,
                    successfulRequests: 0,
                    failedRequests: 0,
                    averageResponseTime: 0,
                    totalDataTransferred: 0,
                    cacheHitRatio: 0
                };
            }
            
            const successful = events.filter(e => e.success);
            const failed = events.filter(e => !e.success);
            const cached = events.filter(e => e.cached);
            
            return {
                totalRequests: events.length,
                successfulRequests: successful.length,
                failedRequests: failed.length,
                averageResponseTime: events.reduce((sum, e) => sum + e.duration, 0) / events.length,
                totalDataTransferred: events.reduce((sum, e) => sum + e.size, 0),
                cacheHitRatio: events.length > 0 ? (cached.length / events.length) * 100 : 0,
                byStatus: events.reduce((acc, e) => {
                    acc[e.status] = (acc[e.status] || 0) + 1;
                    return acc;
                }, {}),
                byMethod: events.reduce((acc, e) => {
                    acc[e.method] = (acc[e.method] || 0) + 1;
                    return acc;
                }, {}),
                slowestRequests: events.sort((a, b) => b.duration - a.duration).slice(0, 5)
            };
        }
        
        // Install event
        self.addEventListener('install', function(event) {
            console.log('JTAG Network Monitor Service Worker installed');
            self.skipWaiting();
        });
        
        // Activate event
        self.addEventListener('activate', function(event) {
            console.log('JTAG Network Monitor Service Worker activated');
            event.waitUntil(self.clients.claim());
        });
        '''
        
        # Install the service worker via JavaScript
        install_js = f'''
        if ('serviceWorker' in navigator) {{
            // Create service worker blob
            const swBlob = new Blob([`{service_worker_code}`], {{ type: 'application/javascript' }});
            const swUrl = URL.createObjectURL(swBlob);
            
            navigator.serviceWorker.register(swUrl, {{ scope: '/' }})
                .then(function(registration) {{
                    console.log('JTAG Network Monitor SW registered:', registration);
                    
                    // Set up message listener
                    navigator.serviceWorker.addEventListener('message', function(event) {{
                        if (event.data.type === 'jtag-network-event') {{
                            // Store in window for easy access
                            if (!window.jtagNetworkEvents) {{
                                window.jtagNetworkEvents = [];
                            }}
                            window.jtagNetworkEvents.push(event.data.event);
                            
                            // Keep only recent events in memory
                            if (window.jtagNetworkEvents.length > 100) {{
                                window.jtagNetworkEvents = window.jtagNetworkEvents.slice(-100);
                            }}
                        }}
                    }});
                    
                    return {{ success: true, registered: true, scope: registration.scope }};
                }})
                .catch(function(error) {{
                    console.error('JTAG Network Monitor SW registration failed:', error);
                    return {{ success: false, error: error.message }};
                }});
        }} else {{
            return {{ success: false, error: 'Service Workers not supported' }};
        }}
        '''
        
        result = await self.executor.execute_js_async(install_js)
        if result.get('success'):
            self.worker_installed = True
        return result
    
    async def get_network_events_async(self, filters: Optional[Dict[str, Any]] = None) -> Dict[str, Any]:
        """Get network events from service worker"""
        if not self.worker_installed:
            install_result = await self.install_service_worker_async()
            if not install_result.get('success'):
                return install_result
        
        js_code = f'''
        if ('serviceWorker' in navigator && navigator.serviceWorker.controller) {{
            const channel = new MessageChannel();
            
            return new Promise((resolve) => {{
                channel.port1.onmessage = function(event) {{
                    resolve(event.data);
                }};
                
                navigator.serviceWorker.controller.postMessage({{
                    type: 'get-network-events',
                    data: {json.dumps(filters or {})}
                }}, [channel.port2]);
                
                // Timeout after 5 seconds
                setTimeout(() => {{
                    resolve({{ success: false, error: 'Service worker timeout' }});
                }}, 5000);
            }});
        }} else {{
            return {{ success: false, error: 'Service worker not available' }};
        }}
        '''
        
        return await self.executor.execute_js_async(js_code)
    
    async def get_network_summary_async(self) -> Dict[str, Any]:
        """Get network summary from service worker"""
        if not self.worker_installed:
            install_result = await self.install_service_worker_async()
            if not install_result.get('success'):
                return install_result
        
        js_code = '''
        if ('serviceWorker' in navigator && navigator.serviceWorker.controller) {
            const channel = new MessageChannel();
            
            return new Promise((resolve) => {
                channel.port1.onmessage = function(event) {
                    resolve(event.data);
                };
                
                navigator.serviceWorker.controller.postMessage({
                    type: 'get-network-summary'
                }, [channel.port2]);
                
                setTimeout(() => {
                    resolve({ success: false, error: 'Service worker timeout' });
                }, 5000);
            });
        } else {
            return { success: false, error: 'Service worker not available' };
        }
        '''
        
        return await self.executor.execute_js_async(js_code)
    
    async def clear_network_events_async(self) -> Dict[str, Any]:
        """Clear network events in service worker"""
        if not self.worker_installed:
            return {"success": False, "error": "Service worker not installed"}
        
        js_code = '''
        if ('serviceWorker' in navigator && navigator.serviceWorker.controller) {
            const channel = new MessageChannel();
            
            return new Promise((resolve) => {
                channel.port1.onmessage = function(event) {
                    resolve(event.data);
                };
                
                navigator.serviceWorker.controller.postMessage({
                    type: 'clear-network-events'
                }, [channel.port2]);
                
                setTimeout(() => {
                    resolve({ success: false, error: 'Service worker timeout' });
                }, 3000);
            });
        } else {
            return { success: false, error: 'Service worker not available' };
        }
        '''
        
        return await self.executor.execute_js_async(js_code)
    
    async def get_live_network_stats_async(self) -> Dict[str, Any]:
        """Get live network statistics (non-blocking)"""
        # This combines service worker data with immediate browser stats
        tasks = [
            self.get_network_summary_async(),
            self._get_browser_network_stats_async()
        ]
        
        try:
            sw_stats, browser_stats = await asyncio.gather(*tasks, return_exceptions=True)
            
            return {
                "success": True,
                "data": {
                    "service_worker": sw_stats if not isinstance(sw_stats, Exception) else {"error": str(sw_stats)},
                    "browser": browser_stats if not isinstance(browser_stats, Exception) else {"error": str(browser_stats)},
                    "timestamp": asyncio.get_event_loop().time()
                }
            }
        except Exception as e:
            return {"success": False, "error": f"Live stats error: {str(e)}"}
    
    async def _get_browser_network_stats_async(self) -> Dict[str, Any]:
        """Get immediate browser network stats"""
        js_code = '''
        const stats = {
            connection: navigator.connection ? {
                effectiveType: navigator.connection.effectiveType,
                downlink: navigator.connection.downlink,
                rtt: navigator.connection.rtt
            } : null,
            
            performance: {
                memory: performance.memory ? {
                    used: Math.round(performance.memory.usedJSHeapSize / 1024 / 1024),
                    total: Math.round(performance.memory.totalJSHeapSize / 1024 / 1024)
                } : null
            },
            
            webSocket: window.continuum ? {
                connected: window.continuum.isConnected(),
                readyState: window.continuum.ws?.readyState,
                bufferedAmount: window.continuum.ws?.bufferedAmount || 0
            } : null,
            
            activeRequests: window.jtagNetworkEvents ? 
                window.jtagNetworkEvents.filter(e => Date.now() - new Date(e.timestamp).getTime() < 5000).length : 0
        };
        
        return stats;
        '''
        
        return await self.executor.execute_js_async(js_code)

# Sync convenience functions
def install_network_monitor() -> Dict[str, Any]:
    """Install network monitoring service worker"""
    from .AsyncExecutor import AsyncExecutor
    executor = AsyncExecutor()
    monitor = NetworkServiceWorker(executor)
    return asyncio.run(monitor.install_service_worker_async())

def get_network_events(filters: Optional[Dict[str, Any]] = None) -> Dict[str, Any]:
    """Get network events from service worker"""
    from .AsyncExecutor import AsyncExecutor
    executor = AsyncExecutor()
    monitor = NetworkServiceWorker(executor)
    return asyncio.run(monitor.get_network_events_async(filters))

def get_live_network_stats() -> Dict[str, Any]:
    """Get live network statistics"""
    from .AsyncExecutor import AsyncExecutor
    executor = AsyncExecutor()
    monitor = NetworkServiceWorker(executor)
    return asyncio.run(monitor.get_live_network_stats_async())