# Anonymous User Leak - Root Cause & Fix

## Problem

Anonymous users can't be permanently deleted because:

1. **Browser localStorage persists deleted userId**
   - When an anonymous user is deleted, their userId is still in `localStorage['continuum-device-identity']`
   - On next session creation, SessionDaemon tries to use this stale userId
   - Since user doesn't exist, it creates a NEW anonymous user
   - Result: Hydra effect - delete one, two more appear

2. **Multiple tabs = multiple anonymous users**
   - Each open tab creates its own session
   - Each session can create an anonymous user if no user found
   - When you delete, other tabs immediately recreate

3. **No cleanup on user deletion**
   - When a user is deleted, device associations aren't cleaned up
   - Browser still thinks it "belongs" to that deleted user

## Root Cause

**File**: `daemons/session-daemon/server/SessionDaemonServer.ts`
**Lines**: 700-722

When creating a session for `browser-ui` client:
```typescript
// Look for existing user associated with this device
const existingUser = await this.findUserByDeviceId(deviceId);
if (existingUser) {
  user = existingUser;  // ✅ Found user for this device
} else {
  // New device - create anonymous human
  user = await this.createAnonymousHuman(params, deviceId);  // ❌ Creates new anonymous user
}
```

**The bug**: If the user was deleted, `findUserByDeviceId` returns null, so a NEW anonymous user is created.

## Solution

### Fix 1: Clear localStorage when deleting users (Client-side)

When a user deletes an anonymous user from the UI, also clear browser localStorage:

```typescript
// In the delete handler
await Commands.execute('data/delete', { collection: 'users', id: userId });

// If it was MY user, clear my localStorage
const myDeviceIdentity = BrowserDeviceIdentity.loadIdentity();
if (myDeviceIdentity?.userId === userId) {
  localStorage.removeItem('continuum-device-identity');
  localStorage.removeItem('continuum-device-key');
  // Reload to get fresh identity
  window.location.reload();
}
```

### Fix 2: Cascade delete device associations (Server-side)

When a user is deleted, clean up orphaned device associations:

**File**: `daemons/user-daemon/server/UserDaemonServer.ts`
**Method**: `handleUserDeleted()`

```typescript
private async handleUserDeleted(userEntity: UserEntity): Promise<void> {
  // Clean up device associations
  const devices = await DataDaemon.list('user_devices', {
    filter: { userId: userEntity.id },
  });

  for (const device of devices) {
    await DataDaemon.remove('user_devices', device.id);
  }

  // Existing cleanup...
  if (userEntity.type === 'persona') {
    this.personaClients.delete(userEntity.id);
  }
}
```

### Fix 3: Don't recreate deleted anonymous users

Add logic to detect "this device used to have a user but it was deleted":

```typescript
const deviceData = await this.getDeviceData(deviceId);
if (deviceData?.lastUserId) {
  const userExists = await this.userExists(deviceData.lastUserId);
  if (!userExists) {
    // User was deleted - clear device association
    await this.clearDeviceUser(deviceId);
  }
}
```

## Immediate Workaround

Run this script after npm start:

```bash
npx tsx scripts/delete-anonymous-users.ts
```

Then in **all open browser tabs**, run in console:
```javascript
localStorage.removeItem('continuum-device-identity');
localStorage.removeItem('continuum-device-key');
location.reload();
```

## Long-term Fix

1. **Fix 1** - Add to UserProfileWidget delete handler
2. **Fix 2** - Add to UserDaemonServer.handleUserDeleted()
3. **Fix 3** - Add to SessionDaemonServer device lookup logic

This will prevent the hydra effect completely.
