#!/usr/bin/env node
/**
 * AI Workspace System Test
 * ========================
 * Demonstrates how AI personas use autonomous workspace sandboxes
 */

const WorkspaceArtifact = require('./src/core/artifacts/WorkspaceArtifact.cjs');
const fs = require('fs').promises;
const path = require('path');

async function testAIWorkspaceSystem() {
    console.log('🤖 Testing AI Workspace Sandbox System\n');

    // Create workspace for AI Persona "DataViz"
    console.log('📦 Creating workspace for AI Persona: DataViz');
    const dataVizWorkspace = new WorkspaceArtifact('DataViz', 'session_001');
    
    await dataVizWorkspace.createStructure();
    console.log(`✅ Workspace created: ${dataVizWorkspace.artifactPath}`);
    console.log(`🎯 AI Persona: ${dataVizWorkspace.aiPersona}`);
    console.log(`🆔 Session ID: ${dataVizWorkspace.sessionId}\n`);

    // AI creates custom component
    console.log('🎨 AI designing custom chart component...');
    const chartComponent = `// Advanced Chart Component by DataViz AI
class AdvancedChart extends HTMLElement {
    constructor() {
        super();
        this.attachShadow({mode: 'open'});
        this.data = [];
    }
    
    connectedCallback() {
        this.render();
        this.setupInteractions();
        
        // Register with workspace
        if (window.aiWorkspace) {
            window.aiWorkspace.logEvent('component:chart_connected', {
                elementId: this.id,
                timestamp: new Date().toISOString()
            });
        }
    }
    
    render() {
        this.shadowRoot.innerHTML = \`
            <style>
                :host { 
                    display: block; 
                    border: 2px solid var(--ai-primary-color, #00ff88);
                    border-radius: 8px;
                    padding: 1rem;
                    margin: 1rem 0;
                    background: var(--ai-bg-secondary, #16213e);
                }
                
                .chart-header {
                    color: var(--ai-primary-color, #00ff88);
                    font-weight: bold;
                    margin-bottom: 1rem;
                }
                
                .chart-container { 
                    width: 100%; 
                    height: 300px;
                    background: var(--ai-bg-primary, #1a1a2e);
                    border: 1px solid var(--ai-border, #333);
                    border-radius: 4px;
                    position: relative;
                    overflow: hidden;
                }
                
                .data-point {
                    position: absolute;
                    width: 4px;
                    background: var(--ai-secondary, #0088ff);
                    bottom: 0;
                    transition: height 0.3s ease;
                }
                
                .chart-controls {
                    margin-top: 1rem;
                    text-align: center;
                }
                
                button {
                    background: var(--ai-primary-color, #00ff88);
                    color: var(--ai-bg-primary, #1a1a2e);
                    border: none;
                    padding: 0.5rem 1rem;
                    border-radius: 4px;
                    cursor: pointer;
                    margin: 0 0.5rem;
                    font-weight: bold;
                }
                
                button:hover {
                    opacity: 0.8;
                }
            </style>
            
            <div class="chart-header">📊 DataViz AI - Advanced Chart</div>
            <div class="chart-container" id="chart"></div>
            <div class="chart-controls">
                <button onclick="this.getRootNode().host.generateData()">Generate Data</button>
                <button onclick="this.getRootNode().host.animateChart()">Animate</button>
                <button onclick="this.getRootNode().host.exportData()">Export</button>
            </div>
        \`;
    }
    
    setupInteractions() {
        this.generateData();
    }
    
    generateData() {
        this.data = Array.from({length: 20}, () => Math.random() * 100);
        this.updateChart();
        
        if (window.aiWorkspace) {
            window.aiWorkspace.logEvent('chart:data_generated', {
                dataPoints: this.data.length,
                maxValue: Math.max(...this.data),
                minValue: Math.min(...this.data)
            });
        }
    }
    
    updateChart() {
        const chartContainer = this.shadowRoot.getElementById('chart');
        chartContainer.innerHTML = '';
        
        this.data.forEach((value, index) => {
            const point = document.createElement('div');
            point.className = 'data-point';
            point.style.left = \`\${(index / this.data.length) * 100}%\`;
            point.style.height = \`\${(value / 100) * 280}px\`;
            point.style.width = \`\${80 / this.data.length}%\`;
            chartContainer.appendChild(point);
        });
    }
    
    animateChart() {
        const points = this.shadowRoot.querySelectorAll('.data-point');
        points.forEach((point, index) => {
            setTimeout(() => {
                point.style.transform = 'scale(1.2)';
                setTimeout(() => {
                    point.style.transform = 'scale(1)';
                }, 200);
            }, index * 50);
        });
        
        if (window.aiWorkspace) {
            window.aiWorkspace.logEvent('chart:animated', {
                points: points.length
            });
        }
    }
    
    exportData() {
        const csvData = this.data.map((value, index) => \`\${index},\${value}\`).join('\\n');
        const blob = new Blob([csvData], { type: 'text/csv' });
        const url = URL.createObjectURL(blob);
        
        const a = document.createElement('a');
        a.href = url;
        a.download = 'chart_data.csv';
        a.click();
        
        if (window.aiWorkspace) {
            window.aiWorkspace.logEvent('chart:data_exported', {
                format: 'csv',
                rows: this.data.length
            });
        }
    }
}

customElements.define('advanced-chart', AdvancedChart);

// Auto-register with workspace if available
if (window.aiWorkspace) {
    window.aiWorkspace.registerComponent('advanced-chart', AdvancedChart);
    window.aiWorkspace.logEvent('component:chart_registered', {
        component: 'advanced-chart',
        features: ['data-generation', 'animation', 'export']
    });
}`;

    await dataVizWorkspace.saveComponent('advanced_chart', chartComponent);
    console.log('✅ Custom chart component saved');

    // AI creates custom CSS theme
    console.log('🎨 AI designing custom theme...');
    const customTheme = `/* DataViz AI Custom Theme */
:root {
    --ai-primary-color: #00ff88;
    --ai-secondary: #0088ff;
    --ai-accent: #ff0088;
    --ai-bg-primary: #0a0a0f;
    --ai-bg-secondary: #1a1a2e;
    --ai-text-primary: #00ff88;
    --ai-text-secondary: #88ffaa;
    --ai-border: #004422;
    --ai-shadow: rgba(0, 255, 136, 0.4);
}

/* DataViz specific styling */
.dataviz-workspace {
    background: linear-gradient(135deg, var(--ai-bg-primary), var(--ai-bg-secondary));
    min-height: 100vh;
    font-family: 'Fira Code', 'Monaco', monospace;
}

.data-visualization {
    border: 2px solid var(--ai-primary-color);
    border-radius: 12px;
    padding: 2rem;
    margin: 1rem;
    background: rgba(0, 255, 136, 0.05);
    backdrop-filter: blur(10px);
}

.ai-widget {
    transition: all 0.3s ease;
    border: 1px solid var(--ai-border);
    background: var(--ai-bg-secondary);
}

.ai-widget:hover {
    border-color: var(--ai-primary-color);
    box-shadow: 0 0 20px var(--ai-shadow);
    transform: translateY(-2px);
}

/* Glowing animations */
@keyframes data-pulse {
    0%, 100% { opacity: 1; }
    50% { opacity: 0.7; }
}

.data-point {
    animation: data-pulse 2s ease-in-out infinite;
}

/* Custom scrollbars */
::-webkit-scrollbar {
    width: 8px;
}

::-webkit-scrollbar-track {
    background: var(--ai-bg-primary);
}

::-webkit-scrollbar-thumb {
    background: var(--ai-primary-color);
    border-radius: 4px;
}`;

    await dataVizWorkspace.saveAsset('themes/dataviz_theme.css', customTheme);
    console.log('✅ Custom theme saved');

    // AI creates background task
    console.log('🔄 AI creating data collection task...');
    await dataVizWorkspace.logWorkspaceEvent('task:creating', {
        taskName: 'data_collector',
        purpose: 'Collect system metrics for visualization'
    });

    const dataCollectorTask = {
        name: 'System Metrics Collector',
        code: `
async function collectSystemMetrics() {
    const metrics = {
        timestamp: new Date().toISOString(),
        memory: performance.memory ? {
            used: performance.memory.usedJSHeapSize,
            total: performance.memory.totalJSHeapSize,
            limit: performance.memory.jsHeapSizeLimit
        } : null,
        timing: performance.timing ? {
            loadComplete: performance.timing.loadEventEnd - performance.timing.navigationStart,
            domReady: performance.timing.domContentLoadedEventEnd - performance.timing.navigationStart
        } : null,
        connection: navigator.connection ? {
            effectiveType: navigator.connection.effectiveType,
            downlink: navigator.connection.downlink
        } : null
    };
    
    // Log to workspace
    if (window.aiWorkspace) {
        window.aiWorkspace.logEvent('metrics:collected', metrics);
    }
    
    return metrics;
}

// Run collection every 10 seconds
const metricsInterval = setInterval(collectSystemMetrics, 10000);

// Initial collection
collectSystemMetrics();`,
        created: new Date(),
        status: 'active'
    };

    await dataVizWorkspace.saveTaskResult('task_001', 'data_collector', dataCollectorTask);
    console.log('✅ Data collection task saved');

    // AI deploys monitoring sentinel
    console.log('👁️ AI deploying performance sentinel...');
    await dataVizWorkspace.logWorkspaceEvent('sentinel:deploying', {
        sentinelName: 'performance_monitor',
        purpose: 'Monitor workspace performance and alert on issues'
    });

    const performanceSentinel = {
        name: 'Performance Monitor',
        code: `
function monitorPerformance() {
    const performance = {
        memory: window.performance.memory ? 
            window.performance.memory.usedJSHeapSize / window.performance.memory.totalJSHeapSize : 0,
        responseTime: window.performance.now(),
        domElements: document.querySelectorAll('*').length,
        eventListeners: window.aiWorkspace ? window.aiWorkspace.eventLog.length : 0
    };
    
    // Check for performance issues
    const alerts = [];
    
    if (performance.memory > 0.8) {
        alerts.push({
            type: 'HIGH_MEMORY_USAGE',
            value: performance.memory,
            threshold: 0.8,
            severity: 'warning'
        });
    }
    
    if (performance.domElements > 1000) {
        alerts.push({
            type: 'HIGH_DOM_COUNT',
            value: performance.domElements,
            threshold: 1000,
            severity: 'info'
        });
    }
    
    const result = {
        status: alerts.length > 0 ? 'alert' : 'healthy',
        performance: performance,
        alerts: alerts,
        timestamp: new Date().toISOString()
    };
    
    // Log to workspace
    if (window.aiWorkspace) {
        if (alerts.length > 0) {
            window.aiWorkspace.logEvent('sentinel:alert', result);
        } else {
            window.aiWorkspace.logEvent('sentinel:healthy', result);
        }
    }
    
    return result;
}

// Monitor every 30 seconds
const performanceInterval = setInterval(monitorPerformance, 30000);

// Initial check
monitorPerformance();`,
        deployed: new Date(),
        interval: 30000,
        status: 'active'
    };

    await dataVizWorkspace.saveTaskResult('sentinel_001', 'performance_monitor', performanceSentinel);
    console.log('✅ Performance sentinel deployed');

    // Log workspace events
    await dataVizWorkspace.logWorkspaceEvent('workspace:setup_complete', {
        components: 1,
        tasks: 1,
        sentinels: 1,
        theme: 'custom_dataviz'
    });

    // Save complete workspace
    await dataVizWorkspace.saveWorkspaceData();
    console.log('✅ Workspace data saved');

    // Display workspace summary
    console.log('\n📊 WORKSPACE SUMMARY:');
    console.log('='.repeat(50));
    console.log(`🤖 AI Persona: ${dataVizWorkspace.aiPersona}`);
    console.log(`🆔 Session ID: ${dataVizWorkspace.sessionId}`);
    console.log(`📁 Artifact Path: ${dataVizWorkspace.artifactPath}`);
    console.log(`📝 Events Logged: ${dataVizWorkspace.eventLog.length}`);
    console.log(`🧩 Components: ${dataVizWorkspace.components.size}`);
    console.log(`🔄 Tasks: ${dataVizWorkspace.runningTasks.size}`);
    console.log(`👁️ Sentinels: ${dataVizWorkspace.activeSentinels.size}`);

    // Show directory structure
    console.log('\n📂 WORKSPACE STRUCTURE:');
    console.log('='.repeat(50));
    await showDirectoryStructure(dataVizWorkspace.artifactPath);

    // Show some workspace files
    console.log('\n🎨 WORKSPACE MAIN HTML:');
    console.log('='.repeat(50));
    const mainHTML = await fs.readFile(path.join(dataVizWorkspace.artifactPath, 'workspace', 'main.html'), 'utf8');
    console.log(mainHTML.substring(0, 500) + '...\n');

    console.log('✅ AI Workspace System Test Complete!');
    console.log('\n🎯 KEY FEATURES DEMONSTRATED:');
    console.log('- Complete UI autonomy (HTML, CSS, JS)');
    console.log('- Custom component design and registration');
    console.log('- Background task execution and management');
    console.log('- Sentinel monitoring and alerting');
    console.log('- Event-driven architecture with logging');
    console.log('- Asset management (themes, icons, media)');
    console.log('- Session isolation and artifact persistence');
    console.log('- Inheritance-driven directory structure');

    return dataVizWorkspace.artifactPath;
}

async function showDirectoryStructure(dirPath, prefix = '', maxDepth = 3, currentDepth = 0) {
    if (currentDepth >= maxDepth) return;
    
    try {
        const items = await fs.readdir(dirPath);
        for (let i = 0; i < items.length; i++) {
            const item = items[i];
            const itemPath = path.join(dirPath, item);
            const isLast = i === items.length - 1;
            const currentPrefix = prefix + (isLast ? '└── ' : '├── ');
            const nextPrefix = prefix + (isLast ? '    ' : '│   ');
            
            const stat = await fs.stat(itemPath);
            if (stat.isDirectory()) {
                console.log(`${currentPrefix}📁 ${item}/`);
                await showDirectoryStructure(itemPath, nextPrefix, maxDepth, currentDepth + 1);
            } else {
                const size = stat.size < 1024 ? `${stat.size}B` : `${Math.round(stat.size/1024)}KB`;
                console.log(`${currentPrefix}📄 ${item} (${size})`);
            }
        }
    } catch (error) {
        console.log(`${prefix}❌ Error reading directory: ${error.message}`);
    }
}

// Run the test
if (require.main === module) {
    testAIWorkspaceSystem().catch(console.error);
}

module.exports = { testAIWorkspaceSystem };