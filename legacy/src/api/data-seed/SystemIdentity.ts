/**
 * SystemIdentity - Smart defaults using environment info
 *
 * Philosophy: Use home directory name or username for consistent identity
 * across system restarts, just like how we identify Claude by Claude data.
 *
 * No hardcoded personal data - be clever, not hardcoded.
 */

import * as os from 'os';
import * as path from 'path';

export class SystemIdentity {
  /**
   * Get the system username (primary identity)
   * Priority: HOME dir basename > USER env > USERNAME env > 'developer'
   */
  static getUsername(): string {
    // Try HOME directory basename first (most reliable cross-platform)
    const homeDir = os.homedir();
    if (homeDir && homeDir !== '/') {
      const username = path.basename(homeDir);
      if (username && username !== 'root' && username !== 'home') {
        return username;
      }
    }

    // Fallback to environment variables
    return process.env.USER || process.env.USERNAME || 'developer';
  }

  /**
   * Get display name (capitalized username)
   */
  static getDisplayName(): string {
    const username = this.getUsername();
    return username.charAt(0).toUpperCase() + username.slice(1);
  }

  /**
   * Get email for system user
   */
  static getEmail(): string {
    const username = this.getUsername().toLowerCase();
    return `${username}@continuum.dev`;
  }

  /**
   * Get hostname for bio/location
   */
  static getHostname(): string {
    return os.hostname() || 'Local Machine';
  }

  /**
   * Get consistent user ID based on username
   * Format: user-{username}-{hash}
   * This allows the same user to be recreated consistently across sessions
   */
  static getUserId(): string {
    const username = this.getUsername().toLowerCase();
    // Simple hash for consistency (not cryptographic)
    const hash = username.split('').reduce((acc, char) => {
      return ((acc << 5) - acc) + char.charCodeAt(0);
    }, 0);
    const hashStr = Math.abs(hash).toString(16).padStart(8, '0');
    return `user-${username}-${hashStr}`;
  }

  /**
   * Get avatar emoji (can be customized later)
   */
  static getAvatar(): string {
    return '👤'; // Generic human avatar
  }

  /**
   * Get full system identity info
   */
  static getIdentity() {
    return {
      userId: this.getUserId(),
      username: this.getUsername(),
      displayName: this.getDisplayName(),
      email: this.getEmail(),
      avatar: this.getAvatar(),
      hostname: this.getHostname(),
    };
  }
}
