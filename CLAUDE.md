# CLAUDE.md - wayvol

## Project Overview
wayvol is an accessible GTK4 PipeWire volume mixer for Wayland. It shows per-application audio streams with volume sliders, designed for screen reader users.

## Architecture Reference
This project is part of an accessible Wayland desktop stack. See ~/pw-audioshare for reference on:
- PipeWire thread pattern (separate thread with async-channel for events)
- GTK4 + libadwaita patterns
- AT-SPI accessibility patterns

## Requirements

### Core Features
1. **List all PipeWire audio streams** — Show playback and capture streams (application audio)
2. **Per-stream volume control** — Slider for each stream (0-150%, allowing boost)
3. **Default sink/source selection** — Dropdown or list to switch default output/input device
4. **Mute toggle** — Per-stream mute button
5. **Auto-update** — Streams appear/disappear in real-time as apps start/stop audio
6. **Stream identification** — Show application name, icon where available

### Accessibility (CRITICAL)
- Every widget MUST have AT-SPI accessible labels
- Volume sliders must announce their current value when adjusted (percentage)
- Stream names must be readable by Orca screen reader
- Full keyboard navigation (Tab between streams, arrow keys for sliders)
- Announce when new streams appear or disappear
- Mute state must be announced

### Technical Stack
- **Language:** Rust
- **GUI:** gtk4-rs (0.9+) with libadwaita (0.7+) for modern styling
- **PipeWire:** Use `wpctl` (WirePlumber CLI) for volume control operations OR pipewire-rs crate
  - `wpctl status` — list streams
  - `wpctl get-volume <id>` — get volume
  - `wpctl set-volume <id> <level>` — set volume (level as decimal, e.g. 0.75)
  - `wpctl set-mute <id> toggle` — toggle mute
  - `wpctl inspect <id>` — get details
  - Using wpctl is simpler and more reliable than raw PipeWire bindings for volume control
- **Monitoring:** `pw-dump --monitor` or polling for stream changes
- **Error handling:** anyhow + thiserror
- **Logging:** log + env_logger

### UI Layout
```
┌─────────────────────────────────┐
│ wayvol - Volume Mixer           │
├─────────────────────────────────┤
│ Output Device: [dropdown]       │
│ Input Device:  [dropdown]       │
├─────────────────────────────────┤
│ ♪ Firefox          [====|===] 75% [M] │
│ ♪ MPV Media Player [========] 100% [M] │
│ ♪ Discord          [===|====] 50%  [M] │
│ 🎤 Microphone      [======|=] 80%  [M] │
└─────────────────────────────────┘
```

### PipeWire Volume Control Notes
- PipeWire volumes are float values where 1.0 = 100%
- Allow up to 1.5 (150%) for boost
- wpctl is part of WirePlumber which is the standard PipeWire session manager on Fedora
- Stream IDs from wpctl can change — refresh periodically or watch for changes

### Testing
- Write unit tests for volume parsing, stream identification logic
- Test mute toggle state tracking
- Test accessible label generation
- Follow the testing patterns in ~/pw-audioshare/src/pipewire/messages.rs

### Build
```bash
cargo build
cargo test
cargo clippy
```

### Dependencies
System: gtk4-devel, libadwaita-devel, pipewire-devel (optional, if using pipewire-rs)
WirePlumber must be running (standard on Fedora 43)

## Style
- Clean, idiomatic Rust
- Comprehensive error handling (no unwrap in production code, unwrap OK in tests)
- Comment complex logic
- Keep modules focused and small
