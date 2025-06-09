/**
 * Screenshot Command
 * Captures browser screenshots with various options
 */

class ScreenshotCommand {
  static getDefinition() {
    return {
      name: 'SCREENSHOT',
      category: 'Core',
      icon: '📸',
      description: 'Take screenshot of browser',
      params: '[selector] [coordinates] [resolution] [format] [quality]',
      examples: [
        '{}',
        '{"params": "selector .sidebar"}',
        '{"params": "100,200,800,600"}',
        '{"params": "1920x1080 format jpeg quality 0.8"}'
      ],
      usage: 'Captures current browser state. Use selector for specific elements, coordinates for regions, or leave empty for full page.'
    };
  }
  
  static async execute(params, continuum) {
    console.log('📸 SCREENSHOT COMMAND: Starting execution with params:', params);
    console.log('📸 SCREENSHOT COMMAND: Continuum has webSocketServer:', !!continuum.webSocketServer);
    
    // Parse parameters
    const options = ScreenshotCommand.parseParams(params);
    console.log('📸 SCREENSHOT COMMAND: Parsed options:', options);
    
    // Send screenshot request to browser via WebSocket
    if (continuum.webSocketServer) {
      const screenshotData = {
        x: options.x || 0,
        y: options.y || 0,
        width: options.width || 0,
        height: options.height || 0,
        selector: options.selector || '',
        scale: options.scale || 1,
        resolutionWidth: options.resolutionWidth || 0,
        resolutionHeight: options.resolutionHeight || 0,
        quality: options.quality || 0.92,
        format: options.format || 'png'
      };
      
      // Initialize screenshot data storage if not exists
      if (!continuum.screenshotData) {
        continuum.screenshotData = new Map();
      }
      
      // Send screenshot request
      console.log('📸 SCREENSHOT COMMAND: Broadcasting to WebSocket with data:', screenshotData);
      const jsCommand = ScreenshotCommand.generateScreenshotJS(screenshotData);
      console.log('📸 SCREENSHOT COMMAND: Generated JS command length:', jsCommand.length);
      
      continuum.webSocketServer.broadcast({
        type: 'execute_js',
        data: {
          command: jsCommand
        }
      });
      
      console.log('📸 SCREENSHOT COMMAND: WebSocket broadcast complete');
      
      // Wait for screenshot data to be received (up to 10 seconds)
      const timeout = 10000;
      const startTime = Date.now();
      
      while (Date.now() - startTime < timeout) {
        // Check if any new screenshots were captured
        for (const [screenshotId, data] of continuum.screenshotData.entries()) {
          if (data.capturedAt && new Date(data.capturedAt).getTime() > startTime) {
            // Found a recent screenshot, save it using ScreenshotService
            let saveResult = null;
            if (continuum.screenshotService && data.dataURL) {
              saveResult = await continuum.screenshotService.saveBrowserScreenshot(
                data.dataURL,
                data.filename
              );
            }
            
            const result = {
              success: true,
              message: saveResult && saveResult.success 
                ? `Screenshot captured and saved: ${saveResult.filename}`
                : 'Screenshot captured (save failed)',
              parameters: screenshotData,
              filename: data.filename,
              timestamp: data.timestamp,
              dataURL: data.dataURL,
              screenshotId: screenshotId,
              saved: saveResult && saveResult.success,
              savePath: saveResult && saveResult.path
            };
            
            // Clean up the stored screenshot data
            continuum.screenshotData.delete(screenshotId);
            
            return result;
          }
        }
        
        // Wait 100ms before checking again
        await new Promise(resolve => setTimeout(resolve, 100));
      }
      
      // Timeout - return partial success
      return {
        success: true,
        message: 'Screenshot command sent to browser (waiting for response timed out)',
        parameters: screenshotData,
        timestamp: new Date().toISOString(),
        note: 'Screenshot may still be processing in browser'
      };
    }
    
    return {
      success: false,
      error: 'WebSocket server not available'
    };
  }
  
  static parseParams(params) {
    const options = {};
    
    if (!params || params.trim() === '') {
      return options;
    }
    
    // Parse different parameter formats
    if (params.includes('selector')) {
      const selectorMatch = params.match(/selector\s+([^\s]+)/);
      if (selectorMatch) options.selector = selectorMatch[1];
    }
    
    if (params.includes('format')) {
      const formatMatch = params.match(/format\s+(\w+)/);
      if (formatMatch) options.format = formatMatch[1];
    }
    
    if (params.includes('quality')) {
      const qualityMatch = params.match(/quality\s+([\d.]+)/);
      if (qualityMatch) options.quality = parseFloat(qualityMatch[1]);
    }
    
    // Parse coordinates (x,y,width,height)
    const coordMatch = params.match(/(\d+),(\d+),(\d+),(\d+)/);
    if (coordMatch) {
      options.x = parseInt(coordMatch[1]);
      options.y = parseInt(coordMatch[2]);
      options.width = parseInt(coordMatch[3]);
      options.height = parseInt(coordMatch[4]);
    }
    
    // Parse resolution (WIDTHxHEIGHT)
    const resMatch = params.match(/(\d+)x(\d+)/);
    if (resMatch) {
      options.resolutionWidth = parseInt(resMatch[1]);
      options.resolutionHeight = parseInt(resMatch[2]);
    }
    
    return options;
  }
  
  static generateScreenshotJS(options) {
    return `
      console.log('📸 Screenshot API: Starting capture...');
      
      const captureParams = ${JSON.stringify(options)};
      
      // Wait for HTML to fully load before proceeding
      function waitForPageReady(callback) {
        if (document.readyState === 'complete') {
          console.log('📸 Page already loaded, proceeding...');
          setTimeout(callback, 100); // Small delay to ensure rendering is complete
        } else {
          console.log('📸 Waiting for page to load...');
          window.addEventListener('load', function() {
            console.log('📸 Page loaded, proceeding...');
            setTimeout(callback, 100); // Small delay after load
          });
        }
      }
      
      // Load html2canvas if not already loaded
      if (typeof html2canvas === 'undefined') {
        const script = document.createElement('script');
        script.src = 'https://cdnjs.cloudflare.com/ajax/libs/html2canvas/1.4.1/html2canvas.min.js';
        script.onload = function() {
          console.log('📸 html2canvas loaded, waiting for page ready...');
          waitForPageReady(captureScreenshot);
        };
        document.head.appendChild(script);
      } else {
        waitForPageReady(captureScreenshot);
      }
      
      function captureScreenshot() {
        const timestamp = Date.now();
        let filename = 'continuum-screenshot-' + timestamp + '.' + captureParams.format;
        
        let targetElement = document.body;
        let captureOptions = {
          allowTaint: true,
          useCORS: true,
          scale: captureParams.scale,
          scrollX: 0,
          scrollY: 0,
          backgroundColor: '#1a1a1a'
        };
        
        // Handle selector-based capture
        if (captureParams.selector) {
          const element = document.querySelector(captureParams.selector);
          if (element) {
            targetElement = element;
            filename = 'continuum-element-' + captureParams.selector.replace(/[^a-zA-Z0-9]/g, '-') + '-' + timestamp + '.' + captureParams.format;
            console.log('📸 Capturing element:', captureParams.selector);
          } else {
            console.warn('📸 Selector not found:', captureParams.selector);
          }
        }
        
        // AGGRESSIVE element filtering to prevent createPattern errors
        console.log('📸 AGGRESSIVE filtering of ALL problematic elements...');
        const problematicElements = [];
        
        // Remove ALL canvas elements temporarily - they cause createPattern errors
        const allCanvases = document.querySelectorAll('canvas');
        allCanvases.forEach(canvas => {
          console.log('📸 Removing canvas element:', {
            width: canvas.width,
            height: canvas.height,
            offsetWidth: canvas.offsetWidth,
            offsetHeight: canvas.offsetHeight
          });
          canvas.style.display = 'none';
          canvas.setAttribute('data-screenshot-hidden', 'true');
          problematicElements.push(canvas);
        });
        
        // Also remove other problematic elements that can cause issues
        const problematicSelectors = [
          'svg[width="0"]',
          'svg[height="0"]', 
          'img[width="0"]',
          'img[height="0"]',
          'iframe[width="0"]',
          'iframe[height="0"]',
          'video[width="0"]',
          'video[height="0"]'
        ];
        
        problematicSelectors.forEach(selector => {
          try {
            const elements = document.querySelectorAll(selector);
            elements.forEach(element => {
              console.log('📸 Hiding problematic element:', selector);
              element.style.display = 'none';
              element.setAttribute('data-screenshot-hidden', 'true');
              problematicElements.push(element);
            });
          } catch (e) {
            // Ignore selector errors
          }
        });
        
        // Final pass - hide anything with zero dimensions
        const allElements = document.querySelectorAll('*');
        allElements.forEach(element => {
          if (element.offsetWidth === 0 || element.offsetHeight === 0) {
            if (['CANVAS', 'SVG', 'IMG', 'VIDEO', 'IFRAME'].includes(element.tagName)) {
              if (!element.hasAttribute('data-screenshot-hidden')) {
                console.log('📸 Final pass - hiding zero element:', element.tagName);
                element.style.display = 'none';
                element.setAttribute('data-screenshot-hidden', 'true');
                problematicElements.push(element);
              }
            }
          }
        });
        
        console.log('📸 Hidden ' + problematicElements.length + ' problematic elements');
        
        // ULTRA-AGGRESSIVE ignoreElements function 
        captureOptions.ignoreElements = function(element) {
          // Ignore ALL canvas elements completely - no exceptions
          if (element.tagName === 'CANVAS') {
            return true;
          }
          
          // Ignore ALL iframe elements - they can cause issues
          if (element.tagName === 'IFRAME') {
            return true;
          }
          
          // Ignore any element with zero dimensions
          if (element.offsetWidth === 0 || element.offsetHeight === 0 ||
              element.clientWidth === 0 || element.clientHeight === 0) {
            return true;
          }
          
          // Ignore hidden elements
          if (element.style.display === 'none' ||
              element.style.visibility === 'hidden' ||
              element.hasAttribute('data-screenshot-hidden')) {
            return true;
          }
          
          // Ignore SVG elements with problematic dimensions
          if (element.tagName === 'SVG') {
            const width = element.getAttribute('width');
            const height = element.getAttribute('height');
            if (width === '0' || height === '0' || 
                element.offsetWidth === 0 || element.offsetHeight === 0) {
              return true;
            }
          }
          
          // Ignore video elements that might be problematic
          if (element.tagName === 'VIDEO' && 
              (element.offsetWidth === 0 || element.offsetHeight === 0)) {
            return true;
          }
          
          return false;
        };
        
        // Try html2canvas with proper error handling
        console.log('📸 Starting html2canvas with target:', targetElement.tagName);
        
        html2canvas(targetElement, captureOptions).then(function(canvas) {
          // Restore hidden elements
          problematicElements.forEach(element => {
            element.style.display = '';
            element.removeAttribute('data-screenshot-hidden');
          });
          console.log('📸 Canvas capture completed successfully');
          let finalCanvas = canvas;
          
          // Handle coordinate-based cropping
          if (captureParams.width > 0 && captureParams.height > 0) {
            const cropCanvas = document.createElement('canvas');
            const cropCtx = cropCanvas.getContext('2d');
            
            cropCanvas.width = captureParams.width;
            cropCanvas.height = captureParams.height;
            
            cropCtx.drawImage(
              canvas,
              captureParams.x, captureParams.y, captureParams.width, captureParams.height,
              0, 0, captureParams.width, captureParams.height
            );
            
            finalCanvas = cropCanvas;
            filename = 'continuum-cropped-' + captureParams.x + '-' + captureParams.y + '-' + captureParams.width + 'x' + captureParams.height + '-' + timestamp + '.' + captureParams.format;
          }
          
          // Handle resolution scaling
          if (captureParams.resolutionWidth > 0 && captureParams.resolutionHeight > 0) {
            const scaleCanvas = document.createElement('canvas');
            const scaleCtx = scaleCanvas.getContext('2d');
            
            scaleCanvas.width = captureParams.resolutionWidth;
            scaleCanvas.height = captureParams.resolutionHeight;
            
            scaleCtx.imageSmoothingEnabled = captureParams.resolutionWidth < finalCanvas.width;
            scaleCtx.imageSmoothingQuality = 'high';
            
            scaleCtx.drawImage(
              finalCanvas,
              0, 0, finalCanvas.width, finalCanvas.height,
              0, 0, captureParams.resolutionWidth, captureParams.resolutionHeight
            );
            
            finalCanvas = scaleCanvas;
            filename = 'continuum-scaled-' + captureParams.resolutionWidth + 'x' + captureParams.resolutionHeight + '-' + timestamp + '.' + captureParams.format;
          }
          
          // Create download
          const link = document.createElement('a');
          link.download = filename;
          
          let mimeType, dataURL;
          switch(captureParams.format) {
            case 'jpg':
            case 'jpeg':
              mimeType = 'image/jpeg';
              dataURL = finalCanvas.toDataURL(mimeType, captureParams.quality);
              break;
            case 'webp':
              mimeType = 'image/webp';
              dataURL = finalCanvas.toDataURL(mimeType, captureParams.quality);
              break;
            default:
              mimeType = 'image/png';
              dataURL = finalCanvas.toDataURL(mimeType);
          }
          
          // Send screenshot data via WebSocket instead of download
          console.log('📸 Sending screenshot data via WebSocket...');
          
          if (typeof WebSocket !== 'undefined' && window.ws && window.ws.readyState === WebSocket.OPEN) {
            // Send both for debug interface and command response
            window.ws.send(JSON.stringify({
              type: 'screenshot_data',
              dataURL: dataURL,
              filename: filename,
              timestamp: timestamp
            }));
            
            window.ws.send(JSON.stringify({
              type: 'screenshot_captured',
              data: {
                filename: filename,
                dataURL: dataURL,
                timestamp: timestamp,
                parameters: captureParams
              }
            }));
            
            console.log('✅ Screenshot data sent via WebSocket');
          } else {
            console.log('⚠️ WebSocket not available, creating download fallback');
            // Fallback to download if WebSocket not available
            link.href = dataURL;
            document.body.appendChild(link);
            link.click();
            document.body.removeChild(link);
          }
          
          console.log('📸 Screenshot captured:', filename);
          
        }).catch(function(error) {
          // Restore hidden elements even on error
          problematicElements.forEach(element => {
            element.style.display = '';
            element.removeAttribute('data-screenshot-hidden');
          });
          
          console.error('📸 Screenshot failed:', error);
          
          // Provide meaningful error analysis
          let errorDiagnosis = 'Unknown screenshot error';
          let errorSolution = 'Check browser console for details';
          
          if (error.message.includes('createPattern')) {
            errorDiagnosis = 'createPattern error - zero-dimension canvas elements detected';
            errorSolution = 'Remove or hide all canvas elements with width/height of 0';
          } else if (error.message.includes('tainted')) {
            errorDiagnosis = 'Canvas tainted by cross-origin content';
            errorSolution = 'Enable CORS or use allowTaint option';
          } else if (error.message.includes('SecurityError')) {
            errorDiagnosis = 'Security error - cross-origin restrictions';
            errorSolution = 'Check CORS headers and domain restrictions';
          }
          
          console.error('📊 Error diagnosis:', errorDiagnosis);
          console.error('💡 Suggested solution:', errorSolution);
          
          // Send meaningful error via WebSocket
          if (typeof WebSocket !== 'undefined' && window.ws && window.ws.readyState === WebSocket.OPEN) {
            window.ws.send(JSON.stringify({
              type: 'screenshot_error',
              error: error.message,
              diagnosis: errorDiagnosis,
              solution: errorSolution,
              timestamp: Date.now()
            }));
          }
          
          console.log('📸 Attempting fallback screenshot without problematic elements...');
          
          // Try a more aggressive fallback approach
          try {
            // Hide ALL canvas elements for fallback
            const allCanvases = document.querySelectorAll('canvas');
            const fallbackHidden = [];
            allCanvases.forEach(canvas => {
              canvas.style.display = 'none';
              canvas.setAttribute('data-fallback-hidden', 'true');
              fallbackHidden.push(canvas);
            });
            
            console.log('📸 Fallback: Hidden ' + fallbackHidden.length + ' canvas elements');
            
            const fallbackOptions = {
              allowTaint: true,
              useCORS: true,
              scale: 0.3,
              backgroundColor: '#1a1a1a',
              ignoreElements: function(element) {
                // Be very aggressive about filtering problematic elements
                if (element.tagName === 'CANVAS' || 
                    element.tagName === 'SVG' ||
                    element.style.display === 'none' ||
                    element.style.visibility === 'hidden' ||
                    element.offsetWidth === 0 || 
                    element.offsetHeight === 0 ||
                    element.clientWidth === 0 ||
                    element.clientHeight === 0 ||
                    element.hasAttribute('data-screenshot-hidden') ||
                    element.hasAttribute('data-fallback-hidden')) {
                  return true;
                }
                
                // Skip any element that might cause createPattern errors
                if (element.width === 0 || element.height === 0) {
                  return true;
                }
                
                return false;
              }
            };
            
            html2canvas(document.body, fallbackOptions).then(function(fallbackCanvas) {
              // Restore fallback hidden elements
              fallbackHidden.forEach(element => {
                element.style.display = '';
                element.removeAttribute('data-fallback-hidden');
              });
              
              const fallbackDataURL = fallbackCanvas.toDataURL('image/png');
              const fallbackFilename = 'continuum-fallback-screenshot-' + Date.now() + '.png';
              
              if (typeof WebSocket !== 'undefined' && window.ws && window.ws.readyState === WebSocket.OPEN) {
                window.ws.send(JSON.stringify({
                  type: 'screenshot_data',
                  dataURL: fallbackDataURL,
                  filename: fallbackFilename,
                  timestamp: Date.now(),
                  fallback: true
                }));
                
                console.log('✅ Fallback screenshot captured and sent');
              }
            }).catch(function(fallbackError) {
              // Restore fallback hidden elements even on error
              fallbackHidden.forEach(element => {
                element.style.display = '';
                element.removeAttribute('data-fallback-hidden');
              });
              console.error('📸 Fallback screenshot also failed:', fallbackError);
            });
          } catch (fallbackError) {
            console.error('📸 Could not attempt fallback screenshot:', fallbackError);
          }
        });
      }
    `;
  }
}

module.exports = ScreenshotCommand;