use tauri::{command, AppHandle, Runtime};

use crate::models::*;
use crate::AudioRecorderExt;
use crate::Result;

/// Start audio recording
#[command]
pub(crate) async fn start_recording<R: Runtime>(
    app: AppHandle<R>,
    config: RecordingConfig,
) -> Result<()> {
    app.audio_recorder().start_recording(config)
}

/// Stop audio recording and get the result
#[command]
pub(crate) async fn stop_recording<R: Runtime>(app: AppHandle<R>) -> Result<RecordingResult> {
    app.audio_recorder().stop_recording()
}

/// Pause audio recording
#[command]
pub(crate) async fn pause_recording<R: Runtime>(app: AppHandle<R>) -> Result<()> {
    app.audio_recorder().pause_recording()
}

/// Resume audio recording
#[command]
pub(crate) async fn resume_recording<R: Runtime>(app: AppHandle<R>) -> Result<()> {
    app.audio_recorder().resume_recording()
}

/// Get current recording status
#[command]
pub(crate) async fn get_status<R: Runtime>(app: AppHandle<R>) -> Result<RecordingStatus> {
    app.audio_recorder().get_status()
}

/// List available audio input devices
#[command]
pub(crate) async fn get_devices<R: Runtime>(app: AppHandle<R>) -> Result<AudioDevicesResponse> {
    app.audio_recorder().get_devices()
}

/// List the input channels of an audio device
#[command]
pub(crate) async fn get_channels<R: Runtime>(
    app: AppHandle<R>,
    device_id: Option<String>,
) -> Result<AudioChannelsResponse> {
    app.audio_recorder().get_channels(device_id)
}

/// Check microphone permission status
#[command]
pub(crate) async fn check_permission<R: Runtime>(app: AppHandle<R>) -> Result<PermissionStatus> {
    app.audio_recorder().check_permission()
}

/// Request microphone permission
#[command]
pub(crate) async fn request_permission<R: Runtime>(app: AppHandle<R>) -> Result<PermissionStatus> {
    app.audio_recorder().request_permission()
}
