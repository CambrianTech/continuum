/**
 * BrowserDeviceIdentity - Secure device identification for browser
 *
 * Generates and manages persistent device identity:
 * - Device ID: Unique identifier for this browser/device
 * - User ID: Persistent user identity (anonymous or authenticated)
 * - Encrypted storage in localStorage
 */

import { generateUUID, type UUID } from '../types/CrossPlatformUUID';

interface DeviceIdentity {
  deviceId: string;        // Unique device identifier
  userId: UUID;            // Persistent user ID (anonymous or authenticated)
  createdAt: string;       // When device was first registered
  deviceFingerprint?: {    // Optional device characteristics
    userAgent: string;
    screenResolution: string;
    timezone: string;
    language: string;
  };
}

export class BrowserDeviceIdentity {
  private static readonly STORAGE_KEY = 'continuum-device-identity';
  private static readonly ENCRYPTION_KEY = 'continuum-device-key';

  /**
   * Get or create device identity
   * Returns both deviceId and userId for persistence
   */
  static async getOrCreateIdentity(): Promise<DeviceIdentity> {
    // Try to load existing identity
    const existing = this.loadIdentity();
    if (existing) {
      console.log(`🔐 BrowserDeviceIdentity: Loaded existing device ${existing.deviceId.substring(0, 8)}...`);
      return existing;
    }

    // Create new device identity
    const identity = this.createNewIdentity();
    this.saveIdentity(identity);

    console.log(`🔐 BrowserDeviceIdentity: Created new device ${identity.deviceId.substring(0, 8)}...`);
    return identity;
  }

  /**
   * Get just the device ID (for backward compatibility)
   */
  static async getDeviceId(): Promise<string> {
    const identity = await this.getOrCreateIdentity();
    return identity.deviceId;
  }

  /**
   * Get just the user ID (for backward compatibility)
   */
  static async getUserId(): Promise<UUID> {
    const identity = await this.getOrCreateIdentity();
    return identity.userId;
  }

  /**
   * Update user ID when user authenticates
   * Keeps same deviceId, updates userId
   */
  static async upgradeToAuthenticated(authenticatedUserId: UUID): Promise<void> {
    const identity = await this.getOrCreateIdentity();

    const upgraded: DeviceIdentity = {
      ...identity,
      userId: authenticatedUserId
    };

    this.saveIdentity(upgraded);
    console.log(`🔐 BrowserDeviceIdentity: Upgraded to authenticated user ${authenticatedUserId.substring(0, 8)}...`);
  }

  /**
   * Create new device identity with random IDs
   */
  private static createNewIdentity(): DeviceIdentity {
    return {
      deviceId: `device-${generateUUID()}`,
      userId: generateUUID(),
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
    console.log('🔐 BrowserDeviceIdentity: Cleared device identity');
  }

  /**
   * Get device info for debugging
   */
  static async getDebugInfo(): Promise<{
    hasIdentity: boolean;
    deviceId?: string;
    userId?: UUID;
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
      userId: identity.userId,
      createdAt: identity.createdAt,
      fingerprint: identity.deviceFingerprint
    };
  }
}