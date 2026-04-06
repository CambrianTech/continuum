/**
 * Continuum Tray — system tray app for grid management
 *
 * Shows a circle icon with colored center dot:
 *   Green  = all healthy
 *   Yellow = degraded (some nodes offline)
 *   Red    = local stack down
 *   Gray   = checking...
 *
 * Menu items wrap `continuum` CLI commands.
 */

const { app, Tray, Menu, nativeImage, shell, nativeTheme } = require('electron');
const { execSync, exec } = require('child_process');
const path = require('path');

// Don't show in dock — tray only
app.dock?.hide();

let tray = null;
let healthInterval = null;

const CONTINUUM_BIN = path.resolve(__dirname, '..', 'continuum');
const POLL_INTERVAL = 30_000; // 30s health poll

// ── Icon Generation ──────────────────────────────────────────
// Generate tray icons programmatically — circle with colored center dot
// macOS uses "template" images (black/white) but we need color for the dot

function createTrayIcon(dotColor) {
  const size = 22; // macOS menu bar standard
  const scale = 2; // @2x retina
  const s = size * scale;
  const canvas = `
    <svg xmlns="http://www.w3.org/2000/svg" width="${s}" height="${s}" viewBox="0 0 ${s} ${s}">
      <!-- Outer ring -->
      <circle cx="${s/2}" cy="${s/2}" r="${s/2 - 2}" fill="none" stroke="${nativeTheme.shouldUseDarkColors ? '#ffffff' : '#000000'}" stroke-width="2.5" opacity="0.8"/>
      <!-- Center status dot -->
      <circle cx="${s/2}" cy="${s/2}" r="${s/6}" fill="${dotColor}"/>
    </svg>
  `.trim();

  const img = nativeImage.createFromBuffer(
    Buffer.from(canvas),
    { width: size, height: size, scaleFactor: scale }
  );
  return img;
}

// ── CLI Helpers ──────────────────────────────────────────────

function runCommand(cmd, callback) {
  exec(`"${CONTINUUM_BIN}" ${cmd}`, { timeout: 15_000 }, (err, stdout, stderr) => {
    callback(err, (stdout || '').trim(), (stderr || '').trim());
  });
}

function runCommandSync(cmd) {
  try {
    return execSync(`"${CONTINUUM_BIN}" ${cmd}`, { timeout: 10_000 }).toString().trim();
  } catch (e) {
    return '';
  }
}

function openBrowser(url) {
  shell.openExternal(url);
}

// ── Health Check ─────────────────────────────────────────────

function checkHealth() {
  // Check local Docker first
  try {
    const dockerStatus = execSync('docker info', { timeout: 5000, stdio: 'pipe' }).toString();
    if (!dockerStatus) {
      updateStatus('red', 'Docker not running');
      return;
    }
  } catch {
    updateStatus('red', 'Docker not running');
    return;
  }

  // Check local containers
  runCommand('status', (err, stdout) => {
    if (err || !stdout) {
      updateStatus('yellow', 'Stack not running');
      return;
    }

    const healthyCount = (stdout.match(/healthy/g) || []).length;
    const unhealthyCount = (stdout.match(/unhealthy/g) || []).length;

    if (unhealthyCount > 0) {
      updateStatus('yellow', `${unhealthyCount} unhealthy`);
    } else if (healthyCount >= 4) {
      updateStatus('green', `${healthyCount} services healthy`);
    } else if (healthyCount > 0) {
      updateStatus('yellow', `${healthyCount} services`);
    } else {
      updateStatus('red', 'Not running');
    }
  });
}

function updateStatus(color, tooltip) {
  if (!tray) return;

  const colors = {
    green: '#00ff88',
    yellow: '#ffcc00',
    red: '#ff4444',
    gray: '#888888',
  };

  tray.setImage(createTrayIcon(colors[color] || colors.gray));
  tray.setToolTip(`Continuum — ${tooltip}`);
  rebuildMenu(color, tooltip);
}

// ── Menu ─────────────────────────────────────────────────────

function rebuildMenu(status, statusText) {
  // Get grid nodes for the menu
  let gridNodes = [];
  try {
    const output = runCommandSync('health');
    const lines = output.split('\n').filter(l => l.includes('●'));
    gridNodes = lines.map(line => {
      const match = line.match(/●\s+(\S+)\s+(\S+)\s+(.*)/);
      if (match) return { name: match[1], ip: match[2], detail: match[3].trim() };
      return null;
    }).filter(Boolean);
  } catch { /* no grid */ }

  const statusIcon = status === 'green' ? '🟢' : status === 'yellow' ? '🟡' : '🔴';

  const menuTemplate = [
    { label: `${statusIcon}  ${statusText}`, enabled: false },
    { type: 'separator' },

    // Quick actions
    { label: 'Open UI', click: () => runCommand('open', () => {}) },
    {
      label: 'Grid Nodes',
      submenu: gridNodes.length > 0
        ? gridNodes.map(n => ({
            label: `${n.detail.includes('UI OK') ? '🟢' : '⚪'}  ${n.name}`,
            sublabel: n.ip,
            click: () => {
              if (n.name.includes('-grid')) {
                // Try to open the grid node's UI
                const suffix = runCommandSync('health').match(/\S+\.ts\.net/)?.[0]?.replace(/^\S+\./, '') || '';
                if (suffix) openBrowser(`https://${n.name}.${suffix}`);
              }
            }
          }))
        : [{ label: 'No grid nodes', enabled: false }]
    },
    { type: 'separator' },

    // Management
    { label: 'Start', click: () => runCommand('start', () => checkHealth()) },
    { label: 'Stop', click: () => runCommand('stop', () => checkHealth()) },
    { label: 'Restart', click: () => runCommand('restart', () => checkHealth()) },
    { type: 'separator' },

    // Diagnostics
    {
      label: 'Logs',
      submenu: [
        { label: 'All Services', click: () => exec(`open -a Terminal "${CONTINUUM_BIN} logs"`) },
        { label: 'Node Server', click: () => exec(`open -a Terminal "${CONTINUUM_BIN} logs node-server"`) },
        { label: 'Continuum Core', click: () => exec(`open -a Terminal "${CONTINUUM_BIN} logs continuum-core"`) },
      ]
    },
    { label: 'Doctor', click: () => exec(`open -a Terminal "${CONTINUUM_BIN} doctor"`) },
    { label: 'Update', click: () => exec(`open -a Terminal "${CONTINUUM_BIN} update"`) },
    { type: 'separator' },

    { label: 'Refresh', click: () => checkHealth() },
    { label: 'Quit', click: () => app.quit() },
  ];

  const contextMenu = Menu.buildFromTemplate(menuTemplate);
  tray.setContextMenu(contextMenu);
}

// ── App Lifecycle ────────────────────────────────────────────

app.whenReady().then(() => {
  // Create tray with gray icon (checking...)
  tray = new Tray(createTrayIcon('#888888'));
  tray.setToolTip('Continuum — checking...');

  // Initial health check
  checkHealth();

  // Poll every 30s
  healthInterval = setInterval(checkHealth, POLL_INTERVAL);

  // Click tray icon → open UI
  tray.on('click', () => {
    runCommand('open', () => {});
  });
});

app.on('before-quit', () => {
  if (healthInterval) clearInterval(healthInterval);
});

// Prevent app from quitting when all windows closed (tray-only app)
app.on('window-all-closed', (e) => e.preventDefault());
