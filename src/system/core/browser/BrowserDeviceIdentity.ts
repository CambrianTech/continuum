/**
 * BrowserDeviceIdentity - Device identification for browser
 *
 * IMPORTANT: This is for device identification ONLY.
 * The SERVER owns user identity. Browser just has a device token
 * that the server uses to look up or create the associated user.
 *
 * Flow:
 * 1. Browser generates/loads deviceId (stable per browser)
 * 2. Browser sends deviceId to server on connect
 * 3. Server maintains deviceId → userId mapping
 * 4. Server returns user info, browser displays it (doesn't store userId)
 */

import { generateUUID, type UUID } from '../types/CrossPlatformUUID';

// Verbose logging helper for browser
const verbose = () => typeof window !== 'undefined' && (window as any).JTAG_VERBOSE === true;

interface DeviceIdentity {
  deviceId: string;        // Unique device identifier (browser stores this)
  // NOTE: userId is NOT stored client-side anymore - server is source of truth
  createdAt: string;       // When device was first registered
  deviceFingerprint?: {    // Optional device characteristics
    userAgent: string;
    screenResolution: string;
    timezone: string;
    language: string;
  };
}

// Legacy interface for backward compatibility during migration
interface LegacyDeviceIdentity extends DeviceIdentity {
  userId?: UUID;  // May exist in old localStorage data, ignored
}

export class BrowserDeviceIdentity {
  private static readonly STORAGE_KEY = 'continuum-device-identity';
  private static readonly ENCRYPTION_KEY = 'continuum-device-key';

  /**
   * Get or create device identity
   * Returns deviceId only - server owns userId
   */
  static async getOrCreateIdentity(): Promise<DeviceIdentity & { userId?: never }> {
    // Try to load existing identity
    const existing = this.loadIdentity();
    if (existing) {
      verbose() && console.log(`🔐 BrowserDeviceIdentity: Loaded device ${existing.deviceId.substring(0, 12)}...`);
      // Return without userId even if legacy data has it
      return {
        deviceId: existing.deviceId,
        createdAt: existing.createdAt,
        deviceFingerprint: existing.deviceFingerprint
      };
    }

    // Create new device identity
    const identity = this.createNewIdentity();
    this.saveIdentity(identity);

    verbose() && console.log(`🔐 BrowserDeviceIdentity: Created new device ${identity.deviceId.substring(0, 12)}...`);
    return identity;
  }

  /**
   * Get just the device ID
   */
  static async getDeviceId(): Promise<string> {
    const identity = await this.getOrCreateIdentity();
    return identity.deviceId;
  }

  /**
   * DEPRECATED: User ID is now owned by server
   * This method exists only for backward compatibility during migration
   * @deprecated Use server-returned user instead
   */
  static async getUserId(): Promise<UUID | undefined> {
    const existing = this.loadIdentity() as LegacyDeviceIdentity | null;
    // Return legacy userId if it exists, but this should not be relied upon
    return existing?.userId;
  }

  /**
   * DEPRECATED: User identity is now server-managed
   * The server will associate userId with deviceId in its database
   * @deprecated Server manages user association now
   */
  static async upgradeToAuthenticated(_authenticatedUserId: UUID): Promise<void> {
    verbose() && console.log(`🔐 BrowserDeviceIdentity: upgradeToAuthenticated is deprecated - server manages user identity`);
    // No-op: server manages the deviceId → userId mapping now
  }

  /**
   * Create new device identity (no userId - server will create user)
   */
  private static createNewIdentity(): DeviceIdentity {
    return {
      deviceId: `device-${generateUUID()}`,
      createdAt: new Date().toISOString(),
      deviceFingerprint: this.generateFingerprint()
    };
  }

  /**
   * Generate device fingerprint for additional identification
   * (Not for security, just for user-facing device management)
   */
  private static generateFingerprint() {
    if (typeof window === 'undefined') return undefined;

    return {
      userAgent: navigator.userAgent,
      screenResolution: `${screen.width}x${screen.height}`,
      timezone: Intl.DateTimeFormat().resolvedOptions().timeZone,
      language: navigator.language
    };
  }

  /**
   * Save identity to localStorage with simple encryption
   */
  private static saveIdentity(identity: DeviceIdentity): void {
    try {
      // Simple obfuscation (not real security, just prevents casual inspection)
      const json = JSON.stringify(identity);
      const encrypted = this.simpleEncrypt(json);

      localStorage.setItem(this.STORAGE_KEY, encrypted);
    } catch (error) {
      console.error('❌ BrowserDeviceIdentity: Failed to save identity:', error);
    }
  }

  /**
   * Load identity from localStorage
   */
  private static loadIdentity(): DeviceIdentity | null {
    try {
      const encrypted = localStorage.getItem(this.STORAGE_KEY);
      if (!encrypted) return null;

      const json = this.simpleDecrypt(encrypted);
      return JSON.parse(json);
    } catch (error) {
      console.warn('⚠️ BrowserDeviceIdentity: Failed to load identity, will create new:', error);
      return null;
    }
  }

  /**
   * Simple XOR encryption (obfuscation)
   * NOT for security - just prevents casual localStorage inspection
   * Real security comes from passkey authentication
   */
  private static simpleEncrypt(text: string): string {
    const key = this.getOrCreateEncryptionKey();
    let result = '';

    for (let i = 0; i < text.length; i++) {
      result += String.fromCharCode(
        text.charCodeAt(i) ^ key.charCodeAt(i % key.length)
      );
    }

    return btoa(result); // Base64 encode
  }

  /**
   * Simple XOR decryption
   */
  private static simpleDecrypt(encrypted: string): string {
    const key = this.getOrCreateEncryptionKey();
    const text = atob(encrypted); // Base64 decode
    let result = '';

    for (let i = 0; i < text.length; i++) {
      result += String.fromCharCode(
        text.charCodeAt(i) ^ key.charCodeAt(i % key.length)
      );
    }

    return result;
  }

  /**
   * Get or create encryption key for localStorage obfuscation
   */
  private static getOrCreateEncryptionKey(): string {
    let key = localStorage.getItem(this.ENCRYPTION_KEY);

    if (!key) {
      // Generate random key
      key = Array.from({ length: 32 }, () =>
        String.fromCharCode(Math.floor(Math.random() * 94) + 33)
      ).join('');

      localStorage.setItem(this.ENCRYPTION_KEY, key);
    }

    return key;
  }

  /**
   * Clear device identity (for testing/logout)
   */
  static clear(): void {
    localStorage.removeItem(this.STORAGE_KEY);
    verbose() && console.log('🔐 BrowserDeviceIdentity: Cleared device identity');
  }

  /**
   * Get device info for debugging
   * Note: userId is no longer stored locally - server is source of truth
   */
  static async getDebugInfo(): Promise<{
    hasIdentity: boolean;
    deviceId?: string;
    createdAt?: string;
    fingerprint?: any;
  }> {
    const identity = this.loadIdentity();

    if (!identity) {
      return { hasIdentity: false };
    }

    return {
      hasIdentity: true,
      deviceId: identity.deviceId,
      // userId removed - server is source of truth
      createdAt: identity.createdAt,
      fingerprint: identity.deviceFingerprint
    };
  }
}