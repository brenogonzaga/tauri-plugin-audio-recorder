# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [0.2.0] - 2026-08-22

### Added

- `getChannels(deviceId?)` and `channelId` field in `RecordingConfig` to record a single channel of a multi-channel input instead of the whole stream ([#4](https://github.com/brenogonzaga/tauri-plugin-audio-recorder/pull/4))

### Removed

- **Breaking (Rust API only, not the TypeScript API):** `get_recordings_dir`, `get_plugin_subdir`, `get_cache_dir`, and `resolve_output_path` (previously exported from the crate root). None of these were ever exposed as Tauri commands, so no TypeScript/JS consumer had access to them — only a Rust crate directly depending on this one as a library could have called them. They were never called anywhere in the plugin's own recording flow, never used by the example app, and never documented in the README — unused scaffolding since the initial commit. `validate_path` is unaffected and is now actually used internally (see Fixed).

### Fixed

- **Desktop: `maxDuration` was silently ignored.** The field was accepted by `RecordingConfig` and documented as cross-platform, but nothing on desktop ever read it — recordings never auto-stopped regardless of the value. A recording now spawns a watcher that stops it after `maxDuration` seconds, matching the existing mobile behavior.
- **Desktop: `stopRecording()` could permanently brick the recorder.** If finalizing the WAV file failed (e.g. disk full), the error was returned before the recorder's internal state was reset to idle — every subsequent `startRecording()` call would then fail with "Already recording" for the rest of the process's lifetime. State is now reset regardless of whether finalizing succeeds.
- **Desktop: no protection against path traversal in `outputPath`.** A `validate_path` traversal guard existed in the codebase but was never called from the recording path. It's now applied to every user-supplied `outputPath`.
- **Desktop: a relative `outputPath` with a subdirectory (e.g. `"myfolder/myrecording"`) resolved against the process's working directory** instead of a predictable location, unlike a bare filename (which correctly falls back to the OS temp directory). Both now resolve the same way.
- **Desktop: `outputPath` ending in `.wav` got a duplicated extension** (`recording.wav.wav`). A trailing `.wav`/`.WAV` is now recognized and not doubled.
- A selected `channelId` could silently end up applied to a differently-shaped audio stream than the one it was resolved against — e.g. when the requested quality's sample rate wasn't available at the device's full channel count, negotiation would fall back to a narrower stream and either reject a valid channel or record from the wrong physical channel with no error. Channel selection now requires an exact channel-count match during negotiation and fails clearly (`InvalidChannel`) when the requested quality can't be honored on that channel.
- `device_channel_count`'s fallback path (used when a device advertises no `supported_input_configs`) could report 0 channels instead of falling back to 1, permanently blocking channel selection on such a device.
- `getStatus()` kept reporting the last recording's `outputPath` after `stopRecording()` completed, contradicting its documented "(if recording)" contract.
- **Android/iOS: quality presets (`low`/`medium`/`high`) were computed but never applied to the actual encoder** — every recording was captured at 44.1kHz mono regardless of the requested quality, while `RecordingResult` falsely reported the requested values.
- **Android: `requestPermission()` didn't wait for the user** — it resolved after a fixed 500ms guess instead of the actual dialog result. Now uses Tauri's own permission-callback mechanism (`requestPermissionForAlias`/`@PermissionCallback`), which the plugin wasn't using despite it already being available.
- **Android: `checkPermission()`'s `canRequest` was effectively always `true`**, including after a permanent denial, due to inverted boolean logic.
- **Android: `MediaRecorder` was leaked (never `.release()`d) on every start/stop error path.**
- **Android: an exception during the `maxDuration` auto-stop could crash the app** (uncaught inside a `Handler` callback).
- **iOS: recordings were saved with a `.aac` extension instead of `.m4a`**, inconsistent with Android and the documented mobile output format (same underlying encoding either way).
- **iOS: `checkPermission()`'s `canRequest` had the same inverted-logic bug** — a correctly-computed value was discarded in favor of one that ignored permanent denial.
- **iOS: `requestPermission()`'s completion handler wasn't guaranteed to run on the main thread**, unlike `startRecording()`'s handling of the same API.
- **iOS: the guard against concurrent stop calls (`isStopping`) wasn't actually atomic** — a plain check-then-set with no lock, unlike Android's `synchronized` equivalent.

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
