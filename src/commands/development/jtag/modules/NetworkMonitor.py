#!/usr/bin/env python3

"""
JTAG Network Monitor - Client-side network monitoring for autonomous debugging
============================================================================
Monitors network requests, WebSocket connections, API calls, and performance
metrics from the browser for real-time debugging insights.
"""

import asyncio
import json
from typing import Dict, Any, List, Optional
from .AsyncExecutor import AsyncExecutor

class NetworkMonitor:
    """Client-side network monitoring and analysis"""
    
    def __init__(self, executor: AsyncExecutor):
        self.executor = executor
    
    async def analyze_network_performance_async(self) -> Dict[str, Any]:
        """Analyze network performance and connection health"""
        js_code = """
        const networkInfo = {
            connection: navigator.connection ? {
                effectiveType: navigator.connection.effectiveType,
                downlink: navigator.connection.downlink,
                rtt: navigator.connection.rtt,
                saveData: navigator.connection.saveData
            } : null,
            
            performance: {
                navigation: performance.getEntriesByType('navigation')[0] || null,
                resources: performance.getEntriesByType('resource').map(entry => ({
                    name: entry.name,
                    duration: entry.duration,
                    transferSize: entry.transferSize,
                    encodedBodySize: entry.encodedBodySize,
                    decodedBodySize: entry.decodedBodySize,
                    initiatorType: entry.initiatorType,
                    responseStart: entry.responseStart,
                    responseEnd: entry.responseEnd
                })),
                
                timing: performance.timing ? {
                    domainLookupTime: performance.timing.domainLookupEnd - performance.timing.domainLookupStart,
                    connectTime: performance.timing.connectEnd - performance.timing.connectStart,
                    responseTime: performance.timing.responseEnd - performance.timing.responseStart,
                    domLoadTime: performance.timing.domContentLoadedEventEnd - performance.timing.domContentLoadedEventStart,
                    loadTime: performance.timing.loadEventEnd - performance.timing.loadEventStart
                } : null
            },
            
            websockets: window.continuumWebSocketStats || {
                active: 0,
                totalConnections: 0,
                reconnections: 0,
                lastMessageTime: null,
                messagesReceived: 0,
                messagesSent: 0
            },
            
            timestamp: new Date().toISOString()
        };
        
        return networkInfo;
        """
        
        return await self.executor.execute_js_async(js_code)
    
    async def monitor_api_calls_async(self) -> Dict[str, Any]:
        """Monitor API calls and response patterns"""
        js_code = """
        // Intercept fetch and XMLHttpRequest for API monitoring
        if (!window.jtag_network_monitor) {
            window.jtag_network_monitor = {
                apiCalls: [],
                startTime: Date.now(),
                
                interceptFetch: function() {
                    const originalFetch = window.fetch;
                    window.fetch = function(...args) {
                        const startTime = Date.now();
                        const url = args[0];
                        
                        return originalFetch.apply(this, args).then(response => {
                            const endTime = Date.now();
                            window.jtag_network_monitor.apiCalls.push({
                                url: url,
                                method: args[1]?.method || 'GET',
                                status: response.status,
                                duration: endTime - startTime,
                                timestamp: new Date().toISOString(),
                                success: response.ok
                            });
                            
                            // Keep only last 50 calls
                            if (window.jtag_network_monitor.apiCalls.length > 50) {
                                window.jtag_network_monitor.apiCalls.shift();
                            }
                            
                            return response;
                        }).catch(error => {
                            const endTime = Date.now();
                            window.jtag_network_monitor.apiCalls.push({
                                url: url,
                                method: args[1]?.method || 'GET',
                                status: 0,
                                duration: endTime - startTime,
                                timestamp: new Date().toISOString(),
                                success: false,
                                error: error.message
                            });
                            throw error;
                        });
                    };
                }
            };
            
            // Start intercepting
            window.jtag_network_monitor.interceptFetch();
        }
        
        const stats = {
            totalCalls: window.jtag_network_monitor.apiCalls.length,
            recentCalls: window.jtag_network_monitor.apiCalls.slice(-10),
            
            summary: {
                successful: window.jtag_network_monitor.apiCalls.filter(call => call.success).length,
                failed: window.jtag_network_monitor.apiCalls.filter(call => !call.success).length,
                averageResponseTime: window.jtag_network_monitor.apiCalls.reduce((sum, call) => sum + call.duration, 0) / 
                                   (window.jtag_network_monitor.apiCalls.length || 1),
                slowestCall: window.jtag_network_monitor.apiCalls.reduce((slowest, call) => 
                    call.duration > (slowest?.duration || 0) ? call : slowest, null)
            },
            
            monitoring: {
                active: true,
                startTime: window.jtag_network_monitor.startTime,
                monitoringDuration: Date.now() - window.jtag_network_monitor.startTime
            }
        };
        
        return stats;
        """
        
        return await self.executor.execute_js_async(js_code)
    
    async def monitor_websocket_health_async(self) -> Dict[str, Any]:
        """Monitor WebSocket connection health and message flow"""
        js_code = """
        const wsHealth = {
            continuum: {
                connected: window.continuum?.isConnected() || false,
                sessionId: window.continuum?.sessionId || null,
                lastActivity: window.continuum?.lastActivity || null,
                connectionAttempts: window.continuum?.connectionAttempts || 0,
                reconnections: window.continuum?.reconnections || 0
            },
            
            webSocketState: {
                readyState: window.continuum?.ws?.readyState || null,
                readyStateText: window.continuum?.ws ? 
                    ['CONNECTING', 'OPEN', 'CLOSING', 'CLOSED'][window.continuum.ws.readyState] : 
                    'NO_WEBSOCKET',
                url: window.continuum?.ws?.url || null,
                protocol: window.continuum?.ws?.protocol || null,
                bufferedAmount: window.continuum?.ws?.bufferedAmount || 0
            },
            
            messageStats: window.continuumMessageStats || {
                sent: 0,
                received: 0,
                errors: 0,
                lastMessageTime: null,
                averageLatency: 0
            },
            
            healthScore: function() {
                if (!window.continuum?.isConnected()) return 0;
                if (this.webSocketState.readyState !== 1) return 25;
                if (this.messageStats.errors > 5) return 50;
                if (this.continuum.reconnections > 3) return 75;
                return 100;
            }(),
            
            timestamp: new Date().toISOString()
        };
        
        return wsHealth;
        """
        
        return await self.executor.execute_js_async(js_code)
    
    async def analyze_resource_loading_async(self) -> Dict[str, Any]:
        """Analyze resource loading performance (CSS, JS, images)"""
        js_code = """
        const resources = performance.getEntriesByType('resource');
        
        const analysis = {
            byType: {},
            slowest: [],
            failed: [],
            cacheable: [],
            
            summary: {
                total: resources.length,
                totalSize: 0,
                totalDuration: 0,
                averageSize: 0,
                averageDuration: 0
            }
        };
        
        // Categorize resources
        resources.forEach(resource => {
            const type = resource.initiatorType || 'other';
            if (!analysis.byType[type]) {
                analysis.byType[type] = {
                    count: 0,
                    totalSize: 0,
                    totalDuration: 0,
                    resources: []
                };
            }
            
            const resourceData = {
                name: resource.name.split('/').pop() || resource.name,
                fullName: resource.name,
                duration: resource.duration,
                size: resource.transferSize || 0,
                cached: resource.transferSize === 0 && resource.decodedBodySize > 0,
                compressed: resource.encodedBodySize < resource.decodedBodySize,
                compressionRatio: resource.decodedBodySize > 0 ? 
                    resource.encodedBodySize / resource.decodedBodySize : 1
            };
            
            analysis.byType[type].count++;
            analysis.byType[type].totalSize += resourceData.size;
            analysis.byType[type].totalDuration += resourceData.duration;
            analysis.byType[type].resources.push(resourceData);
            
            analysis.summary.totalSize += resourceData.size;
            analysis.summary.totalDuration += resourceData.duration;
            
            // Track slowest resources
            if (resourceData.duration > 100) {
                analysis.slowest.push(resourceData);
            }
            
            // Track cacheable resources
            if (resourceData.cached) {
                analysis.cacheable.push(resourceData);
            }
        });
        
        // Calculate averages
        analysis.summary.averageSize = analysis.summary.totalSize / (analysis.summary.total || 1);
        analysis.summary.averageDuration = analysis.summary.totalDuration / (analysis.summary.total || 1);
        
        // Sort slowest
        analysis.slowest.sort((a, b) => b.duration - a.duration);
        analysis.slowest = analysis.slowest.slice(0, 10);
        
        return analysis;
        """
        
        return await self.executor.execute_js_async(js_code)
    
    async def get_network_summary_async(self) -> Dict[str, Any]:
        """Get comprehensive network health summary"""
        try:
            # Run all network checks concurrently
            performance_task = self.analyze_network_performance_async()
            api_calls_task = self.monitor_api_calls_async()
            websocket_task = self.monitor_websocket_health_async()
            resources_task = self.analyze_resource_loading_async()
            
            performance, api_calls, websocket, resources = await asyncio.gather(
                performance_task, api_calls_task, websocket_task, resources_task,
                return_exceptions=True
            )
            
            # Handle any exceptions gracefully
            def safe_result(result, fallback):
                return result if not isinstance(result, Exception) else {"error": str(result), **fallback}
            
            summary = {
                "network_performance": safe_result(performance, {"success": False}),
                "api_monitoring": safe_result(api_calls, {"success": False}),
                "websocket_health": safe_result(websocket, {"success": False}),
                "resource_analysis": safe_result(resources, {"success": False}),
                
                "overall_health": {
                    "score": 100,  # Will be calculated based on sub-scores
                    "issues": [],
                    "recommendations": []
                },
                
                "timestamp": asyncio.get_event_loop().time(),
                "success": True
            }
            
            # Calculate overall health score
            issues = []
            if websocket.get("data", {}).get("healthScore", 0) < 80:
                issues.append("WebSocket connection unstable")
                summary["overall_health"]["score"] -= 20
            
            if api_calls.get("data", {}).get("summary", {}).get("failed", 0) > 5:
                issues.append("High API failure rate")
                summary["overall_health"]["score"] -= 15
            
            if performance.get("data", {}).get("connection", {}).get("effectiveType") in ["slow-2g", "2g"]:
                issues.append("Slow network connection detected")
                summary["overall_health"]["score"] -= 10
            
            summary["overall_health"]["issues"] = issues
            
            return {"success": True, "data": summary}
            
        except Exception as e:
            return {"success": False, "error": f"Network summary failed: {str(e)}"}

# Sync convenience functions
def analyze_network_performance() -> Dict[str, Any]:
    """Sync wrapper for network performance analysis"""
    from .AsyncExecutor import AsyncExecutor
    executor = AsyncExecutor()
    monitor = NetworkMonitor(executor)
    return asyncio.run(monitor.analyze_network_performance_async())

def monitor_websocket_health() -> Dict[str, Any]:
    """Sync wrapper for WebSocket health monitoring"""
    from .AsyncExecutor import AsyncExecutor
    executor = AsyncExecutor()
    monitor = NetworkMonitor(executor)
    return asyncio.run(monitor.monitor_websocket_health_async())

def get_network_summary() -> Dict[str, Any]:
    """Sync wrapper for comprehensive network summary"""
    from .AsyncExecutor import AsyncExecutor
    executor = AsyncExecutor()
    monitor = NetworkMonitor(executor)
    return asyncio.run(monitor.get_network_summary_async())