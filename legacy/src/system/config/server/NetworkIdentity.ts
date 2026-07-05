/**
 * NetworkIdentity — Single source of truth for this machine's network identity and TLS.
 *
 * Discovers the machine's mesh hostname and TLS certs from whatever
 * mesh network is active (Tailscale today, Reticulum later, anything).
 *
 * TLS certs live in ~/.continuum/ as <hostname>.crt and <hostname>.key.
 * The hostname is discovered from the mesh, not hardcoded.
 */

import { execSync } from 'child_process';
import * as fs from 'fs';
import * as path from 'path';
import * as os from 'os';

export interface NetworkIdentity {
  /** Mesh DNS name (e.g., "node-name.your-tailnet.ts.net") */
  hostname: string;
  /** Path to TLS cert file */
  certPath: string;
  /** Path to TLS key file */
  keyPath: string;
  /** Which mesh provider discovered the identity */
  provider: 'tailscale' | 'reticulum' | 'manual';
}

const CONTINUUM_DIR = path.join(os.homedir(), '.continuum');

let _cached: NetworkIdentity | null | undefined;

/**
 * Get this machine's network identity, if available and TLS-ready.
 * Tries mesh providers in order: Tailscale, then falls back to manual cert scan.
 * Returns null if no TLS certs are found.
 * Result is cached for the lifetime of the process.
 */
export function getNetworkIdentity(): NetworkIdentity | null {
  if (_cached !== undefined) return _cached;

  // Docker containers: TLS is handled by Caddy (livekit-tls), not by the app.
  // The mounted ~/.continuum has certs from the host, but using them inside
  // Docker causes protocol mismatches (WSS server vs WS browser on localhost).
  if (process.env.JTAG_NO_TLS) {
    _cached = null;
    return _cached;
  }

  _cached = discoverFromTailscale() ?? discoverFromCerts();
  return _cached;
}

/** Ask Tailscale for our DNS name, verify certs exist */
function discoverFromTailscale(): NetworkIdentity | null {
  try {
    const output = execSync('tailscale status --json', { timeout: 3000, encoding: 'utf-8' });
    const status = JSON.parse(output);
    const dnsName = status?.Self?.DNSName?.replace(/\.$/, '');
    if (!dnsName) return null;

    const certPath = path.join(CONTINUUM_DIR, `${dnsName}.crt`);
    const keyPath = path.join(CONTINUUM_DIR, `${dnsName}.key`);
    if (!fs.existsSync(certPath) || !fs.existsSync(keyPath)) return null;

    return { hostname: dnsName, certPath, keyPath, provider: 'tailscale' };
  } catch {
    return null;
  }
}

/** Scan ~/.continuum/ for any .crt/.key pair (manual or future mesh) */
function discoverFromCerts(): NetworkIdentity | null {
  try {
    const files = fs.readdirSync(CONTINUUM_DIR);
    const certFile = files.find(f => f.endsWith('.crt') && !f.startsWith('.'));
    const keyFile = files.find(f => f.endsWith('.key') && !f.startsWith('.'));
    if (!certFile || !keyFile) return null;

    const hostname = certFile.replace('.crt', '');
    return {
      hostname,
      certPath: path.join(CONTINUUM_DIR, certFile),
      keyPath: path.join(CONTINUUM_DIR, keyFile),
      provider: 'manual',
    };
  } catch {
    return null;
  }
}

/**
 * Get TLS options for https.createServer(), or null if no TLS.
 */
export function getTlsOptions(): { cert: Buffer; key: Buffer } | null {
  const identity = getNetworkIdentity();
  if (!identity) return null;
  return {
    cert: fs.readFileSync(identity.certPath),
    key: fs.readFileSync(identity.keyPath),
  };
}

/**
 * Build the correct HTTP(S) URL for a given port.
 */
export function getServiceUrl(port: number): string {
  const identity = getNetworkIdentity();
  if (identity) return `https://${identity.hostname}:${port}`;
  return `http://localhost:${port}`;
}

/**
 * Build the correct WS(S) URL for a given port.
 */
export function getWebSocketUrl(port: number): string {
  const identity = getNetworkIdentity();
  if (identity) return `wss://${identity.hostname}:${port}`;
  return `ws://localhost:${port}`;
}
