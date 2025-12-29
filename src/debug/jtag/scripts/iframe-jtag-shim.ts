/**
 * Injectable JTAG Shim for Proxied Iframes - Screenshot Focused
 *
 * Injected into proxied pages to capture screenshots from INSIDE the iframe
 * where all resources are same-origin (no CORS issues).
 *
 * Communication: postMessage with parent window
 * - Request: { type: 'jtag-shim-request', command: 'screenshot', params, requestId }
 * - Response: { type: 'jtag-shim-response', requestId, result: ScreenshotResult-like }
 */

// This generates the inline JavaScript string to inject
export function generateJtagShimScript(): string {
  return `
(function() {
  var JTAG_SHIM_VERSION = '1.0.0';
  var HTML2CANVAS_CDN = 'https://cdnjs.cloudflare.com/ajax/libs/html2canvas/1.4.1/html2canvas.min.js';
  var html2canvasLoaded = false;
  var html2canvasLoading = false;

  function loadHtml2Canvas() {
    return new Promise(function(resolve, reject) {
      if (html2canvasLoaded) { resolve(); return; }
      if (html2canvasLoading) {
        var check = setInterval(function() {
          if (html2canvasLoaded) { clearInterval(check); resolve(); }
        }, 100);
        return;
      }
      html2canvasLoading = true;
      var script = document.createElement('script');
      script.src = HTML2CANVAS_CDN;
      script.onload = function() {
        html2canvasLoaded = true;
        html2canvasLoading = false;
        console.log('[JTAG Shim] html2canvas loaded');
        resolve();
      };
      script.onerror = function() {
        html2canvasLoading = false;
        reject(new Error('Failed to load html2canvas'));
      };
      document.head.appendChild(script);
    });
  }

  function captureScreenshot(params) {
    params = params || {};
    return loadHtml2Canvas().then(function() {
      var h2c = window.html2canvas;
      if (!h2c) throw new Error('html2canvas not available');

      var target = params.selector ? document.querySelector(params.selector) : document.body;
      if (!target) throw new Error('Element not found: ' + params.selector);

      var scale = params.scale || 1;
      var devicePixelRatio = window.devicePixelRatio || 1;

      return h2c(target, {
        scale: scale / devicePixelRatio,
        useCORS: true,
        allowTaint: true,
        logging: false,
        backgroundColor: params.backgroundColor || '#ffffff',
        foreignObjectRendering: true
      });
    }).then(function(canvas) {
      var format = params.format || 'png';
      var quality = params.quality || 0.9;
      var dataUrl = format === 'png'
        ? canvas.toDataURL('image/png')
        : canvas.toDataURL('image/' + format, quality);

      return {
        success: true,
        dataUrl: dataUrl,
        metadata: {
          width: canvas.width,
          height: canvas.height,
          format: format,
          quality: quality,
          selector: params.selector || 'body',
          captureTime: Date.now()
        }
      };
    }).catch(function(error) {
      return {
        success: false,
        error: { message: error.message || String(error) }
      };
    });
  }

  function handleMessage(event) {
    if (event.source !== window.parent) return;
    var data = event.data || {};
    if (data.type !== 'jtag-shim-request') return;

    var command = data.command;
    var params = data.params;
    var requestId = data.requestId;

    console.log('[JTAG Shim] Command:', command, params);

    var resultPromise;
    if (command === 'ping') {
      resultPromise = Promise.resolve({ success: true, version: JTAG_SHIM_VERSION });
    } else if (command === 'screenshot') {
      resultPromise = captureScreenshot(params);
    } else if (command === 'pageInfo') {
      resultPromise = Promise.resolve({
        success: true,
        url: window.location.href,
        title: document.title,
        scrollHeight: document.documentElement.scrollHeight,
        viewportHeight: window.innerHeight,
        viewportWidth: window.innerWidth
      });
    } else {
      resultPromise = Promise.resolve({ success: false, error: { message: 'Unknown command: ' + command } });
    }

    resultPromise.then(function(result) {
      window.parent.postMessage({
        type: 'jtag-shim-response',
        requestId: requestId,
        result: result
      }, '*');
    });
  }

  window.addEventListener('message', handleMessage);

  // Announce ready
  window.parent.postMessage({
    type: 'jtag-shim-ready',
    version: JTAG_SHIM_VERSION,
    url: window.location.href
  }, '*');

  console.log('[JTAG Shim] Initialized v' + JTAG_SHIM_VERSION);
})();
`;
}
