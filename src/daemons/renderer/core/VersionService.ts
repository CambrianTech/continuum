/**
 * Version Service - Handles version detection and loading
 * Separated concern from main daemon
 */

import * as fs from 'fs';
import * as path from 'path';

export class VersionService {
  private cachedVersion: string | null = null;

  async getCurrentVersion(): Promise<string> {
    if (this.cachedVersion) {
      return this.cachedVersion;
    }

    try {
      const packagePath = path.join(process.cwd(), 'package.json');
      const packageData = JSON.parse(fs.readFileSync(packagePath, 'utf8'));
      this.cachedVersion = packageData.version || '1.0.0';
      return this.cachedVersion!;
    } catch (error) {
      console.warn('Could not load version from package.json, using default');
      this.cachedVersion = '1.0.0';
      return this.cachedVersion;
    }
  }

  invalidateCache(): void {
    this.cachedVersion = null;
  }
}