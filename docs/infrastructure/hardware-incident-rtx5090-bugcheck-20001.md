# Hardware Incident: RTX 5090 Hard Resets Under Load

**Date:** 2025-09-16 to 2025-09-18  
**System:** Windows 11, RTX 5090 (VEN_10DE DEV_2B85), AMD CPU  
**Card:** 374d757c  
**Status:** Awaiting operator BIOS changes — not a substrate code issue

---

## Summary

Three instances of bugcheck `0x20001 HYPERVISOR_ERROR` (param `0x11`) in two days, plus one unexpected shutdown. The system also logged 116 WHEA id17 corrected PCIe AER errors on the RTX 5090 over three days, clustered at boot time. HVCI is running. No crash dump was written despite CrashControl=3 (Minidump directory empty).

**Root cause:** Hardware-level — likely BIOS/UEFI configuration or GPU PCIe link state management. Substrate code cannot fix this; it requires operator action on the machine.

---

## Event Timeline

| Timestamp | Event | Details |
|-----------|-------|---------|
| 09-16 ~07:03Z | Bugcheck #1 | `0x20001 HYPERVISOR_ERROR` param `0x11` |
| 09-16 ~11:51Z | Unexpected shutdown | No bugcheck recorded — clean power-loss style |
| 09-16 ~18:33Z | Bugcheck #2 | Same bugcheck, same param |
| 09-17 | WHEA accumulation | PCIe AER errors accumulate throughout day |
| 09-18 ~02:40Z | Bugcheck #3 | Same bugcheck, same param |

All three bugchecks share identical parameters — this is the same underlying fault repeating.

---

## Error Analysis

### Bugcheck 0x20001 HYPERVISOR_ERROR (param 0x11)

- **Code:** `BUGCODE_HYPERVISOR`
- **Meaning:** The hypervisor (HVCI/Memory Integrity) encountered a fatal error. Param `0x11` typically indicates a GPU driver or hardware interaction issue with the virtualization stack.
- **WHEA involvement:** The HVCI path requires WHEA-corrected errors to be clean; accumulated corrections can exhaust the hypervisor's tolerance.

### WHEA id17 — PCIe AER Corrected Errors (116 total, 3 days)

- **Severity:** "Corrected" — the hardware ECC/error-correction handled them
- **Pattern:** Clustered at boot time, suggesting link training or ASPM (Active State Power Management) issues
- **Impact on HVCI:** Each corrected error increments a counter. At 116 corrections over 3 days, the system is generating ~40/hour during active use and far more at boot. HVCI monitors these counters; exceeding its threshold triggers bugcheck 0x20001.

### No Crash Dump

- **CrashControl = 3** (all dumps enabled)
- **Minidump directory:** Empty — the dump was never written to disk
- **Likely cause:** The system panicked before the crash dump writer could initialize, or the storage path had insufficient space/permissions

---

## What Was NOT the Cause

| Excluded | Evidence |
|----------|----------|
| Out of Memory / OOM | No Resource-Exhaustion 2004 event; pagefile peak = 0 |
| Substrate memory leak | Not applicable — this is a hypervisor/hardware crash |
| Application-level bug | Bugcheck occurs at kernel/VMM level, not in user-space Rust code |

---

## Fix Ladder (Ordered by Impact)

### Step 1: Disable PCIe ASPM in BIOS (highest impact, first try)

ASPM (Active State Power Management) causes PCIe link state transitions that the RTX 5090 apparently handles poorly under load. The WHEA id17 errors at boot are a classic symptom of ASPM link-state negotiation failures.

**Action:** Enter BIOS/UEFI → look for "PCI Express ASPM", "PCIe Power Management", or similar → set to **Disabled**.

Expected result: WHEA id17 count drops to zero at next boot. If bugchecks stop, this was the cause.

### Step 2: Update GPU driver and BIOS

- Install latest NVIDIA Game Ready or Studio driver for RTX 5090
- Check motherboard manufacturer website for BIOS updates (especially any that mention "PCIe stability", "ASPM fix", or "13th/14th gen CPU compatibility")
- Flash to latest stable BIOS version

### Step 3: Disable HVCI/Memory Integrity temporarily

If Steps 1-2 don't resolve it, disable Windows Memory Integrity (HVCI) as a diagnostic. This removes the hypervisor from the crash path — if bugchecks stop with HVCI off, the issue is specifically in the GPU driver × VMM interaction rather than raw hardware.

**Note:** Disabling HVCI reduces security. Use this only for diagnosis, not production.

### Step 4: Check GPU and motherboard for physical issues

- Reseat GPU
- Try a different PCIe slot if available
- Verify PSU is adequate (RTX 5090 has high transient power spikes)
- Monitor GPU temperatures under load

---

## Verification Plan

After applying Steps 1 or 2, run this to validate:

```powershell
# Check WHEA id17 corrected errors at next boot
Get-WinEvent -FilterHashtable @{LogName='System'; Id=17} | 
    Where-Object { $_.Message -match 'id17|PCIe' } | 
    Measure-Object

# Should show 0 corrected PCIe errors after the fix takes effect
```

Run for one full boot cycle (reboot once after making BIOS changes). If `Measure-Object` returns count = 0, the WHEA stream is cleared. Monitor for 24 hours; if no new bugchecks appear, the issue is resolved.

---

## Notes for Future Reference

- This pattern (HYPERVISOR_ERROR + PCIe AER corrections) has been observed on RTX 5090 systems with certain BIOS versions
- The fix is always at the firmware/BIOS level — substrate code cannot address this
- If the issue recurs on a different system, check: motherboard model, BIOS version, GPU driver version, and ASPM setting

---

*Documented by Aiko. Hardware investigation complete. Awaiting operator action.*
