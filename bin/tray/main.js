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
// Circle ring with number inside. Color = status. Number = node count.
// Uses SVG data URL → nativeImage (Electron supports this natively).

function createTrayIcon(statusColor, nodeCount) {
  const s = 44; // 22pt @2x retina
  const ringColor = nativeTheme.shouldUseDarkColors ? 'rgba(255,255,255,0.8)' : 'rgba(0,0,0,0.8)';
  const num = (typeof nodeCount === 'number' && nodeCount > 0) ? String(nodeCount) : '';
  // Thin font weight for the number, slightly smaller if 2+ digits
  const fontSize = num.length > 1 ? 18 : 20;

  const svg = `<svg xmlns="http://www.w3.org/2000/svg" width="${s}" height="${s}" viewBox="0 0 ${s} ${s}">
    <circle cx="${s/2}" cy="${s/2}" r="${s/2 - 3}" fill="none" stroke="${ringColor}" stroke-width="2.5"/>
    ${num
      ? `<text x="${s/2}" y="${s/2}" text-anchor="middle" dominant-baseline="central"
           font-family="-apple-system, Helvetica Neue, sans-serif" font-size="${fontSize}"
           font-weight="300" fill="${statusColor}">${num}</text>`
      : `<circle cx="${s/2}" cy="${s/2}" r="5" fill="${statusColor}"/>`
    }
  </svg>`;

  const dataUrl = `data:image/svg+xml;base64,${Buffer.from(svg).toString('base64')}`;
  const img = nativeImage.createFromDataURL(dataUrl);
  return img.resize({ width: 22, height: 22 });
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
  // Count grid nodes first (works even without local Docker)
  let onlineNodes = 0;
  try {
    const healthOutput = runCommandSync('health');
    onlineNodes = (healthOutput.match(/●/g) || []).length;
  } catch { /* no grid */ }

  // Check local Docker
  try {
    execSync('docker info', { timeout: 5000, stdio: 'pipe' });
  } catch {
    // Docker not running — but grid nodes might be
    if (onlineNodes > 0) {
      updateStatus('yellow', `Docker off, ${onlineNodes} grid nodes`, onlineNodes);
    } else {
      updateStatus('red', 'Docker not running', 0);
    }
    return;
  }

  // Check local containers
  runCommand('status', (err, stdout) => {
    if (err || !stdout) {
      if (onlineNodes > 0) {
        updateStatus('yellow', `Local off, ${onlineNodes} grid nodes`, onlineNodes);
      } else {
        updateStatus('yellow', 'Stack not running', 0);
      }
      return;
    }

    const healthyCount = (stdout.match(/healthy/g) || []).length;
    const unhealthyCount = (stdout.match(/unhealthy/g) || []).length;

    if (unhealthyCount > 0) {
      updateStatus('yellow', `${unhealthyCount} unhealthy, ${onlineNodes} nodes`, onlineNodes);
    } else if (healthyCount >= 4) {
      updateStatus('green', `${healthyCount} services, ${onlineNodes} nodes`, onlineNodes);
    } else if (healthyCount > 0) {
      updateStatus('yellow', `${healthyCount} services, ${onlineNodes} nodes`, onlineNodes);
    } else if (onlineNodes > 0) {
      updateStatus('yellow', `${onlineNodes} grid nodes`, onlineNodes);
    } else {
      updateStatus('red', 'Not running', 0);
    }
  });
}

let lastNodeCount = 0;

function updateStatus(color, tooltip, nodeCount) {
  if (!tray) return;
  lastNodeCount = nodeCount || 0;

  const colors = {
    green: '#00ff88',
    yellow: '#ffcc00',
    red: '#ff4444',
    gray: '#888888',
  };

  tray.setImage(createTrayIcon(colors[color] || colors.gray, lastNodeCount));
  tray.setToolTip(`Continuum — ${tooltip}`);
  tray.setTitle('');

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
