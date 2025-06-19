/**
 * Opera Browser Launcher
 * Handles launching Opera with DevTools enabled
 */

const { spawn } = require('child_process');
const fs = require('fs');

class OperaLauncher {
  constructor() {
    this.operaPaths = [
      '/Applications/Opera GX.app/Contents/MacOS/Opera',
      '/Applications/Opera.app/Contents/MacOS/Opera'
    ];
  }

  findOperaPath() {
    for (const path of this.operaPaths) {
      if (fs.existsSync(path)) {
        return path;
      }
    }
    return null;
  }

  launch(port = 9222) {
    const operaPath = this.findOperaPath();
    
    if (!operaPath) {
      throw new Error('Opera not found in standard locations');
    }

    const opera = spawn(operaPath, [
      `--remote-debugging-port=${port}`,
      '--disable-web-security',
      '--user-data-dir=/tmp/opera-devtools',
      'http://localhost:9000'
    ], {
      detached: true,
      stdio: 'ignore'
    });

    opera.unref();
    
    return {
      pid: opera.pid,
      port: port,
      browser: 'opera'
    };
  }
}

module.exports = OperaLauncher;