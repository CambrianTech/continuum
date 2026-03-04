# Passkey Authentication Design

## Current State (Anonymous Browser Identity)

```typescript
// localStorage: 'continuum-browser-user-id' = generated UUID
// UserState stored in localStorage with backend: 'browser'
// No server authentication - purely local state
```

**Limitations:**
- No cross-device sync (localStorage is device-bound)
- No account recovery
- No real identity verification
- Preferences disappear if localStorage is cleared

---

## Passkey Authentication Integration

### Architecture Overview

```
┌─────────────────────────────────────────────────────────────┐
│                    Authentication Flow                       │
├─────────────────────────────────────────────────────────────┤
│                                                               │
│  Browser (First Visit)                                       │
│  └─ Generate ephemeral session                               │
│     └─ userId: anonymous (from localStorage)                 │
│     └─ UserState in localStorage (local only)                │
│                                                               │
│  User Creates Account (Passkey)                              │
│  └─ navigator.credentials.create()                           │
│     └─ Browser generates cryptographic key pair              │
│     └─ Public key → Server                                   │
│     └─ Private key → Secure enclave (never leaves device)    │
│     └─ Server creates User record with publicKey             │
│     └─ Returns: authenticatedUserId                          │
│                                                               │
│  Upgrade Anonymous → Authenticated                           │
│  └─ Migrate localStorage UserState → Server database         │
│     └─ Copy preferences, contentState, etc.                  │
│     └─ Link anonymous userId → authenticated userId          │
│     └─ Switch backend: 'browser' → 'server'                  │
│                                                               │
│  Subsequent Visits                                           │
│  └─ Check localStorage: 'continuum-authenticated-user'       │
│     └─ If exists: Auto-login via passkey                     │
│     └─ navigator.credentials.get()                           │
│        └─ Sign challenge with private key                    │
│        └─ Server verifies signature                          │
│        └─ Returns: session token + userId                    │
│                                                               │
└─────────────────────────────────────────────────────────────┘
```

---

## Implementation Phases

### Phase 1: Hybrid State (Current + Authenticated)

**Goal**: Support both anonymous (localStorage) and authenticated (server) users

```typescript
// User identity determination
async function resolveUserId(): Promise<{
  userId: UUID;
  backend: 'browser' | 'server';
  authenticated: boolean;
}> {
  // Check for authenticated session
  const authToken = localStorage.getItem('continuum-auth-token');
  if (authToken) {
    const verified = await verifyAuthToken(authToken);
    if (verified.valid) {
      return {
        userId: verified.userId,
        backend: 'server',
        authenticated: true
      };
    }
  }

  // Fall back to anonymous browser identity
  let userId = localStorage.getItem('continuum-browser-user-id');
  if (!userId) {
    userId = generateUUID();
    localStorage.setItem('continuum-browser-user-id', userId);
  }

  return {
    userId,
    backend: 'browser',
    authenticated: false
  };
}
```

**Theme persistence with hybrid support**:
```typescript
async function saveTheme(themeName: string) {
  const identity = await resolveUserId();

  await Commands.execute('data/update', {
    collection: 'UserState',
    filter: { userId: identity.userId },
    backend: identity.backend, // 'browser' or 'server'
    data: {
      preferences: { theme: themeName }
    }
  });
}
```

---

### Phase 2: Passkey Registration Flow

**New Command**: `auth/register-passkey`

```typescript
// Browser command
async function registerPasskey(params: {
  displayName: string;
  email?: string;
}) {
  // 1. Request challenge from server
  const challenge = await Commands.execute('auth/challenge', {
    action: 'register'
  });

  // 2. Create passkey credential
  const credential = await navigator.credentials.create({
    publicKey: {
      challenge: challenge.bytes,
      rp: {
        name: 'Continuum',
        id: window.location.hostname
      },
      user: {
        id: stringToBytes(challenge.userId), // Temporary userId for registration
        name: params.email || params.displayName,
        displayName: params.displayName
      },
      pubKeyCredParams: [
        { alg: -7, type: 'public-key' },  // ES256
        { alg: -257, type: 'public-key' } // RS256
      ],
      authenticatorSelection: {
        authenticatorAttachment: 'platform', // Prefer device biometrics
        userVerification: 'required',
        residentKey: 'preferred' // Discoverable credential
      },
      timeout: 60000,
      attestation: 'none' // Privacy-friendly
    }
  });

  // 3. Send public key to server for verification
  const result = await Commands.execute('auth/register-passkey', {
    credential: credential,
    displayName: params.displayName,
    email: params.email,
    anonymousUserId: localStorage.getItem('continuum-browser-user-id')
  });

  // 4. Store authenticated session
  if (result.success) {
    localStorage.setItem('continuum-auth-token', result.token);
    localStorage.setItem('continuum-authenticated-user', result.userId);

    // 5. Migrate anonymous state to authenticated user
    await migrateAnonymousState(result.userId);

    return { success: true, userId: result.userId };
  }
}
```

---

### Phase 3: Passkey Authentication Flow

**New Command**: `auth/login-passkey`

```typescript
// Browser command (auto-triggered on page load if credential exists)
async function loginWithPasskey() {
  try {
    // 1. Check if user has saved credential
    const savedUserId = localStorage.getItem('continuum-authenticated-user');
    if (!savedUserId) {
      return { authenticated: false };
    }

    // 2. Request challenge from server
    const challenge = await Commands.execute('auth/challenge', {
      action: 'authenticate',
      userId: savedUserId
    });

    // 3. Get passkey credential (browser prompts user)
    const credential = await navigator.credentials.get({
      publicKey: {
        challenge: challenge.bytes,
        rpId: window.location.hostname,
        allowCredentials: [{
          type: 'public-key',
          id: stringToBytes(challenge.credentialId)
        }],
        userVerification: 'required',
        timeout: 60000
      }
    });

    // 4. Verify signature with server
    const result = await Commands.execute('auth/verify-passkey', {
      credential: credential,
      userId: savedUserId
    });

    if (result.success) {
      localStorage.setItem('continuum-auth-token', result.token);
      return {
        authenticated: true,
        userId: result.userId,
        session: result.session
      };
    }

  } catch (error) {
    console.warn('Passkey authentication failed:', error);
    // Fall back to anonymous mode
    return { authenticated: false };
  }
}
```

---

### Phase 4: State Migration (Anonymous → Authenticated)

```typescript
// Migrate localStorage UserState to server database
async function migrateAnonymousState(authenticatedUserId: UUID) {
  const anonymousUserId = localStorage.getItem('continuum-browser-user-id');
  if (!anonymousUserId) return;

  console.log(`🔄 Migrating anonymous state ${anonymousUserId} → ${authenticatedUserId}`);

  // 1. Load anonymous UserState from localStorage
  const anonymousState = await Commands.execute('data/read', {
    collection: 'UserState',
    filter: { userId: anonymousUserId },
    backend: 'browser'
  });

  if (anonymousState.success && anonymousState.data) {
    // 2. Create authenticated UserState on server
    await Commands.execute('data/create', {
      collection: 'UserState',
      backend: 'server', // Store on server for cross-device sync
      data: {
        ...anonymousState.data,
        userId: authenticatedUserId, // Link to authenticated user
        deviceId: `browser-${generateShortId()}`,
        migratedFrom: anonymousUserId,
        migratedAt: new Date().toISOString()
      }
    });

    // 3. Keep localStorage version as cache (hybrid approach)
    await Commands.execute('data/update', {
      collection: 'UserState',
      backend: 'browser',
      filter: { userId: anonymousUserId },
      data: {
        syncedToServer: true,
        authenticatedUserId: authenticatedUserId
      }
    });

    console.log('✅ Anonymous state migrated to authenticated user');
  }
}
```

---

## Security Considerations

### 1. **Passkey Security Model**

**Strengths:**
- Private key never leaves device (stored in secure enclave/TPM)
- Phishing-resistant (cryptographic challenge-response tied to domain)
- No passwords to leak/forget
- Biometric verification (Face ID, Touch ID, Windows Hello)

**Server stores:**
```typescript
interface AuthenticatedUser extends BaseUser {
  // Public key for signature verification
  publicKey: string; // Base64-encoded public key
  credentialId: string; // Unique credential identifier

  // Optional backup authentication
  backupPublicKeys?: Array<{
    publicKey: string;
    credentialId: string;
    deviceName: string;
    addedAt: string;
  }>;

  // Security metadata
  lastAuthenticated: string;
  authenticationCount: number;
  registeredDevices: Array<{
    credentialId: string;
    deviceType: 'platform' | 'cross-platform';
    addedAt: string;
  }>;
}
```

### 2. **Token Management**

```typescript
// JWT token stored in localStorage (not cookie for API usage)
interface AuthToken {
  userId: UUID;
  sessionId: UUID;
  exp: number; // Expiration timestamp
  iat: number; // Issued at
  scope: 'full' | 'limited'; // Permission level
}

// Token refresh flow
async function refreshAuthToken() {
  const currentToken = localStorage.getItem('continuum-auth-token');

  // If token expires in < 5 minutes, refresh
  const decoded = decodeJWT(currentToken);
  if (decoded.exp - Date.now() < 5 * 60 * 1000) {
    const refreshed = await Commands.execute('auth/refresh', {
      token: currentToken
    });

    localStorage.setItem('continuum-auth-token', refreshed.token);
  }
}
```

### 3. **Data Sync Strategy**

```typescript
// Hybrid sync: localStorage (fast) + Server (persistent)
class HybridStateManager {
  // Write to both localStorage and server
  async saveState(userId: UUID, state: UserStateEntity) {
    const isAuthenticated = localStorage.getItem('continuum-auth-token');

    // Always write to localStorage for instant access
    await Commands.execute('data/update', {
      collection: 'UserState',
      backend: 'browser',
      filter: { userId },
      data: state
    });

    // If authenticated, also sync to server
    if (isAuthenticated) {
      await Commands.execute('data/update', {
        collection: 'UserState',
        backend: 'server',
        filter: { userId },
        data: {
          ...state,
          syncedAt: new Date().toISOString()
        }
      });
    }
  }

  // Read from localStorage first (fast), fall back to server
  async loadState(userId: UUID) {
    // Try localStorage first
    const local = await Commands.execute('data/read', {
      collection: 'UserState',
      backend: 'browser',
      filter: { userId }
    });

    if (local.success && local.data) {
      // Background sync from server (check for updates from other devices)
      this.backgroundSyncFromServer(userId);
      return local.data;
    }

    // Fall back to server if localStorage is empty
    const server = await Commands.execute('data/read', {
      collection: 'UserState',
      backend: 'server',
      filter: { userId }
    });

    if (server.success && server.data) {
      // Cache to localStorage for future
      await this.cacheToLocalStorage(server.data);
      return server.data;
    }
  }
}
```

---

## Privacy & Compliance

### GDPR Considerations

1. **Anonymous Mode**: Full privacy, no server storage, no tracking
2. **Authenticated Mode**: User explicitly opts-in via passkey creation
3. **Data Portability**: Export all UserState data
4. **Right to Erasure**: Delete server UserState, keep localStorage untouched
5. **Transparency**: Clear UI showing where data is stored

### Data Minimization

```typescript
// Only sync essential preferences to server
interface ServerSyncedPreferences {
  theme: string;
  maxOpenTabs: number;
  // ... only user-critical settings
}

// Keep device-specific data local only
interface LocalOnlyPreferences {
  lastWindowPosition: { x: number; y: number };
  sidebarWidth: number;
  recentSearches: string[];
  // ... transient UI state
}
```

---

## Migration Path

### Step 1: Current (Anonymous Only) ✅
- localStorage browser userId
- backend: 'browser' for all operations
- No server authentication

### Step 2: Hybrid Support (Next)
- Add `resolveUserId()` helper
- Support both backends based on authentication
- Theme/preferences work for both anonymous and authenticated

### Step 3: Passkey Infrastructure
- Implement `auth/register-passkey` command
- Implement `auth/login-passkey` command
- Add AuthDaemon for token management

### Step 4: State Migration
- Auto-migrate on first authentication
- Hybrid sync (localStorage + server)
- Cross-device sync for authenticated users

### Step 5: Advanced Features
- Multiple device support
- Recovery keys (backup authentication)
- Account management UI
- Activity log & security alerts

---

## User Experience Flow

### Anonymous User (Current)
```
1. Visit site → Generate browser userId → Store in localStorage
2. Change preferences → Save to localStorage
3. Refresh page → Load from localStorage
4. Clear localStorage → Preferences lost (expected)
```

### Authenticated User (Future)
```
1. Visit site → Auto-login via passkey (if registered)
2. Change preferences → Save to both localStorage + server
3. Refresh page → Load from localStorage (instant)
4. Use different device → Login via passkey → Sync from server
5. Clear localStorage → Re-sync from server on next login
```

### Progressive Upgrade
```
1. Use anonymously for days/weeks
2. Decide to create account → Click "Sign Up"
3. Create passkey (Face ID prompt)
4. All preferences instantly migrate
5. Now synced across devices
```

---

## Implementation Checklist

- [ ] Add AuthDaemon (server + browser)
- [ ] Implement passkey registration flow
- [ ] Implement passkey authentication flow
- [ ] Add `resolveUserId()` helper
- [ ] Update theme commands to support both backends
- [ ] Implement state migration logic
- [ ] Add hybrid sync manager
- [ ] Create account management UI
- [ ] Add recovery key backup system
- [ ] GDPR compliance (data export, deletion)
- [ ] Security audit of token handling
- [ ] Cross-device sync testing
- [ ] Biometric failure fallback

---

## Technical Notes

**Passkey Browser Support** (2025):
- ✅ Chrome 108+
- ✅ Safari 16+
- ✅ Firefox 119+
- ✅ Edge 108+

**Platform Authenticators**:
- iOS: Face ID / Touch ID
- macOS: Touch ID
- Windows: Windows Hello
- Android: Fingerprint / Face unlock

**Fallback for unsupported browsers**:
- Detect `navigator.credentials` availability
- Show "Install modern browser" message
- Keep anonymous mode working
- No passkey requirement for basic usage