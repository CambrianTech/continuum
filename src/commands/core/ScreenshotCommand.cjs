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
      
      // Load html2canvas if not already loaded
      if (typeof html2canvas === 'undefined') {
        const script = document.createElement('script');
        script.src = 'https://cdnjs.cloudflare.com/ajax/libs/html2canvas/1.4.1/html2canvas.min.js';
        script.onload = function() {
          console.log('📸 html2canvas loaded, starting capture...');
          captureScreenshot();
        };
        document.head.appendChild(script);
      } else {
        captureScreenshot();
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
        
        // Pre-filter problematic elements to prevent canvas errors
        console.log('📸 Pre-filtering zero-dimension elements...');
        const problematicElements = [];
        const canvasElements = document.querySelectorAll('canvas');
        
        canvasElements.forEach(canvas => {
          if (canvas.width === 0 || canvas.height === 0) {
            console.log('📸 Hiding zero-dimension canvas temporarily:', canvas);
            canvas.style.display = 'none';
            canvas.setAttribute('data-screenshot-hidden', 'true');
            problematicElements.push(canvas);
          }
        });
        
        // Add ignoreElements function to capture options
        captureOptions.ignoreElements = function(element) {
          // Skip elements that might cause issues
          if (element.tagName === 'CANVAS' && (element.width === 0 || element.height === 0)) {
            console.log('📸 Ignoring zero-dimension canvas:', element);
            return true;
          }
          return false;
        };
        
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
          console.log('📸 Attempting fallback screenshot without problematic elements...');
          
          // Try a simpler fallback approach
          try {
            const fallbackOptions = {
              allowTaint: true,
              useCORS: true,
              scale: 0.5,
              ignoreElements: function(element) {
                return element.tagName === 'CANVAS' || 
                       element.style.display === 'none' ||
                       element.offsetWidth === 0 || 
                       element.offsetHeight === 0;
              }
            };
            
            html2canvas(document.body, fallbackOptions).then(function(fallbackCanvas) {
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