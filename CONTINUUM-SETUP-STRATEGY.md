# Continuum Easy Setup Strategy

## User Experience Goal

**One-click install** that sets up all protection:
```
Continuum → Preferences → Security → Enable Shell Protection
[Install Protection]

Installing...
✓ Shell proxy installed
✓ Bluetooth monitoring active  
✓ Threat detection configured
✓ You're protected!
```

## Physical Drive vs Internal Storage

### Option A: Internal Storage (Easy)
```
~/.continuum/security/
├── safebash           # Shell proxy
├── threat-profiles.json
├── logs/
└── evidence/
```

**Advantages:**
- ✅ Always available (no external drive needed)
- ✅ Faster access (SSD speeds)
- ✅ Works for all users (no hardware required)
- ✅ Automatic updates via Continuum

**Disadvantages:**
- ❌ Evidence on compromised system (attacker can find it)
- ❌ No physical kill switch
- ❌ Harder to preserve if system wiped

### Option B: External Drive (Advanced)
```
/Volumes/SecureDrive/continuum/
├── safebash
├── threat-profiles.json
├── logs/
└── evidence/
```

**Advantages:**
- ✅ **Physical kill switch** (unplug = protection off, evidence preserved)
- ✅ **Airgap evidence** (can't be deleted remotely)
- ✅ **Portable** (move to another Mac, USB drive, encrypted volume)
- ✅ **Forensics-grade** (unmount = evidence preserved in original state)

**Disadvantages:**
- ❌ Requires external drive
- ❌ Must be plugged in to work
- ❌ User must manage drive

## Continuum Setup: **Hybrid Approach** (Best of both)

### Default: Internal Storage
Most users get automatic protection with no hardware:

```typescript
// Continuum installer:
async installShellProtection() {
    const installPath = '~/.continuum/security/';
    
    // Copy shell proxy
    await fs.copy('safebash', `${installPath}/safebash`);
    
    // Install rbash stub
    await exec('sudo cp rbash-stub /usr/local/bin/rbash');
    await exec('sudo sh -c "echo /usr/local/bin/rbash >> /etc/shells"');
    
    // Set as default shell
    await exec('chsh -s /usr/local/bin/rbash');
    
    // Done - works immediately
    showSuccess('Shell protection active!');
}
```

### Advanced: External Drive Option
Power users can enable "Forensics Mode":

```typescript
// Continuum Settings → Security → Advanced
[✓] Shell Protection (Active)
    Location: Internal (~/.continuum/security/)
    
    Advanced Options:
    [ ] Forensics Mode (use external drive)
        └─> [Configure...]
```

**Configure dialog:**
```
Forensics Mode Setup

This mode stores all evidence on an external drive for:
• Physical kill switch (unplug = disable)
• Airgap evidence preservation
• Forensics-grade integrity

Requirements:
• External drive (USB, Thunderbolt, encrypted volume)
• Drive must be mounted for protection to work

Select drive: [FlashGordon ▼]
              Macintosh HD
              FlashGordon ← (recommended)
              Other...

[✓] Auto-mount on login
[✓] Notify if drive unplugged
[✓] Fallback to internal if unavailable

[Cancel] [Enable Forensics Mode]
```

### Migration Path:
```
User starts → Internal (easy, automatic)
             ↓ (later, if needed)
User enables → Forensics Mode (external drive)
```

## Implementation: Smart Stub

```bash
# /usr/local/bin/rbash (installed by Continuum)

# Check for external drive first (forensics mode):
if [[ -x /Volumes/*/continuum/safebash ]]; then
    exec /Volumes/*/continuum/safebash "$@"
fi

# Fallback to internal (default mode):
if [[ -x ~/.continuum/security/safebash ]]; then
    exec ~/.continuum/security/safebash "$@"
fi

# Ultimate fallback:
exec /bin/bash "$@"
```

**Result:**
- Forensics Mode users: External drive used if plugged in
- Default users: Internal storage always works
- Everyone: Automatic fallback if primary unavailable

## Physical Drive Advantages - Concrete Scenarios

### Scenario 1: Remote Attacker
**Internal:** Attacker can delete evidence remotely
**External:** Unplug drive → evidence preserved, attacker can't touch it

### Scenario 2: Corporate IT/MDM
**Internal:** JumpCloud can scan `~/.continuum/` and find monitoring tools
**External:** Evidence lives outside normal file system paths, harder to discover

### Scenario 3: Legal Investigation
**Internal:** Evidence on potentially compromised system (integrity questions)
**External:** Clean evidence on separate media (chain of custody intact)

### Scenario 4: System Wipe
**Internal:** Evidence lost if system wiped/reinstalled
**External:** Evidence survives complete system wipe

### Scenario 5: Multi-Machine Investigation
**Internal:** Evidence stuck on one Mac
**External:** Plug drive into another Mac, continue investigation

## Recommendation: Tiered Approach

### Tier 1: Casual Users (90% of users)
- **Setup:** One-click install, internal storage
- **Experience:** "It just works"
- **Protection:** Good - monitors and alerts
- **Evidence:** Adequate for most threats

### Tier 2: Security-Conscious (9% of users)
- **Setup:** Enable "Forensics Mode" in settings
- **Experience:** Plug in external drive when investigating
- **Protection:** Better - physical kill switch
- **Evidence:** Forensics-grade preservation

### Tier 3: High-Risk Targets (1% of users)
- **Setup:** Custom encrypted volume, complex threat profiles
- **Experience:** Full manual control
- **Protection:** Maximum - airgapped, encrypted
- **Evidence:** Court-admissible preservation

## Continuum UI

### Settings → Security → Shell Protection

```
Shell Protection: [Enabled ✓]

Mode: Standard (Internal Storage)
      ↓ Change to Forensics Mode...

Status:
  • Monitoring: Active
  • Threats Detected: 2 (JumpCloud, Bluetooth device)
  • Sessions Logged: 47
  • Evidence Size: 2.3 MB

Threat Profiles:
  [Manage Threats...] (opens threat-profiles.json editor)

Actions:
  [View Logs] [Export Evidence] [Disable Protection]
```

### Advanced: Forensics Mode Active

```
Shell Protection: [Enabled ✓]

Mode: Forensics (External Drive)
      Location: /Volumes/FlashGordon/continuum/
      
Status:
  • Drive Status: Connected ✓
  • Monitoring: Active
  • Kill Switch: Armed (unplug to disable)
  • Threats Detected: 2
  
Drive Settings:
  [Change Drive...] [Backup Evidence...] [Eject Safely]
```

## Migration Example

**User starts with internal:**
```
Day 1: Install Continuum → Enable Shell Protection → Works!
```

**Later, they have an incident:**
```
Day 30: Suspicious activity detected
        → User: "I want forensics-grade evidence"
        → Enable Forensics Mode
        → Plug in external drive
        → Evidence migrates automatically
        → Physical kill switch now active
```

## Best of Both Worlds

**Ship with internal, offer external:**
- 90% of users never need external drive
- 9% enable it when they need it
- 1% use it from day one
- Everyone gets protection that fits their needs

**Continuum handles complexity:**
- Automatic fallback (external → internal)
- Seamless migration (internal → external)
- Smart detection (which mode am I in?)
- User sees simple UI, complexity hidden

---

**Physical drive = forensics-grade, but optional.**

**Everyone gets protection, power users get airgap.**

**Continuum makes it seamless.**

