# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [0.1.2] - 2026-07-07

### Fixed

- macOS: `stopRecording` could leave the input device open indefinitely when recording from a manually selected device (`deviceId`) — the OS mic-in-use indicator and USB mic record LEDs stayed on, and leaked stream callbacks could corrupt reported durations. The stream is now explicitly paused before being dropped, in both the stop and shutdown paths ([#3](https://github.com/brenogonzaga/tauri-plugin-audio-recorder/pull/3))

## [0.1.1] - 2026-07-07

### Added

- `deviceId` field in `RecordingConfig` to record from a specific input device instead of the system default ([#2](https://github.com/brenogonzaga/tauri-plugin-audio-recorder/pull/2))

## [0.1.0] - 2025-12

### Added

- Initial release of Audio Recorder plugin for Tauri 2.x
- Cross-platform support (Windows, macOS, Linux, iOS, Android)
- WAV recording with 16-bit PCM encoding
- Quality presets: low (16kHz mono), medium (44.1kHz mono), high (48kHz stereo)
- Pause and resume functionality
- Real-time duration tracking via `getStatus()`
- Audio device enumeration via `getDevices()`
- Permission checking and requesting APIs
- Max duration limit support
- TypeScript API with full type definitions

### Desktop Implementation

- Uses `cpal` crate for cross-platform audio input
- Uses `hound` crate for WAV file encoding
- Supports multiple sample formats (F32, I16, U16)
- Thread-safe recording state management

### iOS Implementation

- Uses AVAudioRecorder with Linear PCM format
- Proper audio session management
- Native permission handling via AVAudioSession

### Android Implementation

### Requirements

- Tauri: 2.9+
- Rust: 1.77+
- Android SDK: 24+ (Android 7.0+)
- iOS: 14.0+
- Uses MediaRecorder API
- Pause/Resume support on Android N+
- Runtime permission handling for RECORD_AUDIO
