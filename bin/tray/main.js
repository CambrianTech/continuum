/**
 * continuum Tray — system tray app for grid management
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
// Circle ring with colored number/dot inside.
// Uses offscreen BrowserWindow to render SVG → PNG snapshot.
// Cached per color+count so we don't re-render every poll.

const { BrowserWindow } = require('electron');
const iconCache = new Map();

async function createTrayIconAsync(statusColor) {
  const key = statusColor;
  if (iconCache.has(key)) return iconCache.get(key);

  const s = 44;
  const ringColor = nativeTheme.shouldUseDarkColors ? 'rgba(255,255,255,0.75)' : 'rgba(0,0,0,0.75)';

  // HAL 9000. Ring + glowing center eye. Color = mood.
  const svg = `<svg xmlns="http://www.w3.org/2000/svg" width="${s}" height="${s}" viewBox="0 0 ${s} ${s}">
    <circle cx="${s/2}" cy="${s/2}" r="${s/2 - 3}" fill="none" stroke="${ringColor}" stroke-width="2"/>
    <circle cx="${s/2}" cy="${s/2}" r="6" fill="${statusColor}"/>
    <circle cx="${s/2}" cy="${s/2}" r="9" fill="${statusColor}" opacity="0.2"/>
  </svg>`;

  const html = `<html><body style="margin:0;background:transparent;">${svg}</body></html>`;

  const win = new BrowserWindow({
    width: s, height: s, show: false, frame: false, transparent: true,
    webPreferences: { offscreen: true }
  });

  await win.loadURL(`data:text/html;base64,${Buffer.from(html).toString('base64')}`);
  const img = await win.webContents.capturePage({ x: 0, y: 0, width: s, height: s });
  win.destroy();

  const resized = img.resize({ width: 22, height: 22 });
  iconCache.set(key, resized);
  return resized;
}

// Synchronous fallback for initial icon (before async render completes)
function createFallbackIcon() {
  // 22x22 empty icon — just so the tray doesn't crash on creation
  return nativeImage.createEmpty();
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

async function updateStatus(color, tooltip, nodeCount) {
  if (!tray) return;
  lastNodeCount = nodeCount || 0;

  const colors = {
    green: '#00ff88',
    yellow: '#ffcc00',
    red: '#ff4444',
    gray: '#888888',
  };

  try {
    const icon = await createTrayIconAsync(colors[color] || colors.gray);
    tray.setImage(icon);
  } catch (e) {
    console.error('Icon render failed:', e);
  }
  tray.setToolTip(`continuum — ${tooltip}`);
  tray.setTitle('');

  rebuildMenu(color, tooltip);
}

// ── Menu ─────────────────────────────────────────────────────

// ── Menu Section Registry ────────────────────────────────────
// Pluggable menu system. Each section is a function that returns MenuItemConstructorOptions[].
// Add sections by calling registerMenuSection(name, priority, builder).
// Lower priority = higher in menu. Sections separated by dividers.

const menuSections = new Map();

function registerMenuSection(name, priority, builder) {
  menuSections.set(name, { name, priority, builder });
}

function buildMenuFromSections(context) {
  const sections = [...menuSections.values()]
    .sort((a, b) => a.priority - b.priority);

  const items = [];
  for (const section of sections) {
    try {
      const sectionItems = section.builder(context);
      if (sectionItems && sectionItems.length > 0) {
        if (items.length > 0) items.push({ type: 'separator' });
        items.push(...sectionItems);
      }
    } catch (e) {
      console.error(`Menu section '${section.name}' failed:`, e);
    }
  }
  return items;
}

// Get tailnet suffix for building HTTPS URLs
let cachedTailnet = '';
function getTailnet() {
  if (cachedTailnet) return cachedTailnet;
  try {
    cachedTailnet = runCommandSync('health').match(/(\S+\.ts\.net)/)?.[1]?.replace(/^\S+\./, '') || '';
  } catch {}
  return cachedTailnet;
}

// ── Default Menu Sections ────────────────────────────────────

registerMenuSection('header', 0, (ctx) => [
  { label: 'continuum', enabled: false },
  { label: `${ctx.statusIcon}  ${ctx.statusText}`, enabled: false },
]);

registerMenuSection('grid-nodes', 10, (ctx) => {
  if (ctx.gridNodes.length === 0) return [{ label: '  No grid nodes', enabled: false }];
  return ctx.gridNodes.map(n => {
    const isGrid = n.name.includes('-grid');
    const isOnline = !n.detail.includes('offline');
    const hasUI = n.detail.includes('UI OK');
    const icon = hasUI ? '🟢' : isOnline ? '🟡' : '🔴';
    const url = isGrid && ctx.tailnet ? `https://${n.name}.${ctx.tailnet}` : null;
    return {
      label: `${icon}  ${n.name}`,
      sublabel: url || n.ip,
      enabled: isOnline,
      click: () => {
        if (url) openBrowser(url);
        else if (isGrid) openBrowser(`http://${n.ip}:9003`);
      }
    };
  });
});

registerMenuSection('services', 20, () => [
  { label: 'Start Services', click: () => { exec(`"${CONTINUUM_BIN}" start`); setTimeout(checkHealth, 5000); } },
  { label: 'Stop Services', click: () => { exec(`"${CONTINUUM_BIN}" stop`); setTimeout(checkHealth, 3000); } },
  { label: 'Restart Services', click: () => { exec(`"${CONTINUUM_BIN}" restart`); setTimeout(checkHealth, 5000); } },
]);

registerMenuSection('tools', 30, () => [
  {
    label: 'Logs',
    submenu: [
      { label: 'All Services', click: () => exec(`open -a Terminal.app "${CONTINUUM_BIN}" logs`) },
      { label: 'Node Server', click: () => exec(`open -a Terminal.app "${CONTINUUM_BIN}" logs node-server`) },
      { label: 'Core', click: () => exec(`open -a Terminal.app "${CONTINUUM_BIN}" logs continuum-core`) },
    ]
  },
  {
    label: 'More',
    submenu: [
      { label: 'Doctor', click: () => exec(`open -a Terminal.app "${CONTINUUM_BIN}" doctor`) },
      { label: 'Update', click: () => exec(`open -a Terminal.app "${CONTINUUM_BIN}" update`) },
      { label: 'Provision Config', click: () => exec(`open -a Terminal.app "${CONTINUUM_BIN}" provision`) },
    ]
  },
]);

registerMenuSection('footer', 100, () => [
  { label: 'continuum v1.0', enabled: false },
  { label: 'Quit continuum', accelerator: 'CmdOrCtrl+Q', click: () => app.quit() },
]);

// ── Menu Builder ─────────────────────────────────────────────

function rebuildMenu(status, statusText) {
  // Gather context for section builders
  let gridNodes = [];
  try {
    const output = runCommandSync('health');
    const lines = output.split('\n').filter(l => l.includes('●'));
    gridNodes = lines.map(line => {
      const match = line.match(/●\s+(\S+)\s+(\S+)\s+(.*)/);
      if (match) return { name: match[1], ip: match[2], detail: match[3].trim() };
      return null;
    }).filter(Boolean);
  } catch {}

  const ctx = {
    status,
    statusText,
    statusIcon: status === 'green' ? '🟢' : status === 'yellow' ? '🟡' : '🔴',
    gridNodes,
    tailnet: getTailnet(),
    nodeCount: lastNodeCount,
  };

  // Build menu from registered sections
  const items = buildMenuFromSections(ctx);
  const contextMenu = Menu.buildFromTemplate(items);
  tray.setContextMenu(contextMenu);
}

// ── App Lifecycle ────────────────────────────────────────────

app.whenReady().then(() => {
  // Create tray with empty icon (async render will fill it)
  tray = new Tray(createFallbackIcon());
  tray.setToolTip('continuum — checking...');
  // Render initial gray eye
  createTrayIconAsync('#888888').then(icon => tray.setImage(icon)).catch(() => {});

  // Initial health check
  checkHealth();

  // Poll every 30s
  healthInterval = setInterval(checkHealth, POLL_INTERVAL);

  // Click tray icon → show context menu (same as right-click)
  tray.on('click', () => {
    tray.popUpContextMenu();
  });
});

app.on('before-quit', () => {
  if (healthInterval) clearInterval(healthInterval);
});

// Prevent app from quitting when all windows closed (tray-only app)
app.on('window-all-closed', (e) => e.preventDefault());
