/**
 * Screenshot Agent
 * Generic screenshot capabilities as an AI agent in the continuum
 */

class ScreenshotAgent {
  constructor(continuum) {
    this.continuum = continuum;
    this.name = 'ScreenshotAgent';
    this.capabilities = [
      'screenshot',
      'screen_capture', 
      'visual_analysis',
      'ui_inspection',
      'coordinate_capture'
    ];
  }

  /**
   * Handle screenshot requests like any other AI agent
   */
  async processRequest(request) {
    try {
      // Parse the request for screenshot parameters
      const params = this.parseScreenshotRequest(request);
      
      // Generate the JavaScript for browser execution
      const screenshotJS = this.generateScreenshotJS(params);
      
      // Send to browsers via WebSocket
      if (this.continuum.webSocketServer) {
        this.continuum.webSocketServer.broadcast({
          type: 'execute_js',
          data: {
            command: screenshotJS,
            timestamp: new Date().toISOString()
          }
        });
      }
      
      return {
        success: true,
        message: `Screenshot requested with params: ${JSON.stringify(params)}`,
        action: 'screenshot_initiated',
        parameters: params,
        timestamp: new Date().toISOString()
      };
      
    } catch (error) {
      return {
        success: false,
        error: error.message,
        timestamp: new Date().toISOString()
      };
    }
  }

  /**
   * Parse natural language or structured requests into screenshot parameters
   */
  parseScreenshotRequest(request) {
    const params = {
      x: 0,
      y: 0,
      width: 0,
      height: 0,
      selector: '',
      scale: 1.0,
      resolutionWidth: 0,
      resolutionHeight: 0,
      quality: 0.92,
      format: 'png'
    };

    // Extract coordinates if specified
    const coordMatch = request.match(/(\d+),\s*(\d+),\s*(\d+),\s*(\d+)/);
    if (coordMatch) {
      params.x = parseInt(coordMatch[1]);
      params.y = parseInt(coordMatch[2]);
      params.width = parseInt(coordMatch[3]);
      params.height = parseInt(coordMatch[4]);
    }

    // Extract CSS selector
    const selectorMatch = request.match(/selector[:\s]+['"]([^'"]+)['"]/i) || 
                         request.match(/element[:\s]+['"]([^'"]+)['"]/i) ||
                         request.match(/capture[:\s]+['"]([^'"]+)['"]/i);
    if (selectorMatch) {
      params.selector = selectorMatch[1];
    }

    // Extract format
    const formatMatch = request.match(/format[:\s]+(png|jpeg|jpg|webp)/i);
    if (formatMatch) {
      params.format = formatMatch[1].toLowerCase();
    }

    // Extract resolution
    const resMatch = request.match(/(\d+)x(\d+)/);
    if (resMatch) {
      params.resolutionWidth = parseInt(resMatch[1]);
      params.resolutionHeight = parseInt(resMatch[2]);
    }

    // Extract quality
    const qualityMatch = request.match(/quality[:\s]+(0\.\d+|\d+)/i);
    if (qualityMatch) {
      params.quality = parseFloat(qualityMatch[1]);
      if (params.quality > 1) params.quality = params.quality / 100;
    }

    return params;
  }

  /**
   * Generate browser JavaScript for screenshot capture
   */
  generateScreenshotJS(params) {
    return `
      console.log('📸 ScreenshotAgent: Starting capture with params:', ${JSON.stringify(params)});
      
      // Load html2canvas if needed
      if (typeof html2canvas === 'undefined') {
        const script = document.createElement('script');
        script.src = 'https://cdnjs.cloudflare.com/ajax/libs/html2canvas/1.4.1/html2canvas.min.js';
        script.onload = function() {
          executeScreenshot();
        };
        document.head.appendChild(script);
      } else {
        executeScreenshot();
      }
      
      function executeScreenshot() {
        const timestamp = Date.now();
        let filename = 'screenshot-' + timestamp;
        let targetElement = document.body;
        
        // Handle selector targeting
        if ('${params.selector}') {
          const element = document.querySelector('${params.selector}');
          if (element) {
            targetElement = element;
            filename = 'element-' + '${params.selector}'.replace(/[^a-zA-Z0-9]/g, '-') + '-' + timestamp;
          }
        }
        
        // Capture with html2canvas
        html2canvas(targetElement, {
          allowTaint: true,
          useCORS: true,
          scale: ${params.scale},
          backgroundColor: '#1a1a1a'
        }).then(function(canvas) {
          
          let finalCanvas = canvas;
          
          // Handle cropping
          if (${params.width} > 0 && ${params.height} > 0) {
            const cropCanvas = document.createElement('canvas');
            const cropCtx = cropCanvas.getContext('2d');
            cropCanvas.width = ${params.width};
            cropCanvas.height = ${params.height};
            
            cropCtx.drawImage(
              canvas,
              ${params.x}, ${params.y}, ${params.width}, ${params.height},
              0, 0, ${params.width}, ${params.height}
            );
            
            finalCanvas = cropCanvas;
            filename = 'cropped-' + ${params.x} + '-' + ${params.y} + '-' + ${params.width} + 'x' + ${params.height} + '-' + timestamp;
          }
          
          // Handle resolution scaling
          if (${params.resolutionWidth} > 0 && ${params.resolutionHeight} > 0) {
            const scaleCanvas = document.createElement('canvas');
            const scaleCtx = scaleCanvas.getContext('2d');
            scaleCanvas.width = ${params.resolutionWidth};
            scaleCanvas.height = ${params.resolutionHeight};
            
            scaleCtx.imageSmoothingEnabled = ${params.resolutionWidth} < finalCanvas.width;
            if (scaleCtx.imageSmoothingEnabled) {
              scaleCtx.imageSmoothingQuality = 'high';
            }
            
            scaleCtx.drawImage(
              finalCanvas,
              0, 0, finalCanvas.width, finalCanvas.height,
              0, 0, ${params.resolutionWidth}, ${params.resolutionHeight}
            );
            
            finalCanvas = scaleCanvas;
            filename = 'scaled-' + ${params.resolutionWidth} + 'x' + ${params.resolutionHeight} + '-' + timestamp;
          }
          
          // Generate download
          filename += '.${params.format}';
          
          let mimeType, dataURL;
          switch('${params.format}') {
            case 'jpg':
            case 'jpeg':
              mimeType = 'image/jpeg';
              dataURL = finalCanvas.toDataURL(mimeType, ${params.quality});
              break;
            case 'webp':
              mimeType = 'image/webp';
              dataURL = finalCanvas.toDataURL(mimeType, ${params.quality});
              break;
            default:
              mimeType = 'image/png';
              dataURL = finalCanvas.toDataURL(mimeType);
          }
          
          const link = document.createElement('a');
          link.download = filename;
          link.href = dataURL;
          document.body.appendChild(link);
          link.click();
          document.body.removeChild(link);
          
          console.log('📸 ScreenshotAgent: Download triggered -', filename);
          
          // Show preview
          const preview = finalCanvas.cloneNode();
          preview.getContext('2d').drawImage(finalCanvas, 0, 0);
          preview.style.cssText = 'position:fixed;top:10px;right:10px;max-width:200px;max-height:200px;border:2px solid #00ff88;z-index:9999;';
          preview.title = 'Screenshot: ' + filename;
          document.body.appendChild(preview);
          
          setTimeout(() => {
            if (preview.parentNode) {
              preview.parentNode.removeChild(preview);
            }
          }, 3000);
          
        }).catch(function(error) {
          console.error('📸 ScreenshotAgent failed:', error);
        });
      }
    `;
  }

  /**
   * Get agent info for discovery
   */
  getInfo() {
    return {
      name: this.name,
      type: 'service_agent',
      capabilities: this.capabilities,
      description: 'Captures screenshots of web interface with coordinate and selector support',
      examples: [
        'take a screenshot',
        'capture element .sidebar',
        'screenshot 100,200,800,600',
        'capture at 800x600 resolution',
        'screenshot selector ".chat-area" format jpeg quality 0.8'
      ]
    };
  }
}

module.exports = ScreenshotAgent;