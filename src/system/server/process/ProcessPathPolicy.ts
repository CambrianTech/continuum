import * as path from 'path';

const SYSTEM_BIN_DIRS = Object.freeze([
  '/opt/homebrew/bin',
  '/usr/local/bin',
  '/usr/bin',
  '/bin',
]);

export function sandboxPath(): string {
  return SYSTEM_BIN_DIRS.join(path.delimiter);
}

export function sandboxPathDirs(): readonly string[] {
  return SYSTEM_BIN_DIRS;
}

export function ensureDaemonPath(currentPath: string, homeDir = process.env.HOME): string {
  const requiredDirs = [
    path.dirname(process.execPath),
    ...SYSTEM_BIN_DIRS,
    homeDir ? path.join(homeDir, '.local', 'bin') : undefined,
    homeDir ? path.join(homeDir, '.nvm', 'current', 'bin') : undefined,
  ].filter((dir): dir is string => Boolean(dir));

  const pathDirs = new Set(currentPath.split(path.delimiter).filter(Boolean));
  for (const dir of requiredDirs) {
    pathDirs.add(dir);
  }
  return Array.from(pathDirs).join(path.delimiter);
}
