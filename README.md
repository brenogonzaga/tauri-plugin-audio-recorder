# Tauri Plugin Audio Recorder

Cross-platform audio recording for Tauri 2.x. Desktop captures to WAV (PCM via `cpal` + `hound`); mobile uses the native encoder so output is M4A/AAC.

## Platform Matrix

| Platform | Engine          | Output |
| -------- | --------------- | ------ |
| macOS    | CPAL            | WAV    |
| Windows  | CPAL            | WAV    |
| Linux    | CPAL            | WAV    |
| iOS      | AVAudioRecorder | M4A    |
| Android  | MediaRecorder   | M4A    |

## Installation

### Rust

```toml
[dependencies]
tauri-plugin-audio-recorder = "0.1"
```

### TypeScript

```bash
npm install tauri-plugin-audio-recorder-api
```

## Setup

```rust
fn main() {
    tauri::Builder::default()
        .plugin(tauri_plugin_audio_recorder::init())
        .run(tauri::generate_context!())
        .unwrap();
}
```

### Permissions

```json
{ "permissions": ["audio-recorder:default"] }
```

Granular:

```json
{
  "permissions": [
    "audio-recorder:allow-start-recording",
    "audio-recorder:allow-stop-recording",
    "audio-recorder:allow-pause-recording",
    "audio-recorder:allow-resume-recording",
    "audio-recorder:allow-get-status",
    "audio-recorder:allow-get-devices",
    "audio-recorder:allow-check-permission",
    "audio-recorder:allow-request-permission"
  ]
}
```

### Platform Setup

**Android** — `AndroidManifest.xml`:

```xml
<uses-permission android:name="android.permission.RECORD_AUDIO" />
```

**iOS** — `Info.plist`:

```xml
<key>NSMicrophoneUsageDescription</key>
<string>Microphone access required for recording.</string>
```

## Usage

```typescript
import {
  startRecording,
  stopRecording,
  pauseRecording,
  resumeRecording,
  getStatus,
  getDevices,
  requestPermission,
} from "tauri-plugin-audio-recorder-api";

const { granted } = await requestPermission();
if (!granted) return;

await startRecording({
  outputPath: "/path/to/recording", // extension is appended automatically
  quality: "medium",
  maxDuration: 300,
});

const status = await getStatus(); // { state, durationMs, outputPath }
await pauseRecording();
await resumeRecording();

const result = await stopRecording();
// result.filePath ends with ".wav" on desktop, ".m4a" on mobile
console.log(`${result.durationMs}ms → ${result.filePath} (${result.fileSize} bytes)`);
```

### Handling the format difference

Desktop produces WAV; mobile produces M4A. Check the extension when processing across platforms:

```typescript
const result = await stopRecording();
if (result.filePath.endsWith(".m4a")) {
  // Convert with tauri-plugin-media-toolkit if WAV is needed
}
```

### Device enumeration (desktop only)

```typescript
const { devices } = await getDevices(); // returns [] on mobile
devices.forEach(d => console.log(d.name, d.isDefault ? "(default)" : ""));
```

### Max-duration detection

`maxDuration` stops recording automatically with no callback. Poll to detect completion — and do **not** call `stopRecording()` afterward, since the recorder is already idle:

```typescript
const outputPath = "/path/to/recording";
await startRecording({ outputPath, maxDuration: 60 });

const poll = setInterval(async () => {
  const { state } = await getStatus();
  if (state === "idle") {
    clearInterval(poll);
    // File is already saved at outputPath + ".wav" (desktop) or ".m4a" (mobile)
  }
}, 1000);
```

## API Reference

- `startRecording(config)` — starts capture; throws if already recording
- `stopRecording()` → `RecordingResult` — finalises and returns file metadata
- `pauseRecording()` — Android requires API 24+ (Android 7.0+)
- `resumeRecording()`
- `getStatus()` → `{ state, durationMs, outputPath }`
- `getDevices()` → `{ devices }` — desktop only, empty on mobile
- `checkPermission()` / `requestPermission()` → `{ granted, canRequest }`

### RecordingConfig

```typescript
interface RecordingConfig {
  outputPath: string;                   // without extension
  quality?: "low" | "medium" | "high"; // 16kHz mono | 44.1kHz mono | 48kHz stereo
  maxDuration?: number;                 // seconds, 0 = unlimited
}
```

### RecordingResult

```typescript
interface RecordingResult {
  filePath: string;   // full path with extension
  durationMs: number;
  fileSize: number;
  sampleRate: number;
  channels: number;
}
```

### Quality Presets

| Preset   | Sample Rate | Channels |
| -------- | ----------- | -------- |
| `low`    | 16 kHz      | Mono     |
| `medium` | 44.1 kHz    | Mono     |
| `high`   | 48 kHz      | Stereo   |

## Troubleshooting

**Permission denied** — iOS: verify `NSMicrophoneUsageDescription` in Info.plist. Android: verify `RECORD_AUDIO` in AndroidManifest. Always call `requestPermission()` before `startRecording()`.

**Pause/Resume on Android** — requires Android N (API 24+). Catch the error and fall back to stop/restart on older devices.

**Empty or tiny output file** — the output path's parent directory doesn't exist, or recording was stopped immediately. Check that `result.durationMs > 100` and `result.fileSize > 1000`.

## License

MIT
