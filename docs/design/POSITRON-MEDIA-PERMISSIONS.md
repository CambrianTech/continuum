# Media permissions — a positron concern, one contract per platform

**Source: Joel, 2026-08-31:** camera/mic access "needs to be a positron thing
inherent for people… for mobile and web, either html or natives you have this
permission ask / state the reason etc, then approve, and it never asks again.
It's tricky."

## The contract (platform-independent)

A human joining a live activity owns two capabilities — camera and mic — and
every client renders the SAME three-stage flow for each:

1. **REASON before ASK.** The native permission dialog never fires cold. The
   surface first shows a positron-owned card: what the capability is for, in
   the product's own words ("Citizens see you through your camera. Your feed
   stays on this grid — it is never recorded or sent anywhere else."), with an
   explicit continue affordance. Why: on web a blind deny is STICKY — the
   browser remembers it per-origin and recovery is buried in site settings; on
   iOS a denied `AVCaptureDevice` ask requires a trip to Settings.app. One
   bad cold prompt costs the capability semi-permanently.
2. **ASK on intent, once.** The native prompt (getUserMedia / OS runtime
   permission) fires only from the continue gesture. The platform's own
   permission memory is the durable store — positron never keeps a parallel
   "they said yes" flag that could disagree with the OS truth.
3. **STATE is queried, never assumed.** Each render, the surface reads the
   platform's live permission state and maps it:
   - `granted` → capability buttons act directly, no card ever again.
   - `prompt`  → button opens the reason card (stage 1).
   - `denied`  → button opens an honest recovery card: what was denied and the
     platform-specific path to undo it (web: the padlock/site-settings;
     iOS/Android: Settings → app → permission). Never a silent dead button,
     never a re-prompt loop the platform will refuse anyway.

## Per-platform mapping

| Stage | Web (HTML) | iOS | Android |
|---|---|---|---|
| State query | `navigator.permissions.query({name: 'camera'/'microphone'})` (Safari lacks it — treat as `prompt` and rely on the ask's outcome) | `AVCaptureDevice.authorizationStatus` | `checkSelfPermission` |
| Reason copy | positron card in the live face | same card + `NSCameraUsageDescription` / `NSMicrophoneUsageDescription` (the OS dialog's subtitle — keep the SAME wording) | same card + runtime-rationale (`shouldShowRequestPermissionRationale`) |
| The ask | `getUserMedia` on the continue click | `requestAccess(for:)` | `requestPermissions` |
| Memory | per-origin, browser-owned | OS-owned | OS-owned |
| Recovery from deny | site settings (padlock) — card links the steps | Settings.app deep link | app-settings intent |

## Web slice (shipped 2026-08-31)

`<chat-widget>`'s live face implements stages 1–3 for camera + mic:
`_mediaAsk` state drives the reason/recovery card in `renderLive`; the
mic/camera buttons query `navigator.permissions` first and only fire
`getUserMedia` from the card's continue gesture (or directly once granted).
Safari's missing Permissions API degrades to prompt-first, which is stage-1
behavior anyway.

## Laws

- The reason card's wording is product truth, not boilerplate — it must say
  where the feed goes (this grid) and where it doesn't.
- Positron never stores its own grant flag; the platform is the single
  authority ([[one-logical-decision-one-place]]).
- A denied state renders recovery instructions, never a re-ask loop.
