/**
 * Centralized Screenshot Utilities
 * Single source of truth for all html2canvas operations  
 * Proper ES module with import/export
 * Version: 1.2.0 - ES Module conversion
 */

export class ScreenshotUtils {
    /**
     * Take screenshot with consistent configuration and error handling
     * @param {HTMLElement} targetElement - Element to screenshot
     * @param {Object} options - Screenshot options
     * @returns {Promise<HTMLCanvasElement>} Canvas with screenshot
     */
    static async takeScreenshot(targetElement, options = {}) {
        const {
            scale = 1.0,
            backgroundColor = '#1a1a1a',
            allowTaint = true,
            useCORS = true,
            source = 'unknown'
        } = options;

        console.log(`📸 Taking screenshot: ${source} (${targetElement.tagName})`);

        if (typeof html2canvas === 'undefined') {
            throw new Error('html2canvas not available');
        }

        if (!targetElement) {
            throw new Error('Target element is required');
        }

        // Log element dimensions for debugging
        console.log(`📐 Target element: ${targetElement.offsetWidth}x${targetElement.offsetHeight}`);

        // Simple validation: reject if target element has zero dimensions
        if (targetElement.offsetWidth === 0 || targetElement.offsetHeight === 0) {
            const error = `Cannot screenshot, element size is ${targetElement.offsetWidth}x${targetElement.offsetHeight}`;
            console.error(`❌ ${error}`);
            return Promise.reject(new Error(error));
        }

        // For large captures (like document.body), check for problematic children
        if (targetElement === document.body || targetElement.tagName === 'BODY') {
            const zeroElements = targetElement.querySelectorAll('*');
            let zeroCount = 0;
            let canvasCount = 0;
            
            for (let element of zeroElements) {
                if (element.offsetWidth === 0 || element.offsetHeight === 0) {
                    zeroCount++;
                    if (element.tagName === 'CANVAS') {
                        canvasCount++;
                    }
                    if (zeroCount > 50) break; // Don't count all, just enough to know it's problematic
                }
            }
            
            // Be more aggressive for document.body - any zero-dimension elements are problematic
            if (zeroCount > 10) {
                const error = `Cannot screenshot document.body, found ${zeroCount} elements with 0x0 dimensions (${canvasCount} canvas). Use a more specific selector like '#main-content' instead.`;
                console.error(`❌ ${error}`);
                return Promise.reject(new Error(error));
            } else if (zeroCount > 0) {
                console.warn(`⚠️ Found ${zeroCount} zero-dimension elements, attempting screenshot with filtering...`);
            }
        }

        return html2canvas(targetElement, {
            allowTaint,
            useCORS,
            scale,
            backgroundColor,
            ignoreElements: function(element) {
                // CRITICAL FIX: Ignore zero-dimension elements to prevent createPattern error
                const isZero = element.offsetWidth === 0 || element.offsetHeight === 0;
                
                // Additional checks for problematic elements
                const isCanvas = element.tagName === 'CANVAS' && (element.width === 0 || element.height === 0);
                const isHidden = window.getComputedStyle(element).display === 'none';
                const isInvisible = window.getComputedStyle(element).visibility === 'hidden';
                
                const shouldIgnore = isZero || isCanvas || isHidden || isInvisible;
                
                if (shouldIgnore) {
                    const reason = isZero ? 'zero-dim' : isCanvas ? 'zero-canvas' : isHidden ? 'hidden' : 'invisible';
                    console.log(`🚫 Ignoring ${reason} element: ${element.tagName} (${source})`);
                }
                
                return shouldIgnore;
            }
        }).then(canvas => {
            console.log(`✅ Screenshot successful: ${canvas.width}x${canvas.height} (${source})`);
            return canvas;
        }).catch(error => {
            // Provide simple, clear error messages
            let simpleError = error.message;
            
            if (error.message && error.message.includes('createPattern') && error.message.includes('width or height of 0')) {
                simpleError = `Cannot screenshot, found elements with 0x0 dimensions. Try using a more specific selector instead of document.body.`;
            } else if (error.message && error.message.includes('html2canvas')) {
                simpleError = `Screenshot failed: ${error.message}. Try using a smaller element or different selector.`;
            }
            
            console.error(`❌ Screenshot failed (${source}): ${simpleError}`);
            throw new Error(simpleError);
        });
    }

    /**
     * Take screenshot and send via WebSocket
     * @param {HTMLElement} targetElement - Element to screenshot  
     * @param {Object} messageData - WebSocket message data
     * @param {Object} options - Screenshot options
     */
    static async takeScreenshotAndSend(targetElement, messageData, options = {}) {
        try {
            const canvas = await this.takeScreenshot(targetElement, options);
            const dataURL = canvas.toDataURL('image/png');
            
            const screenshotMessage = {
                type: 'screenshot_data',
                dataURL: dataURL,
                dimensions: { width: canvas.width, height: canvas.height },
                timestamp: Date.now(),
                ...messageData
            };

            // Calculate data sizes for detailed logging
            const dataURLSize = dataURL.length;
            const messageSize = JSON.stringify(screenshotMessage).length;
            const base64ImageSize = dataURL.replace(/^data:image\/png;base64,/, '').length;
            const estimatedPNGSize = Math.floor(base64ImageSize * 0.75); // Base64 to binary size estimate

            if (window.ws && window.ws.readyState === WebSocket.OPEN) {
                console.log(`📤 Sending screenshot: ${messageData.filename || 'unnamed'}`);
                console.log(`📊 Data sizes - Canvas: ${canvas.width}x${canvas.height}, DataURL: ${dataURLSize} chars, Message: ${messageSize} bytes, Est PNG: ${estimatedPNGSize} bytes`);
                window.ws.send(JSON.stringify(screenshotMessage));
                return { success: true, canvas, dataURL, dataSizes: { dataURLSize, messageSize, estimatedPNGSize } };
            } else {
                throw new Error('WebSocket not available');
            }
        } catch (error) {
            console.error('❌ Screenshot and send failed:', error);
            throw error;
        }
    }
}

// ES Module exports
export const VERSION = '1.2.0';
export default ScreenshotUtils;

// Backward compatibility - make available globally if needed
if (typeof window !== 'undefined') {
    window.ScreenshotUtils = ScreenshotUtils;
    window.ScreenshotUtils.VERSION = VERSION;
    
    // Register version with continuum if available
    if (window.continuum?.fileVersions) {
        window.continuum.fileVersions['ScreenshotUtils.js'] = VERSION;
        console.log('📦 ScreenshotUtils v1.2.0 registered with continuum (ES Module)');
    }
}