use serde::{Deserialize, Serialize};

/// Audio format for recording
#[derive(Debug, Clone, Copy, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum AudioFormat {
    #[default]
    Wav,
    // Future: Mp3, Ogg, etc.
}

/// Audio quality preset
#[derive(Debug, Clone, Copy, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum AudioQuality {
    /// 16kHz, 16-bit mono - Good for speech
    Low,
    #[default]
    /// 44.1kHz, 16-bit mono - Standard quality
    Medium,
    /// 48kHz, 16-bit stereo - High quality
    High,
}

impl AudioQuality {
    pub fn sample_rate(&self) -> u32 {
        match self {
            AudioQuality::Low => 16000,
            AudioQuality::Medium => 44100,
            AudioQuality::High => 48000,
        }
    }

    pub fn channels(&self) -> u16 {
        match self {
            AudioQuality::Low | AudioQuality::Medium => 1,
            AudioQuality::High => 2,
        }
    }
}

/// Recording configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RecordingConfig {
    /// Output file path (without extension)
    pub output_path: String,
    /// Audio format (default: wav)
    #[serde(default)]
    pub format: AudioFormat,
    /// Audio quality preset (default: medium)
    #[serde(default)]
    pub quality: AudioQuality,
    /// Maximum recording duration in seconds (0 = unlimited)
    #[serde(default)]
    pub max_duration: u32,
    /// Input device to record from, identified by the `id` returned from
    /// `get_devices` (default: system default input device).
    /// Desktop only; ignored on mobile.
    #[serde(default)]
    pub device_id: Option<String>,
}

/// Recording state
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum RecordingState {
    Idle,
    Recording,
    Paused,
}

impl Default for RecordingState {
    fn default() -> Self {
        Self::Idle
    }
}

/// Recording status response
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RecordingStatus {
    /// Current state
    pub state: RecordingState,
    /// Duration recorded so far (in milliseconds)
    pub duration_ms: u64,
    /// Output file path (if recording)
    pub output_path: Option<String>,
}

/// Recording completed response
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RecordingResult {
    /// Full path to the recorded file
    pub file_path: String,
    /// Duration in milliseconds
    pub duration_ms: u64,
    /// File size in bytes
    pub file_size: u64,
    /// Sample rate used
    pub sample_rate: u32,
    /// Number of channels
    pub channels: u16,
}

/// Audio input device
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AudioDevice {
    /// Device identifier
    pub id: String,
    /// Human-readable device name
    pub name: String,
    /// Whether this is the default device
    pub is_default: bool,
}

/// List of available audio devices
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AudioDevicesResponse {
    pub devices: Vec<AudioDevice>,
}

/// Permission status
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PermissionStatus {
    /// Whether microphone permission is granted
    pub granted: bool,
    /// Whether we can ask for permission
    pub can_request: bool,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_audio_quality_sample_rate() {
        assert_eq!(AudioQuality::Low.sample_rate(), 16000);
        assert_eq!(AudioQuality::Medium.sample_rate(), 44100);
        assert_eq!(AudioQuality::High.sample_rate(), 48000);
    }

    #[test]
    fn test_audio_quality_channels() {
        assert_eq!(AudioQuality::Low.channels(), 1);
        assert_eq!(AudioQuality::Medium.channels(), 1);
        assert_eq!(AudioQuality::High.channels(), 2);
    }

    #[test]
    fn test_recording_state_serialization() {
        assert_eq!(
            serde_json::to_string(&RecordingState::Idle).unwrap(),
            "\"idle\""
        );
        assert_eq!(
            serde_json::to_string(&RecordingState::Recording).unwrap(),
            "\"recording\""
        );
        assert_eq!(
            serde_json::to_string(&RecordingState::Paused).unwrap(),
            "\"paused\""
        );
    }

    #[test]
    fn test_recording_config_deserialization() {
        let json = r#"{
            "outputPath": "/path/to/recording",
            "quality": "high"
        }"#;

        let config: RecordingConfig = serde_json::from_str(json).unwrap();
        assert_eq!(config.output_path, "/path/to/recording");
        assert!(matches!(config.quality, AudioQuality::High));
        assert!(matches!(config.format, AudioFormat::Wav));
        assert_eq!(config.max_duration, 0);
    }

    #[test]
    fn test_recording_config_defaults() {
        let json = r#"{"outputPath": "/test"}"#;
        let config: RecordingConfig = serde_json::from_str(json).unwrap();

        assert!(matches!(config.format, AudioFormat::Wav));
        assert!(matches!(config.quality, AudioQuality::Medium));
        assert_eq!(config.max_duration, 0);
        assert_eq!(config.device_id, None);
    }

    #[test]
    fn test_recording_config_device_id() {
        let json = r#"{
            "outputPath": "/test",
            "deviceId": "Digta SonicMic 3"
        }"#;
        let config: RecordingConfig = serde_json::from_str(json).unwrap();

        assert_eq!(config.device_id.as_deref(), Some("Digta SonicMic 3"));
    }
}
