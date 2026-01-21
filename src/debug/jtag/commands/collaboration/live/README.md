# Live Commands

Real-time audio/video collaboration commands - like Slack huddles, Discord voice channels, Zoom.

## Overview

Live sessions allow multiple participants to communicate via audio, video, and screen sharing.
Each session is attached to a room/activity. Participants join/leave dynamically.

## Commands

### `live/join`

Join a live session for a room. Creates session if none exists.

```bash
# Join live session for a room
./jtag live/join --roomId="general"

# Response includes session info and participant list
{
  "success": true,
  "sessionId": "...",
  "participants": [...],
  "myParticipant": { "userId": "...", "micEnabled": false, ... }
}
```

### `live/leave`

Leave a live session. Session ends automatically when last participant leaves.

```bash
# Leave a session
./jtag live/leave --sessionId="..."

# Response
{
  "success": true,
  "sessionEnded": false,
  "remainingParticipants": 2
}
```

## Architecture

### LiveSessionEntity

Tracks session state in database:
- `roomId` - Associated room
- `status` - 'active' | 'ended'
- `participants` - Array of LiveParticipant with mic/camera/screen state
- `peakParticipants` / `totalParticipants` - Analytics

### Events

Session events are emitted for real-time UI updates:
- `live:joined:{sessionId}` - Participant joined
- `live:left:{sessionId}` - Participant left
- `live:speaking:{sessionId}` - Speaking indicators (from audio worker)
- `live:audio:{sessionId}` - Mixed audio stream (from Rust worker)

### LiveWidget

Browser widget for the live UI:
- Participant grid with avatars/video
- Mic/camera/screen share toggles
- Join/leave controls
- Speaking indicators

## Future Work

- Connect to `streaming-core` Rust worker for audio mixing
- Video relay (SFU pattern)
- Screen share handling
- Integration with chat widget as overlay bar
