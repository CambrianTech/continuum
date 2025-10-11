# SecretManager - Security Architecture

## 🔐 Overview

SecretManager provides **server-side only** secret management for API keys and sensitive credentials used by AI provider adapters and other system components.

## Security Principles

### 1. **Server-Side Only Access**
- API keys are NEVER sent to the browser
- `getSecret()` can only be called from server-side code
- Browser environment cannot access SecretManager

### 2. **Multi-Source Loading Priority**
API keys are loaded in this order:
1. `~/.continuum/config.env` (user's home directory - PRIMARY)
2. `process.env` (system environment variables - FALLBACK)
3. `.env` file (project-local - DEVELOPMENT ONLY, never commit!)

### 3. **Automatic Redaction**
- All API keys are automatically filtered from:
  - Server logs
  - Browser logs
  - Screenshots (via JTAG screenshot command)
  - Error messages
  - Debug output
- Redacted format: `[REDACTED-OPENAI_API_KEY]`

### 4. **Audit Trail**
- Every secret access is logged with:
  - Key name (not value!)
  - Timestamp
  - Requesting component (e.g., 'OpenAIAdapter')
  - Environment (always 'server')
- Last 1000 accesses kept in memory
- Use `SecretManager.getInstance().getAuditLog()` for security review

### 5. **Graceful Degradation**
- Missing API keys don't crash the system
- Adapters can check availability: `SecretManager.getInstance().has('OPENAI_API_KEY')`
- Clear error messages guide users to add missing keys

## Usage

### In AI Provider Adapters
```typescript
import { getSecret } from '../../../../system/secrets/SecretManager';

export class OpenAIAdapter extends BaseOpenAICompatibleAdapter {
  constructor(apiKey?: string) {
    super({
      providerId: 'openai',
      providerName: 'OpenAI',
      // ✅ Secure: Server-side only, automatically redacted from logs
      apiKey: apiKey || getSecret('OPENAI_API_KEY', 'OpenAIAdapter') || '',
      baseUrl: 'https://api.openai.com',
      // ...
    });
  }
}
```

### Initialization
SecretManager is initialized automatically during AIProviderDaemon startup:

```typescript
// In AIProviderDaemonServer.ts
import { initializeSecrets } from '../../../system/secrets/SecretManager';

protected async initialize(): Promise<void> {
  // Initialize SecretManager FIRST (adapters depend on it)
  await initializeSecrets();
  // ... rest of initialization
}
```

### User Configuration
Users configure API keys in `~/.continuum/config.env`:

```bash
# ~/.continuum/config.env
OPENAI_API_KEY=sk-proj-...
ANTHROPIC_API_KEY=sk-ant-...
TOGETHER_API_KEY=...
FIREWORKS_API_KEY=...
GROQ_API_KEY=...
```

**Note**: This file should have restricted permissions (`chmod 600 ~/.continuum/config.env`)

## API Reference

### Core Methods

#### `initializeSecrets()`
Initializes SecretManager singleton and loads secrets from all sources.
```typescript
await initializeSecrets();
```

#### `getSecret(key, requestedBy?)`
Retrieves a secret value (returns `undefined` if not found).
```typescript
const apiKey = getSecret('OPENAI_API_KEY', 'MyComponent');
```

#### `requireSecret(key, requestedBy?)`
Retrieves a secret value (throws if not found).
```typescript
const apiKey = requireSecret('OPENAI_API_KEY', 'MyComponent');
// Throws: Missing required secret: OPENAI_API_KEY
// Please add it to ~/.continuum/config.env:
// OPENAI_API_KEY=your-key-here
```

#### `redactSecrets(text)`
Removes all API key values from text.
```typescript
const safeLog = redactSecrets('Using key: sk-proj-abc123...');
// Returns: "Using key: [REDACTED-OPENAI_API_KEY]"
```

### Advanced Methods

#### `SecretManager.getInstance().set(key, value)`
Sets or updates a secret (persists to `~/.continuum/config.env`).
```typescript
await SecretManager.getInstance().set('OPENAI_API_KEY', 'sk-proj-...');
```

#### `SecretManager.getInstance().remove(key)`
Removes a secret (deletes from `~/.continuum/config.env`).
```typescript
await SecretManager.getInstance().remove('OPENAI_API_KEY');
```

#### `SecretManager.getInstance().getAvailableKeys()`
Lists configured secret keys (NOT values!).
```typescript
const keys = SecretManager.getInstance().getAvailableKeys();
// Returns: ['OPENAI_API_KEY', 'ANTHROPIC_API_KEY', ...]
```

#### `SecretManager.getInstance().getAuditLog()`
Retrieves access audit trail for security review.
```typescript
const log = SecretManager.getInstance().getAuditLog();
// Returns: [
//   { key: 'OPENAI_API_KEY', accessedAt: 1234567890, requestedBy: 'OpenAIAdapter', environment: 'server' },
//   ...
// ]
```

## Security Best Practices

### ✅ DO
- Store API keys in `~/.continuum/config.env` (user's home directory)
- Use `getSecret()` in adapter constructors
- Set file permissions: `chmod 600 ~/.continuum/config.env`
- Review audit logs periodically for unexpected access
- Use `requireSecret()` for critical keys that must be present

### ❌ DON'T
- Never commit `.env` files to git (already in `.gitignore`)
- Never hardcode API keys in source code
- Never log API key values directly
- Never send API keys to browser via commands/events
- Never use `process.env` directly - always use `getSecret()`

## Cross-Environment Security

### Why SecretManager is Critical
Continuum supports:
- **Remote command execution** - Commands can run on remote JTAG instances
- **Browser-server architecture** - Code runs in both environments
- **P2P mesh networking** - Commands can traverse the network

**Without SecretManager**, API keys could accidentally:
- Leak through browser console logs
- Appear in screenshots shared for debugging
- Travel across the network in command parameters
- Show up in error messages displayed in UI

**With SecretManager**:
- Keys stay server-side only
- Automatic redaction protects against accidental exposure
- Clear boundaries prevent cross-environment leaks

## Future Enhancements

### Persona-Guided Setup Widget
A UI widget will guide users through secure API key setup:
- Explains how to obtain API keys for each provider
- Validates key format before saving
- Provides visual confirmation (without showing actual key)
- Helper AI persona assists with the process

### Enhanced Security
- [ ] Encrypted storage (OS keychain integration)
- [ ] Key rotation support
- [ ] Rate limiting per key
- [ ] Cost alerts and spending limits
- [ ] Multi-user key isolation

## Integration Status

### ✅ Integrated Components
- `OpenAIAdapter` - Uses `getSecret('OPENAI_API_KEY')`
- `TogetherAIAdapter` - Uses `getSecret('TOGETHER_API_KEY')`
- `FireworksAdapter` - Uses `getSecret('FIREWORKS_API_KEY')`
- `AIProviderDaemonServer` - Initializes SecretManager on startup

### 🔜 Pending Integration
- AnthropicAdapter (existing, needs SecretManager)
- GroqAdapter (not yet implemented)
- MistralAdapter (not yet implemented)
- GoogleGeminiAdapter (not yet implemented)
- CohereAdapter (not yet implemented)

## Troubleshooting

### "Missing required secret" Error
```
Error: Missing required secret: OPENAI_API_KEY
Please add it to ~/.continuum/config.env:
OPENAI_API_KEY=your-key-here
```

**Solution**: Create or edit `~/.continuum/config.env` and add the key:
```bash
mkdir -p ~/.continuum
echo "OPENAI_API_KEY=sk-proj-..." >> ~/.continuum/config.env
chmod 600 ~/.continuum/config.env
```

### Keys Not Loading
**Check initialization**:
```bash
tail -f .continuum/sessions/user/shared/*/logs/server.log | grep SecretManager
```

Expected output:
```
🔐 AIProviderDaemonServer: Initializing SecretManager...
🔐 SecretManager: Initializing secrets...
✅ SecretManager: Loaded secrets from /Users/joel/.continuum/config.env
✅ SecretManager: Loaded 4 secrets
✅ AIProviderDaemonServer: SecretManager initialized
```

### Audit Access
```typescript
// In any server-side code
const audit = SecretManager.getInstance().getAuditLog();
console.log('Recent secret access:', audit.slice(-10));
```

## Testing SecretManager

### Manual Testing
```bash
# 1. Create test config
mkdir -p ~/.continuum
cat > ~/.continuum/config.env << 'EOF'
TEST_API_KEY=test-key-123
EOF

# 2. Deploy system
npm start

# 3. Check logs
tail -f .continuum/sessions/user/shared/*/logs/server.log | grep SecretManager

# 4. Test redaction
# Any logs containing "test-key-123" should show "[REDACTED-TEST_API_KEY]" instead
```

### Programmatic Testing
```typescript
import { SecretManager, getSecret, redactSecrets } from './SecretManager';

// Test basic access
const key = getSecret('TEST_API_KEY', 'TestSuite');
console.assert(key === 'test-key-123', 'Key retrieval failed');

// Test redaction
const text = 'API key: test-key-123';
const redacted = redactSecrets(text);
console.assert(redacted === 'API key: [REDACTED-TEST_API_KEY]', 'Redaction failed');

// Test audit trail
const log = SecretManager.getInstance().getAuditLog();
console.assert(log.some(entry =>
  entry.key === 'TEST_API_KEY' &&
  entry.requestedBy === 'TestSuite'
), 'Audit trail failed');
```

## Summary

SecretManager provides **defense-in-depth** security for API keys in Continuum's cross-environment, distributed architecture:

1. ✅ **Server-side only** - Keys never leave the server
2. ✅ **Automatic redaction** - Accidental exposure prevented
3. ✅ **Audit trail** - Security monitoring built-in
4. ✅ **Graceful degradation** - Missing keys don't crash system
5. ✅ **Multi-source loading** - Flexible configuration

**Result**: Safe AI provider integration with strong security guarantees.
