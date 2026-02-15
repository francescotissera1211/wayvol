# wayvol

An accessible GTK4 volume mixer for PipeWire on Wayland.

wayvol gives you per-application volume control with full keyboard navigation and screen reader support. It uses WirePlumber's `wpctl` for reliable volume management and `pw-dump` for real-time stream monitoring.

## Features

- **Per-app volume control** — individual sliders for every audio stream (0–150%)
- **Mute toggles** — per-stream mute/unmute
- **Device switching** — output and input device selection via dropdowns
- **Live updates** — streams appear and disappear in real time as apps start and stop audio
- **Unplugged device filtering** — HDMI/DisplayPort outputs that aren't connected are hidden automatically
- **Accessible** — full keyboard navigation, AT-SPI labels on every widget, screen reader announcements for stream changes

## Requirements

- GTK4 (4.14+)
- libadwaita (1.4+)
- WirePlumber (provides `wpctl`)
- PipeWire (provides `pw-dump`)
- PulseAudio compatibility layer (provides `pactl`, used for device availability — ships with PipeWire on most distros)

### Fedora

```bash
sudo dnf install gtk4-devel libadwaita-devel
```

WirePlumber and PipeWire are installed by default on Fedora.

### Other distros

Install the equivalent GTK4 and libadwaita development packages. WirePlumber must be the active PipeWire session manager.

## Building

```bash
git clone https://github.com/destructatron/wayvol.git
cd wayvol
cargo build --release
```

The binary will be at `target/release/wayvol`.

## Usage

```bash
wayvol
```

Navigate with Tab and arrow keys. Sliders respond to Left/Right arrows. Press Space to toggle mute buttons.

## How it works

- **Streams:** Parsed from `wpctl status` (supports both legacy Sink Inputs/Source Outputs format and PipeWire 1.4+ Streams format)
- **Volumes:** Read and written via `wpctl get-volume` / `wpctl set-volume`
- **Monitoring:** `pw-dump --monitor` watches for PipeWire state changes in real time, with debounced refresh and structural diffing to avoid unnecessary UI rebuilds
- **Device availability:** `pactl list sinks/sources` provides port availability to filter out unplugged outputs

## Accessibility

wayvol is designed for screen reader users from the ground up:

- Every control has an AT-SPI accessible label
- Stream rows use `AdwActionRow` for native libadwaita keyboard navigation
- Volume changes are reflected in accessible properties
- New streams appearing or disappearing are announced
- No focus traps — Tab moves between controls predictably

Tested with Orca on Niri (Wayland tiling compositor).

## Roadmap

- [ ] Audio card profile switching
- [ ] Per-stream routing (move stream to different output)
- [ ] System tray integration

## License

MIT
