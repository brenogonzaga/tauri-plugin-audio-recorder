use serde::de::DeserializeOwned;
use tauri::{
    plugin::{PluginApi, PluginHandle},
    AppHandle, Runtime,
};

use crate::models::*;

#[cfg(target_os = "ios")]
tauri::ios_plugin_binding!(init_plugin_audio_recorder);

// initializes the Kotlin or Swift plugin classes
pub fn init<R: Runtime, C: DeserializeOwned>(
    _app: &AppHandle<R>,
    api: PluginApi<R, C>,
) -> crate::Result<AudioRecorder<R>> {
    #[cfg(target_os = "android")]
    let handle = api.register_android_plugin("io.affex.audio_recorder", "AudioRecorderPlugin")?;
    #[cfg(target_os = "ios")]
    let handle = api.register_ios_plugin(init_plugin_audio_recorder)?;
    Ok(AudioRecorder(handle))
}

/// Access to the audio-recorder APIs.
pub struct AudioRecorder<R: Runtime>(PluginHandle<R>);

impl<R: Runtime> AudioRecorder<R> {
    /// Start recording audio
    pub fn start_recording(&self, config: RecordingConfig) -> crate::Result<()> {
        self.0
            .run_mobile_plugin("startRecording", config)
            .map_err(Into::into)
    }

    /// Stop recording and return the result
    pub fn stop_recording(&self) -> crate::Result<RecordingResult> {
        self.0
            .run_mobile_plugin("stopRecording", ())
            .map_err(Into::into)
    }

    /// Pause recording
    pub fn pause_recording(&self) -> crate::Result<()> {
        self.0
            .run_mobile_plugin("pauseRecording", ())
            .map_err(Into::into)
    }

    /// Resume recording
    pub fn resume_recording(&self) -> crate::Result<()> {
        self.0
            .run_mobile_plugin("resumeRecording", ())
            .map_err(Into::into)
    }

    /// Get current recording status
    pub fn get_status(&self) -> crate::Result<RecordingStatus> {
        self.0
            .run_mobile_plugin("getStatus", ())
            .map_err(Into::into)
    }

    /// List available audio input devices
    pub fn get_devices(&self) -> crate::Result<AudioDevicesResponse> {
        self.0
            .run_mobile_plugin("getDevices", ())
            .map_err(Into::into)
    }

    /// List the input channels of an audio device
    pub fn get_channels(&self, _device_id: Option<String>) -> crate::Result<AudioChannelsResponse> {
        // Mobile records the active audio route; channel selection is desktop only
        Ok(AudioChannelsResponse { channels: vec![] })
    }

    /// Check microphone permission
    pub fn check_permission(&self) -> crate::Result<PermissionStatus> {
        self.0
            .run_mobile_plugin("checkPermission", ())
            .map_err(Into::into)
    }

    /// Request microphone permission
    pub fn request_permission(&self) -> crate::Result<PermissionStatus> {
        self.0
            .run_mobile_plugin("requestPermission", ())
            .map_err(Into::into)
    }
}
