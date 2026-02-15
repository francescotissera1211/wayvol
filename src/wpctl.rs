//! Interface to WirePlumber's wpctl CLI for volume control operations.

use std::process::Command;

use anyhow::{Context, Result};
use thiserror::Error;

#[derive(Error, Debug)]
pub enum WpctlError {
    #[error("wpctl command failed: {0}")]
    CommandFailed(String),
    #[error("failed to parse wpctl output: {0}")]
    ParseError(String),
}

/// Represents a PipeWire audio stream (sink input or source output).
#[derive(Debug, Clone, PartialEq)]
pub struct Stream {
    pub id: u32,
    pub name: String,
    pub stream_type: StreamType,
    pub volume: f64,
    pub muted: bool,
}

/// Whether a stream is playback (sink input) or capture (source output).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StreamType {
    Playback,
    Capture,
}

impl StreamType {
    pub fn as_str(&self) -> &'static str {
        match self {
            StreamType::Playback => "Playback",
            StreamType::Capture => "Capture",
        }
    }

    pub fn icon(&self) -> &'static str {
        match self {
            StreamType::Playback => "audio-volume-high-symbolic",
            StreamType::Capture => "audio-input-microphone-symbolic",
        }
    }
}

/// A PipeWire device (sink or source).
#[derive(Debug, Clone, PartialEq)]
pub struct Device {
    pub id: u32,
    pub name: String,
    pub is_default: bool,
    /// Whether the device port is available (plugged in).
    /// None means availability is unknown (assume available).
    pub available: Option<bool>,
}

/// The type of device section we're parsing.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DeviceType {
    Sink,
    Source,
}

/// Volume info returned by `wpctl get-volume`.
#[derive(Debug, Clone, PartialEq)]
pub struct VolumeInfo {
    pub volume: f64,
    pub muted: bool,
}

/// Run a wpctl command and return stdout.
fn run_wpctl(args: &[&str]) -> Result<String> {
    let output = Command::new("wpctl")
        .args(args)
        .output()
        .context("failed to execute wpctl")?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(WpctlError::CommandFailed(format!(
            "wpctl {} failed: {}",
            args.join(" "),
            stderr
        ))
        .into());
    }

    Ok(String::from_utf8_lossy(&output.stdout).into_owned())
}

/// Parse `wpctl get-volume <id>` output.
/// Format: "Volume: 0.75" or "Volume: 0.75 [MUTED]"
pub fn parse_volume(output: &str) -> Result<VolumeInfo> {
    let line = output.trim();
    let rest = line
        .strip_prefix("Volume: ")
        .ok_or_else(|| WpctlError::ParseError(format!("unexpected volume format: {line}")))?;

    let muted = rest.contains("[MUTED]");
    let vol_str = rest.split_whitespace().next().ok_or_else(|| {
        WpctlError::ParseError(format!("no volume value in: {line}"))
    })?;

    let volume: f64 = vol_str
        .parse()
        .map_err(|e| WpctlError::ParseError(format!("invalid volume number '{vol_str}': {e}")))?;

    Ok(VolumeInfo { volume, muted })
}

/// Get volume info for a stream by ID.
pub fn get_volume(id: u32) -> Result<VolumeInfo> {
    let output = run_wpctl(&["get-volume", &id.to_string()])?;
    parse_volume(&output)
}

/// Set volume for a stream. Level is a float (0.0 to 1.5).
pub fn set_volume(id: u32, level: f64) -> Result<()> {
    let level = level.clamp(0.0, 1.5);
    run_wpctl(&["set-volume", &id.to_string(), &format!("{level:.2}")])?;
    Ok(())
}

/// Toggle mute for a stream.
pub fn toggle_mute(id: u32) -> Result<()> {
    run_wpctl(&["set-mute", &id.to_string(), "toggle"])?;
    Ok(())
}

/// Set a device as the default sink or source.
pub fn set_default(id: u32) -> Result<()> {
    run_wpctl(&["set-default", &id.to_string()])?;
    Ok(())
}

/// Parse the `wpctl status` output to extract streams.
///
/// Handles two wpctl output formats:
/// - **Old format:** Separate "Sink Inputs:" and "Source Outputs:" sections
///   with `[vol: X.XX]` on each line.
/// - **New format (PipeWire 1.4+):** Single "Streams:" section under Audio,
///   with parent entries (`87. RHVoice`) and child channel-routing lines
///   (`88. output_FL > Headphones:playback_FL [active]`). Only parent
///   entries are streams; child lines (containing ">") are skipped.
///   Volume is not inline — callers should use `get_volume(id)`.
pub fn parse_streams(status_output: &str) -> Vec<Stream> {
    let mut streams = Vec::new();
    let mut current_section: Option<StreamType> = None;
    let mut in_streams_section = false;
    let mut in_audio = false;

    for line in status_output.lines() {
        let trimmed = line.trim();

        // Track top-level sections — only parse streams under Audio.
        if trimmed == "Audio" {
            in_audio = true;
            continue;
        }
        if trimmed == "Video" || trimmed == "Settings" {
            in_audio = false;
            current_section = None;
            in_streams_section = false;
            continue;
        }

        // Detect section headers
        if trimmed.contains("Sink Inputs:") {
            current_section = Some(StreamType::Playback);
            in_streams_section = false;
            continue;
        }
        if trimmed.contains("Source Outputs:") {
            current_section = Some(StreamType::Capture);
            in_streams_section = false;
            continue;
        }

        // Check for tree-drawn section headers
        {
            let stripped = trimmed
                .trim_start_matches('│')
                .trim_start_matches('├')
                .trim_start_matches('└')
                .trim_start_matches('─')
                .trim();

            // "Streams:" section (PipeWire 1.4+ format) — only under Audio
            if stripped == "Streams:" {
                if in_audio {
                    current_section = Some(StreamType::Playback);
                    in_streams_section = true;
                }
                continue;
            }

            // Any other section header resets
            if stripped.ends_with(':') && !stripped.contains('.')
                && stripped != "Sink Inputs:" && stripped != "Source Outputs:"
            {
                current_section = None;
                in_streams_section = false;
                continue;
            }
        }

        if let Some(stream_type) = current_section {
            // In the new Streams section, child lines show channel routing
            // (e.g. "88. output_FL > Headphones:playback_FL [active]").
            // Skip those — we only want the parent stream entries.
            if in_streams_section && line.contains('>') {
                continue;
            }

            if let Some(stream) = parse_stream_line(trimmed, stream_type) {
                streams.push(stream);
            }
        }
    }

    streams
}

/// Parse a single stream line from wpctl status.
/// Format examples:
///  │  *   47. Firefox                          [vol: 0.75]
///  │      48. Discord                          [vol: 1.00 MUTED]
///  │  *   52. mpv Media Player                 [vol: 0.50]
fn parse_stream_line(line: &str, stream_type: StreamType) -> Option<Stream> {
    // Strip tree-drawing characters
    let cleaned = line
        .replace(['│', '├', '└', '─'], "");
    let cleaned = cleaned.trim();

    // Skip empty lines and section headers
    if cleaned.is_empty() || cleaned.ends_with(':') {
        return None;
    }

    // Remove leading asterisk (default indicator)
    let cleaned = cleaned.trim_start_matches('*').trim();

    // Extract ID: starts with a number followed by a period
    let dot_pos = cleaned.find('.')?;
    let id_str = cleaned[..dot_pos].trim();
    let id: u32 = id_str.parse().ok()?;

    let rest = cleaned[dot_pos + 1..].trim();

    // Extract volume info from brackets
    let (name, volume, muted) = if let Some(vol_start) = rest.find("[vol:") {
        let name = rest[..vol_start].trim().to_string();
        let vol_section = &rest[vol_start..];
        let vol_end = vol_section.find(']').unwrap_or(vol_section.len());
        let vol_inner = &vol_section[5..vol_end].trim(); // skip "[vol:"

        let muted = vol_inner.contains("MUTED");
        let vol_str = vol_inner.split_whitespace().next().unwrap_or("1.0");
        let volume: f64 = vol_str.parse().unwrap_or(1.0);

        (name, volume, muted)
    } else {
        // No volume info — default to 1.0
        (rest.to_string(), 1.0, false)
    };

    if name.is_empty() {
        return None;
    }

    Some(Stream {
        id,
        name,
        stream_type,
        volume,
        muted,
    })
}

/// Parse `wpctl status` output for devices (sinks or sources).
pub fn parse_devices(status_output: &str, device_type: DeviceType) -> Vec<Device> {
    let mut devices = Vec::new();
    let mut in_section = false;

    let section_header = match device_type {
        DeviceType::Sink => "Sinks:",
        DeviceType::Source => "Sources:",
    };

    for line in status_output.lines() {
        let trimmed = line.trim();

        if trimmed.contains(section_header) {
            in_section = true;
            continue;
        }

        // End of section: blank line or new section header
        if in_section && trimmed.is_empty() {
            break;
        }
        if in_section
            && trimmed.ends_with(':')
            && !trimmed.starts_with('│')
            && !trimmed.contains('.')
        {
            break;
        }

        if in_section {
            if let Some(device) = parse_device_line(trimmed) {
                devices.push(device);
            }
        }
    }

    devices
}

/// Parse a device line from wpctl status.
/// Format:  │  *   46. Built-in Audio Analog Stereo [vol: 0.60]
fn parse_device_line(line: &str) -> Option<Device> {
    let cleaned = line
        .replace(['│', '├', '└', '─'], "");
    let cleaned = cleaned.trim();

    if cleaned.is_empty() || cleaned.ends_with(':') {
        return None;
    }

    let is_default = cleaned.contains('*');
    let cleaned = cleaned.trim_start_matches('*').trim();

    let dot_pos = cleaned.find('.')?;
    let id_str = cleaned[..dot_pos].trim();
    let id: u32 = id_str.parse().ok()?;

    let rest = cleaned[dot_pos + 1..].trim();

    // Strip volume info from name
    let name = if let Some(bracket_pos) = rest.find('[') {
        rest[..bracket_pos].trim().to_string()
    } else {
        rest.to_string()
    };

    if name.is_empty() {
        return None;
    }

    Some(Device {
        id,
        name,
        is_default,
        available: None,
    })
}

/// Check port availability for sinks/sources using pactl.
///
/// `wpctl status` doesn't expose port availability, but `pactl list sinks`
/// does. Filters out unplugged HDMI/DisplayPort outputs etc.
pub fn enrich_device_availability(devices: &mut [Device], device_type: DeviceType) {
    let cmd = match device_type {
        DeviceType::Sink => "sinks",
        DeviceType::Source => "sources",
    };

    let output = match Command::new("pactl")
        .args(["list", cmd])
        .output()
    {
        Ok(o) if o.status.success() => String::from_utf8_lossy(&o.stdout).into_owned(),
        _ => return,
    };

    // Parse pactl output: map sink/source IDs to port availability.
    // Port lines: [Out] HDMI3: ... (type: HDMI, ..., not available)
    let mut current_id: Option<u32> = None;
    let mut in_ports = false;

    for line in output.lines() {
        let trimmed = line.trim();

        if let Some(rest) = trimmed.strip_prefix("Sink #")
            .or_else(|| trimmed.strip_prefix("Source #"))
        {
            current_id = rest.trim().parse().ok();
            in_ports = false;
            continue;
        }

        if trimmed == "Ports:" {
            in_ports = true;
            continue;
        }

        if in_ports && !trimmed.starts_with('[') {
            in_ports = false;
        }

        if in_ports && trimmed.starts_with('[') {
            if let Some(id) = current_id {
                if let Some(dev) = devices.iter_mut().find(|d| d.id == id) {
                    if trimmed.contains("not available") {
                        dev.available = Some(false);
                    } else if trimmed.contains("available") {
                        dev.available = Some(true);
                    }
                }
            }
        }
    }
}

/// Fetch current wpctl status output.
pub fn get_status() -> Result<String> {
    run_wpctl(&["status"])
}

/// Get the application name for a stream by inspecting it.
/// Falls back to the stream name if inspect fails.
pub fn get_app_name(id: u32) -> Option<String> {
    let output = run_wpctl(&["inspect", &id.to_string()]).ok()?;
    for line in output.lines() {
        let trimmed = line.trim();
        if trimmed.starts_with("application.name") {
            let val = trimmed.split('=').nth(1)?.trim().trim_matches('"');
            if !val.is_empty() {
                return Some(val.to_string());
            }
        }
    }
    None
}

/// Generate an accessible label for a stream.
pub fn accessible_label(stream: &Stream) -> String {
    let type_str = stream.stream_type.as_str();
    let pct = (stream.volume * 100.0).round() as i32;
    let mute_str = if stream.muted { ", muted" } else { "" };
    format!("{} {} stream, {}%{}", stream.name, type_str, pct, mute_str)
}

/// Generate an accessible label for a volume slider.
pub fn slider_accessible_label(stream: &Stream) -> String {
    let pct = (stream.volume * 100.0).round() as i32;
    format!("{} volume, {}%", stream.name, pct)
}

/// Generate an accessible label for a mute button.
pub fn mute_button_label(stream: &Stream) -> String {
    if stream.muted {
        format!("Unmute {}", stream.name)
    } else {
        format!("Mute {}", stream.name)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_volume_normal() {
        let info = parse_volume("Volume: 0.75").unwrap();
        assert!((info.volume - 0.75).abs() < f64::EPSILON);
        assert!(!info.muted);
    }

    #[test]
    fn test_parse_volume_muted() {
        let info = parse_volume("Volume: 0.50 [MUTED]").unwrap();
        assert!((info.volume - 0.50).abs() < f64::EPSILON);
        assert!(info.muted);
    }

    #[test]
    fn test_parse_volume_full() {
        let info = parse_volume("Volume: 1.00").unwrap();
        assert!((info.volume - 1.0).abs() < f64::EPSILON);
        assert!(!info.muted);
    }

    #[test]
    fn test_parse_volume_boost() {
        let info = parse_volume("Volume: 1.50").unwrap();
        assert!((info.volume - 1.50).abs() < f64::EPSILON);
    }

    #[test]
    fn test_parse_volume_zero() {
        let info = parse_volume("Volume: 0.00 [MUTED]").unwrap();
        assert!((info.volume - 0.0).abs() < f64::EPSILON);
        assert!(info.muted);
    }

    #[test]
    fn test_parse_volume_invalid() {
        assert!(parse_volume("Something else").is_err());
        assert!(parse_volume("Volume: notanumber").is_err());
    }

    #[test]
    fn test_parse_stream_line_playback() {
        let stream =
            parse_stream_line("│  *   47. Firefox                 [vol: 0.75]", StreamType::Playback)
                .unwrap();
        assert_eq!(stream.id, 47);
        assert_eq!(stream.name, "Firefox");
        assert!((stream.volume - 0.75).abs() < f64::EPSILON);
        assert!(!stream.muted);
        assert_eq!(stream.stream_type, StreamType::Playback);
    }

    #[test]
    fn test_parse_stream_line_muted() {
        let stream = parse_stream_line(
            "│      48. Discord          [vol: 1.00 MUTED]",
            StreamType::Playback,
        )
        .unwrap();
        assert_eq!(stream.id, 48);
        assert_eq!(stream.name, "Discord");
        assert!(stream.muted);
    }

    #[test]
    fn test_parse_stream_line_capture() {
        let stream = parse_stream_line(
            "│      52. OBS Studio       [vol: 0.80]",
            StreamType::Capture,
        )
        .unwrap();
        assert_eq!(stream.id, 52);
        assert_eq!(stream.stream_type, StreamType::Capture);
    }

    #[test]
    fn test_parse_stream_line_empty() {
        assert!(parse_stream_line("│", StreamType::Playback).is_none());
        assert!(parse_stream_line("", StreamType::Playback).is_none());
    }

    #[test]
    fn test_parse_stream_line_section_header() {
        assert!(parse_stream_line("│  Sink Inputs:", StreamType::Playback).is_none());
    }

    #[test]
    fn test_parse_streams_full() {
        let status = r#"
Audio
 ├─ Devices:
 │      40. Built-in Audio
 │
 ├─ Sinks:
 │  *   46. Built-in Audio Analog Stereo           [vol: 0.60]
 │
 ├─ Sink Inputs:
 │  *   47. Firefox                                 [vol: 0.75]
 │      48. mpv Media Player                        [vol: 1.00]
 │
 ├─ Sources:
 │  *   49. Built-in Audio Analog Stereo            [vol: 1.00]
 │
 ├─ Source Outputs:
 │      50. Discord                                 [vol: 0.80]
 │
 └─ Filters:
"#;
        let streams = parse_streams(status);
        assert_eq!(streams.len(), 3);
        assert_eq!(streams[0].name, "Firefox");
        assert_eq!(streams[0].stream_type, StreamType::Playback);
        assert_eq!(streams[1].name, "mpv Media Player");
        assert_eq!(streams[2].name, "Discord");
        assert_eq!(streams[2].stream_type, StreamType::Capture);
    }

    #[test]
    fn test_parse_streams_empty() {
        let status = "Audio\n ├─ Sink Inputs:\n │\n";
        let streams = parse_streams(status);
        assert!(streams.is_empty());
    }

    #[test]
    fn test_parse_streams_new_format() {
        // PipeWire 1.4+ format: "Streams:" section with channel routing lines
        let status = r#"
Audio
 ├─ Devices:
 │      43. Blue Snowball                       [alsa]
 │
 ├─ Sinks:
 │  *   57. Headphones                          [vol: 0.32]
 │
 ├─ Sources:
 │  *   58. Stereo Microphone                   [vol: 0.41]
 │
 ├─ Filters:
 │
 └─ Streams:
        87. RHVoice
             88. output_FL       > Headphones:playback_FL	[active]
             89. output_FR       > Headphones:playback_FR	[active]
        93. speech-dispatcher-dummy
             96. output_FR       > Headphones:playback_FR	[paused]
             97. output_FL       > Headphones:playback_FL	[paused]

Video
 ├─ Devices:
 │      52. HP Wide Vision HD Camera            [v4l2]
 │
 └─ Streams:

Settings
"#;
        let streams = parse_streams(status);
        assert_eq!(streams.len(), 2);
        assert_eq!(streams[0].id, 87);
        assert_eq!(streams[0].name, "RHVoice");
        assert_eq!(streams[0].stream_type, StreamType::Playback);
        assert_eq!(streams[1].id, 93);
        assert_eq!(streams[1].name, "speech-dispatcher-dummy");
        assert_eq!(streams[1].stream_type, StreamType::Playback);
    }

    #[test]
    fn test_parse_streams_new_format_no_video_leak() {
        // Ensure Video Streams section doesn't leak into audio results
        let status = r#"
Audio
 └─ Streams:
        87. Firefox

Video
 └─ Streams:
        99. OBS Virtual Camera

Settings
"#;
        let streams = parse_streams(status);
        assert_eq!(streams.len(), 1);
        assert_eq!(streams[0].name, "Firefox");
    }

    #[test]
    fn test_parse_devices_sinks() {
        let status = r#"
Audio
 ├─ Sinks:
 │  *   46. Built-in Audio Analog Stereo           [vol: 0.60]
 │      55. HDMI Output                            [vol: 1.00]
 │
 ├─ Sink Inputs:
"#;
        let devices = parse_devices(status, DeviceType::Sink);
        assert_eq!(devices.len(), 2);
        assert_eq!(devices[0].id, 46);
        assert!(devices[0].is_default);
        assert_eq!(devices[1].id, 55);
        assert!(!devices[1].is_default);
    }

    #[test]
    fn test_parse_devices_sources() {
        let status = r#"
 ├─ Sources:
 │  *   49. Built-in Audio Analog Stereo            [vol: 1.00]
 │
 ├─ Source Outputs:
"#;
        let devices = parse_devices(status, DeviceType::Source);
        assert_eq!(devices.len(), 1);
        assert_eq!(devices[0].id, 49);
        assert!(devices[0].is_default);
    }

    #[test]
    fn test_parse_device_line_default() {
        let device = parse_device_line("│  *   46. Built-in Audio Analog Stereo  [vol: 0.60]").unwrap();
        assert_eq!(device.id, 46);
        assert_eq!(device.name, "Built-in Audio Analog Stereo");
        assert!(device.is_default);
    }

    #[test]
    fn test_parse_device_line_not_default() {
        let device = parse_device_line("│      55. HDMI Output  [vol: 1.00]").unwrap();
        assert_eq!(device.id, 55);
        assert!(!device.is_default);
    }

    #[test]
    fn test_stream_type_as_str() {
        assert_eq!(StreamType::Playback.as_str(), "Playback");
        assert_eq!(StreamType::Capture.as_str(), "Capture");
    }

    #[test]
    fn test_stream_type_icon() {
        assert_eq!(StreamType::Playback.icon(), "audio-volume-high-symbolic");
        assert_eq!(StreamType::Capture.icon(), "audio-input-microphone-symbolic");
    }

    #[test]
    fn test_accessible_label() {
        let stream = Stream {
            id: 47,
            name: "Firefox".to_string(),
            stream_type: StreamType::Playback,
            volume: 0.75,
            muted: false,
        };
        assert_eq!(accessible_label(&stream), "Firefox Playback stream, 75%");
    }

    #[test]
    fn test_accessible_label_muted() {
        let stream = Stream {
            id: 48,
            name: "Discord".to_string(),
            stream_type: StreamType::Capture,
            volume: 0.50,
            muted: true,
        };
        assert_eq!(
            accessible_label(&stream),
            "Discord Capture stream, 50%, muted"
        );
    }

    #[test]
    fn test_slider_accessible_label() {
        let stream = Stream {
            id: 47,
            name: "Firefox".to_string(),
            stream_type: StreamType::Playback,
            volume: 0.75,
            muted: false,
        };
        assert_eq!(slider_accessible_label(&stream), "Firefox volume, 75%");
    }

    #[test]
    fn test_mute_button_label() {
        let stream = Stream {
            id: 47,
            name: "Firefox".to_string(),
            stream_type: StreamType::Playback,
            volume: 0.75,
            muted: false,
        };
        assert_eq!(mute_button_label(&stream), "Mute Firefox");

        let muted_stream = Stream {
            muted: true,
            ..stream
        };
        assert_eq!(mute_button_label(&muted_stream), "Unmute Firefox");
    }

    #[test]
    fn test_volume_percentage_rounding() {
        let stream = Stream {
            id: 1,
            name: "Test".to_string(),
            stream_type: StreamType::Playback,
            volume: 0.333,
            muted: false,
        };
        assert_eq!(slider_accessible_label(&stream), "Test volume, 33%");
    }

    #[test]
    fn test_volume_boost_label() {
        let stream = Stream {
            id: 1,
            name: "Boost".to_string(),
            stream_type: StreamType::Playback,
            volume: 1.5,
            muted: false,
        };
        assert_eq!(slider_accessible_label(&stream), "Boost volume, 150%");
    }
}
