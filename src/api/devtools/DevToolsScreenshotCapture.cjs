/**
 * DevTools Screenshot Capture - Advanced Screenshot System
 * ========================================================
 * 
 * High-level screenshot capture system with intelligent features:
 * 
 * - Multiple capture methods (DevTools Protocol, html2canvas fallback)
 * - Smart filename generation and organization
 * - Viewport and element-specific capture
 * - Session-aware screenshot management
 * - Automatic retry and error handling
 */

const fs = require('fs').promises;
const path = require('path');

class DevToolsScreenshotCapture {
    constructor(coordinator) {
        this.coordinator = coordinator;
        this.screenshotDir = '.continuum/screenshots';
        this.sessionScreenshotDir = '.continuum/artifacts';
        
        this.ensureDirectories();
    }

    async ensureDirectories() {
        await fs.mkdir(this.screenshotDir, { recursive: true });
        await fs.mkdir(this.sessionScreenshotDir, { recursive: true });
    }

    /**
     * Capture screenshot for specific session
     */
    async captureScreenshot(session, filename = null, options = {}) {
        try {
            // Generate filename if not provided
            if (!filename) {
                filename = this.generateFilename(session, options);
            }

            // Determine capture method
            const captureMethod = options.method || 'devtools';
            
            let screenshotData = null;
            
            if (captureMethod === 'devtools') {
                screenshotData = await this.captureViaDevTools(session, options);
            } else if (captureMethod === 'html2canvas') {
                screenshotData = await this.captureViaHtml2Canvas(session, options);
            } else {
                // Try DevTools first, fallback to html2canvas
                try {
                    screenshotData = await this.captureViaDevTools(session, options);
                } catch (error) {
                    console.warn(`⚠️ DevTools capture failed, trying html2canvas: ${error.message}`);
                    screenshotData = await this.captureViaHtml2Canvas(session, options);
                }
            }

            // Save screenshot
            const savedPath = await this.saveScreenshot(screenshotData, filename, session, options);
            
            console.log(`📸 Screenshot captured: ${savedPath}`);
            return {
                success: true,
                path: savedPath,
                filename: filename,
                method: captureMethod,
                session: session.sessionId
            };

        } catch (error) {
            console.error(`❌ Screenshot capture failed:`, error);
            return {
                success: false,
                error: error.message,
                session: session.sessionId
            };
        }
    }

    /**
     * Capture screenshot via DevTools Protocol
     */
    async captureViaDevTools(session, options = {}) {
        try {
            // Get tabs for the session
            const response = await fetch(`http://localhost:${session.port}/json`);
            const tabs = await response.json();
            
            const targetTab = tabs.find(tab => 
                tab.url.includes('localhost:9000') || 
                tab.url.includes(options.urlPattern || 'localhost')
            );

            if (!targetTab) {
                throw new Error('No suitable tab found for screenshot');
            }

            // Connect to tab via WebSocket
            const { WebSocket } = require('ws');
            const ws = new WebSocket(targetTab.webSocketDebuggerUrl);

            return new Promise((resolve, reject) => {
                let messageId = 1;
                const timeout = setTimeout(() => {
                    ws.close();
                    reject(new Error('Screenshot capture timeout'));
                }, 15000);

                ws.on('open', () => {
                    // Take screenshot
                    const screenshotParams = {
                        format: options.format || 'png',
                        quality: options.quality || 90,
                        fromSurface: true,
                        ...options.devtoolsParams
                    };

                    ws.send(JSON.stringify({
                        id: messageId++,
                        method: 'Page.captureScreenshot',
                        params: screenshotParams
                    }));
                });

                ws.on('message', (data) => {
                    try {
                        const message = JSON.parse(data.toString());
                        
                        if (message.result && message.result.data) {
                            clearTimeout(timeout);
                            ws.close();
                            resolve(message.result.data);
                        } else if (message.error) {
                            clearTimeout(timeout);
                            ws.close();
                            reject(new Error(`DevTools screenshot error: ${message.error.message}`));
                        }
                    } catch (error) {
                        clearTimeout(timeout);
                        ws.close();
                        reject(new Error(`Screenshot response parsing error: ${error.message}`));
                    }
                });

                ws.on('error', (error) => {
                    clearTimeout(timeout);
                    reject(new Error(`WebSocket error: ${error.message}`));
                });
            });

        } catch (error) {
            throw new Error(`DevTools screenshot failed: ${error.message}`);
        }
    }

    /**
     * Capture screenshot via html2canvas (fallback method)
     */
    async captureViaHtml2Canvas(session, options = {}) {
        try {
            // Use existing html2canvas integration
            const { spawn } = require('child_process');
            
            // Use Python client to capture via html2canvas
            const result = await new Promise((resolve, reject) => {
                const pythonProcess = spawn('python3', [
                    'python-client/ai-portal.py',
                    '--cmd', 'screenshot',
                    '--params', JSON.stringify({
                        filename: 'temp_html2canvas_capture',
                        method: 'html2canvas'
                    })
                ]);

                let stdout = '';
                let stderr = '';

                pythonProcess.stdout.on('data', (data) => {
                    stdout += data.toString();
                });

                pythonProcess.stderr.on('data', (data) => {
                    stderr += data.toString();
                });

                pythonProcess.on('close', (code) => {
                    if (code === 0) {
                        resolve(stdout);
                    } else {
                        reject(new Error(`html2canvas capture failed: ${stderr}`));
                    }
                });
            });

            // Read the captured file
            const tempPath = path.join(this.screenshotDir, 'temp_html2canvas_capture.png');
            const imageData = await fs.readFile(tempPath);
            
            // Clean up temp file
            await fs.unlink(tempPath).catch(() => {}); // Ignore cleanup errors
            
            return imageData.toString('base64');

        } catch (error) {
            throw new Error(`html2canvas screenshot failed: ${error.message}`);
        }
    }

    /**
     * Generate intelligent filename for screenshot
     */
    generateFilename(session, options = {}) {
        const timestamp = new Date().toISOString()
            .replace(/[:.-]/g, '')
            .replace('T', '_')
            .slice(0, 15);
        
        const parts = [
            options.prefix || session.purpose || 'screenshot',
            session.aiPersona !== 'system' ? session.aiPersona : null,
            timestamp
        ].filter(Boolean);

        return parts.join('_') + (options.extension || '.png');
    }

    /**
     * Save screenshot data to appropriate location
     */
    async saveScreenshot(screenshotData, filename, session, options = {}) {
        // Determine save location
        let savePath;
        
        if (options.sessionArtifact && session.artifactPath) {
            // Save to session artifact
            const artifactScreenshotDir = path.join(session.artifactPath, 'screenshots');
            await fs.mkdir(artifactScreenshotDir, { recursive: true });
            savePath = path.join(artifactScreenshotDir, filename);
        } else {
            // Save to global screenshots directory
            savePath = path.join(this.screenshotDir, filename);
        }

        // Convert base64 data to buffer
        const imageBuffer = Buffer.from(screenshotData, 'base64');
        
        // Save file
        await fs.writeFile(savePath, imageBuffer);
        
        // Create latest symlink if this is a session screenshot
        if (options.createLatestSymlink !== false) {
            await this.createLatestSymlink(savePath, session);
        }

        return savePath;
    }

    /**
     * Create latest symlink for easy access to most recent screenshot
     */
    async createLatestSymlink(screenshotPath, session) {
        try {
            const linkName = `latest_${session.purpose}_${session.aiPersona}.png`;
            const linkPath = path.join(this.screenshotDir, linkName);
            
            // Remove existing symlink
            await fs.unlink(linkPath).catch(() => {}); // Ignore if doesn't exist
            
            // Create new symlink
            await fs.symlink(path.resolve(screenshotPath), linkPath);
            
        } catch (error) {
            // Symlink creation is not critical
            console.warn(`⚠️ Could not create latest symlink: ${error.message}`);
        }
    }

    /**
     * Capture element-specific screenshot
     */
    async captureElement(session, selector, filename = null, options = {}) {
        try {
            // Inject script to get element bounds
            const elementScript = `
                const element = document.querySelector('${selector}');
                if (!element) throw new Error('Element not found: ${selector}');
                
                const rect = element.getBoundingClientRect();
                const scrollX = window.pageXOffset || document.documentElement.scrollLeft;
                const scrollY = window.pageYOffset || document.documentElement.scrollTop;
                
                ({
                    x: rect.left + scrollX,
                    y: rect.top + scrollY,
                    width: rect.width,
                    height: rect.height
                });
            `;

            // Execute script to get element bounds
            const tabs = await fetch(`http://localhost:${session.port}/json`).then(r => r.json());
            const targetTab = tabs.find(tab => tab.url.includes('localhost:9000'));
            
            if (!targetTab) {
                throw new Error('Target tab not found');
            }

            // Get element bounds via DevTools
            const { WebSocket } = require('ws');
            const ws = new WebSocket(targetTab.webSocketDebuggerUrl);

            const elementBounds = await new Promise((resolve, reject) => {
                let messageId = 1;
                const timeout = setTimeout(() => {
                    ws.close();
                    reject(new Error('Element bounds timeout'));
                }, 10000);

                ws.on('open', () => {
                    ws.send(JSON.stringify({
                        id: messageId++,
                        method: 'Runtime.evaluate',
                        params: {
                            expression: elementScript,
                            returnByValue: true
                        }
                    }));
                });

                ws.on('message', (data) => {
                    const message = JSON.parse(data.toString());
                    if (message.result && message.result.result) {
                        clearTimeout(timeout);
                        ws.close();
                        resolve(message.result.result.value);
                    } else if (message.error) {
                        clearTimeout(timeout);
                        ws.close();
                        reject(new Error(message.error.message));
                    }
                });
            });

            // Capture screenshot with element bounds
            const screenshotOptions = {
                ...options,
                devtoolsParams: {
                    clip: elementBounds,
                    ...options.devtoolsParams
                }
            };

            return await this.captureScreenshot(session, filename, screenshotOptions);

        } catch (error) {
            throw new Error(`Element screenshot failed: ${error.message}`);
        }
    }

    /**
     * Capture full page screenshot (scrolls to capture entire page)
     */
    async captureFullPage(session, filename = null, options = {}) {
        const fullPageOptions = {
            ...options,
            devtoolsParams: {
                captureBeyondViewport: true,
                ...options.devtoolsParams
            }
        };

        return await this.captureScreenshot(session, filename, fullPageOptions);
    }

    /**
     * Get recent screenshots for session
     */
    async getRecentScreenshots(session, limit = 5) {
        try {
            const screenshots = [];
            
            // Check session artifact screenshots
            if (session.artifactPath) {
                const artifactScreenshotDir = path.join(session.artifactPath, 'screenshots');
                try {
                    const files = await fs.readdir(artifactScreenshotDir);
                    const pngFiles = files.filter(f => f.endsWith('.png'));
                    
                    for (const file of pngFiles) {
                        const filePath = path.join(artifactScreenshotDir, file);
                        const stats = await fs.stat(filePath);
                        screenshots.push({
                            filename: file,
                            path: filePath,
                            created: stats.mtime,
                            size: stats.size,
                            location: 'artifact'
                        });
                    }
                } catch (error) {
                    // Artifact directory might not exist
                }
            }

            // Check global screenshots directory
            try {
                const files = await fs.readdir(this.screenshotDir);
                const sessionPattern = new RegExp(`${session.purpose}.*${session.aiPersona}.*\\.png$`);
                const sessionFiles = files.filter(f => sessionPattern.test(f));
                
                for (const file of sessionFiles) {
                    const filePath = path.join(this.screenshotDir, file);
                    const stats = await fs.stat(filePath);
                    screenshots.push({
                        filename: file,
                        path: filePath,
                        created: stats.mtime,
                        size: stats.size,
                        location: 'global'
                    });
                }
            } catch (error) {
                // Global directory might not exist
            }

            // Sort by creation time (newest first) and limit
            return screenshots
                .sort((a, b) => b.created - a.created)
                .slice(0, limit);

        } catch (error) {
            console.error(`❌ Failed to get recent screenshots:`, error);
            return [];
        }
    }
}

module.exports = DevToolsScreenshotCapture;