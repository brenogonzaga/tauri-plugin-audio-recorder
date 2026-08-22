use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use cpal::{SampleFormat, StreamConfig};
use hound::{WavSpec, WavWriter};
use serde::de::DeserializeOwned;
use std::fs::File;
use std::io::BufWriter;
use std::marker::PhantomData;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{mpsc, Arc, Mutex};
use std::thread::{self, JoinHandle};
use tauri::{plugin::PluginApi, AppHandle, Runtime};

use crate::error::Error;
use crate::models::*;
use crate::paths::validate_path;

type SharedWavWriter = Arc<Mutex<Option<WavWriter<BufWriter<File>>>>>;

// Commands for the recording thread
enum RecorderCommand {
    Start(RecordingConfig, mpsc::Sender<Result<(), Error>>),
    Stop(mpsc::Sender<Result<RecordingResult, Error>>),
    Pause(mpsc::Sender<Result<(), Error>>),
    Resume(mpsc::Sender<Result<(), Error>>),
    Shutdown,
}

// Shared state that's Send + Sync
struct SharedState {
    is_recording: AtomicBool,
    is_paused: AtomicBool,
    duration_ms: AtomicU64,
    output_path: Mutex<Option<String>>,
    sample_rate: AtomicU64,
    channels: AtomicU64,
    generation: AtomicU64,
}

impl Default for SharedState {
    fn default() -> Self {
        Self {
            is_recording: AtomicBool::new(false),
            is_paused: AtomicBool::new(false),
            duration_ms: AtomicU64::new(0),
            output_path: Mutex::new(None),
            sample_rate: AtomicU64::new(44100),
            channels: AtomicU64::new(1),
            generation: AtomicU64::new(0),
        }
    }
}

pub fn init<R: Runtime, C: DeserializeOwned>(
    _app: &AppHandle<R>,
    _api: PluginApi<R, C>,
) -> crate::Result<AudioRecorder<R>> {
    let shared = Arc::new(SharedState::default());
    let (cmd_tx, cmd_rx) = mpsc::channel::<RecorderCommand>();

    // Spawn the recording thread
    let shared_clone = Arc::clone(&shared);
    let cmd_tx_clone = cmd_tx.clone();
    let handle = thread::spawn(move || {
        recording_thread(cmd_rx, shared_clone, cmd_tx_clone);
    });

    Ok(AudioRecorder {
        shared,
        cmd_tx,
        _thread_handle: Mutex::new(Some(handle)),
        _runtime: PhantomData,
    })
}

// Recording thread - owns all non-Send cpal types
fn recording_thread(
    rx: mpsc::Receiver<RecorderCommand>,
    shared: Arc<SharedState>,
    cmd_tx: mpsc::Sender<RecorderCommand>,
) {
    let mut current_stream: Option<cpal::Stream> = None;
    let mut current_writer: Option<SharedWavWriter> = None;
    let mut write_flag: Option<Arc<AtomicBool>> = None;

    loop {
        match rx.recv() {
            Ok(RecorderCommand::Start(config, reply)) => {
                let result = start_recording_internal(&config, &shared, &cmd_tx);
                match result {
                    Ok((stream, writer, flag)) => {
                        current_stream = Some(stream);
                        current_writer = Some(writer);
                        write_flag = Some(flag);
                        let _ = reply.send(Ok(()));
                    }
                    Err(e) => {
                        let _ = reply.send(Err(e));
                    }
                }
            }
            Ok(RecorderCommand::Stop(reply)) => {
                let result =
                    stop_recording_internal(&shared, &mut current_stream, &mut current_writer);
                write_flag = None;
                let _ = reply.send(result);
            }
            Ok(RecorderCommand::Pause(reply)) => {
                if let Some(ref flag) = write_flag {
                    flag.store(false, Ordering::SeqCst);
                    shared.is_paused.store(true, Ordering::SeqCst);
                    let _ = reply.send(Ok(()));
                } else {
                    let _ = reply.send(Err(Error::NotRecording));
                }
            }
            Ok(RecorderCommand::Resume(reply)) => {
                if let Some(ref flag) = write_flag {
                    flag.store(true, Ordering::SeqCst);
                    shared.is_paused.store(false, Ordering::SeqCst);
                    let _ = reply.send(Ok(()));
                } else {
                    let _ = reply.send(Err(Error::NotRecording));
                }
            }
            Ok(RecorderCommand::Shutdown) | Err(_) => {
                // Clean up and exit (pause first — see stop_recording_internal)
                if let Some(stream) = current_stream.take() {
                    let _ = stream.pause();
                    drop(stream);
                }
                if let Some(writer) = current_writer.take() {
                    if let Ok(mut guard) = writer.lock() {
                        if let Some(w) = guard.take() {
                            let _ = w.finalize();
                        }
                    }
                }
                break;
            }
        }
    }
}

/// Resolve the input device for a recording: the device whose name matches
/// `device_id` (as returned by `get_devices`), or the system default when no
/// id is given. A requested device that is no longer present (e.g. unplugged
/// since the last device scan) falls back to the default so the recording
/// still succeeds — unless a `channel_id` was also requested and the
/// fallback device can't honor it, in which case the recording still fails
/// with `InvalidChannel` rather than silently capturing the wrong physical
/// channel on a device the caller didn't ask for.
fn find_input_device(host: &cpal::Host, device_id: Option<&str>) -> Result<cpal::Device, Error> {
    if let Some(id) = device_id {
        let mut devices = host
            .input_devices()
            .map_err(|e| Error::Recording(format!("Failed to enumerate devices: {}", e)))?;
        if let Some(device) = devices.find(|d| d.name().map(|n| n == id).unwrap_or(false)) {
            log::info!("Using requested input device: {}", id);
            return Ok(device);
        }
        log::warn!(
            "Requested input device '{}' not found, falling back to default",
            id
        );
    }
    host.default_input_device().ok_or(Error::DeviceNotFound)
}

/// Number of input channels a device can deliver: the widest configuration it
/// supports, since that is the stream a single channel has to be picked out of.
fn device_channel_count(device: &cpal::Device) -> u16 {
    let from_supported = device
        .supported_input_configs()
        .ok()
        .and_then(|configs| configs.map(|cfg| cfg.channels()).filter(|&c| c > 0).max());

    match from_supported {
        Some(channels) => channels,
        None => match device.default_input_config() {
            Ok(cfg) if cfg.channels() > 0 => cfg.channels(),
            _ => 1,
        },
    }
}

/// Resolve the channel index for a recording: the channel whose `id` (as
/// returned by `get_channels`) was requested, or `None` to record every
/// channel. Unlike a missing device, a channel the device does not have is an
/// error — silently recording a different channel would be indistinguishable
/// from success.
fn resolve_channel(channel_id: Option<&str>, available: u16) -> Result<Option<u16>, Error> {
    let Some(id) = channel_id else {
        return Ok(None);
    };

    let index: u16 = id
        .parse()
        .map_err(|_| Error::InvalidChannel(format!("'{}' is not a channel id", id)))?;

    if index >= available {
        return Err(Error::InvalidChannel(format!(
            "'{}' — device has {} channel(s)",
            id, available
        )));
    }

    log::info!("Using requested input channel: {}", index);
    Ok(Some(index))
}

/// Samples that belong in the WAV file: the whole interleaved buffer, or only
/// the selected channel's sample from each frame.
fn wav_samples<T>(data: &[T], channel: Option<u16>, channels: u16) -> impl Iterator<Item = &T> {
    let (offset, step) = match channel {
        Some(index) => (index as usize, channels as usize),
        None => (0, 1),
    };
    data.iter().skip(offset).step_by(step)
}

/// Pick the best supported (sample_rate, channels) for a requested quality,
/// from each config's (sr_min, sr_max, channels).
fn negotiate_config(
    configs: impl Iterator<Item = (u32, u32, u16)>,
    target_sample_rate: u32,
    target_channels: u16,
    channel_selected: bool,
) -> Option<(u32, u16)> {
    let mut sample_rate_match = None;
    for (sr_min, sr_max, ch) in configs {
        let sr_in_range = target_sample_rate >= sr_min && target_sample_rate <= sr_max;
        if sr_in_range && ch == target_channels {
            return Some((target_sample_rate, target_channels));
        }
        if !channel_selected && sr_in_range && sample_rate_match.is_none() {
            sample_rate_match = Some((target_sample_rate, ch));
        }
    }
    sample_rate_match
}

/// Resolve the WAV output path for a recording. Empty, or a relative path
/// (bare filename or subdirectory), resolves under the OS temp directory
/// rather than the process's unpredictable current directory; an absolute
/// path is used as-is. Rejects `..` path-traversal components.
fn resolve_wav_path(output_path: &str) -> Result<PathBuf, Error> {
    if output_path.is_empty() {
        let timestamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs();
        return Ok(std::env::temp_dir().join(format!("recording-{}.wav", timestamp)));
    }

    validate_path(output_path)?;

    let with_wav_extension = |name: &str| -> String {
        if name.to_lowercase().ends_with(".wav") {
            name.to_string()
        } else {
            format!("{}.wav", name)
        }
    };

    let path = PathBuf::from(output_path);
    if path.is_absolute() {
        Ok(PathBuf::from(with_wav_extension(output_path)))
    } else {
        Ok(std::env::temp_dir().join(with_wav_extension(output_path)))
    }
}

fn start_recording_internal(
    config: &RecordingConfig,
    shared: &Arc<SharedState>,
    cmd_tx: &mpsc::Sender<RecorderCommand>,
) -> Result<(cpal::Stream, SharedWavWriter, Arc<AtomicBool>), Error> {
    if shared.is_recording.load(Ordering::SeqCst) {
        return Err(Error::AlreadyRecording);
    }

    let host = cpal::default_host();
    let device = find_input_device(&host, config.device_id.as_deref())?;
    let available_channels = device_channel_count(&device);
    let selected_channel = resolve_channel(config.channel_id.as_deref(), available_channels)?;

    // Get the device's default/supported configuration
    let supported_config = device
        .default_input_config()
        .map_err(|e| Error::Recording(format!("Failed to get device config: {}", e)))?;

    // Try to use quality preset settings, fall back to device defaults
    let target_sample_rate = config.quality.sample_rate();
    let target_channels = match selected_channel {
        // A single channel can only be picked out of a stream that carries it,
        // so capture everything the device offers and drop the rest on write.
        Some(_) => available_channels,
        None => config.quality.channels(),
    };

    // Check if device supports the target configuration
    let supported_configs = device.supported_input_configs();
    let negotiated = supported_configs.ok().and_then(|configs| {
        negotiate_config(
            configs.map(|cfg| {
                (
                    cfg.min_sample_rate().0,
                    cfg.max_sample_rate().0,
                    cfg.channels(),
                )
            }),
            target_sample_rate,
            target_channels,
            selected_channel.is_some(),
        )
    });

    let (sample_rate, channels) = match negotiated {
        Some((sr, ch)) => {
            log::info!(
                "Using quality preset: {}Hz, {} channels (target was {}Hz, {} channels)",
                sr,
                ch,
                target_sample_rate,
                target_channels
            );
            (sr, ch)
        }
        None => {
            if let Some(index) = selected_channel {
                return Err(Error::InvalidChannel(format!(
                    "'{}' — no supported input configuration offers {} channel(s) at {}Hz",
                    index, target_channels, target_sample_rate
                )));
            }
            log::warn!(
                "Quality preset not supported ({}Hz, {} ch), using device defaults",
                target_sample_rate,
                target_channels
            );
            (
                supported_config.sample_rate().0,
                supported_config.channels(),
            )
        }
    };

    debug_assert!(selected_channel.map_or(true, |index| index < channels));

    // Only the selected channel is written, so the file becomes mono
    let output_channels = if selected_channel.is_some() {
        1
    } else {
        channels
    };

    log::info!(
        "Recording config: {}Hz, {} channels, format: {:?}",
        sample_rate,
        channels,
        supported_config.sample_format()
    );

    // Build stream config with our target settings
    let cpal_config = StreamConfig {
        channels,
        sample_rate: cpal::SampleRate(sample_rate),
        buffer_size: cpal::BufferSize::Default,
    };

    // Create output file path
    let path = resolve_wav_path(&config.output_path)?;
    let file_path = path.to_string_lossy().to_string();

    // Ensure parent directory exists
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }

    let spec = WavSpec {
        channels: output_channels,
        sample_rate,
        bits_per_sample: 16,
        sample_format: hound::SampleFormat::Int,
    };

    let writer = WavWriter::create(&path, spec)
        .map_err(|e| Error::Recording(format!("Failed to create WAV file: {}", e)))?;
    let writer = Arc::new(Mutex::new(Some(writer)));
    let writer_clone = Arc::clone(&writer);

    let write_enabled = Arc::new(AtomicBool::new(true));
    let write_enabled_clone = Arc::clone(&write_enabled);

    let shared_clone = Arc::clone(shared);
    let start_time = std::time::Instant::now();

    let err_fn = |err| log::error!("Audio stream error: {}", err);

    let stream = match supported_config.sample_format() {
        SampleFormat::F32 => device.build_input_stream(
            &cpal_config,
            move |data: &[f32], _: &cpal::InputCallbackInfo| {
                if !write_enabled_clone.load(Ordering::Relaxed) {
                    return;
                }
                if let Ok(mut guard) = writer_clone.lock() {
                    if let Some(ref mut w) = *guard {
                        for &sample in wav_samples(data, selected_channel, channels) {
                            let sample_i16 = (sample * 32767.0) as i16;
                            let _ = w.write_sample(sample_i16);
                        }
                    }
                }
                let elapsed = start_time.elapsed().as_millis() as u64;
                shared_clone.duration_ms.store(elapsed, Ordering::Relaxed);
            },
            err_fn,
            None,
        ),
        SampleFormat::I16 => {
            let write_enabled_clone = Arc::clone(&write_enabled);
            let writer_clone = Arc::clone(&writer);
            let shared_clone = Arc::clone(shared);
            device.build_input_stream(
                &cpal_config,
                move |data: &[i16], _: &cpal::InputCallbackInfo| {
                    if !write_enabled_clone.load(Ordering::Relaxed) {
                        return;
                    }
                    if let Ok(mut guard) = writer_clone.lock() {
                        if let Some(ref mut w) = *guard {
                            for &sample in wav_samples(data, selected_channel, channels) {
                                let _ = w.write_sample(sample);
                            }
                        }
                    }
                    let elapsed = start_time.elapsed().as_millis() as u64;
                    shared_clone.duration_ms.store(elapsed, Ordering::Relaxed);
                },
                err_fn,
                None,
            )
        }
        SampleFormat::U16 => {
            let write_enabled_clone = Arc::clone(&write_enabled);
            let writer_clone = Arc::clone(&writer);
            let shared_clone = Arc::clone(shared);
            device.build_input_stream(
                &cpal_config,
                move |data: &[u16], _: &cpal::InputCallbackInfo| {
                    if !write_enabled_clone.load(Ordering::Relaxed) {
                        return;
                    }
                    if let Ok(mut guard) = writer_clone.lock() {
                        if let Some(ref mut w) = *guard {
                            for &sample in wav_samples(data, selected_channel, channels) {
                                let sample_i16 = (sample as i32 - 32768) as i16;
                                let _ = w.write_sample(sample_i16);
                            }
                        }
                    }
                    let elapsed = start_time.elapsed().as_millis() as u64;
                    shared_clone.duration_ms.store(elapsed, Ordering::Relaxed);
                },
                err_fn,
                None,
            )
        }
        _ => return Err(Error::UnsupportedFormat),
    }
    .map_err(|e| Error::Recording(format!("Failed to build audio stream: {}", e)))?;

    stream
        .play()
        .map_err(|e| Error::Recording(format!("Failed to start audio stream: {}", e)))?;

    // Update shared state
    let generation = shared.generation.fetch_add(1, Ordering::SeqCst) + 1;
    shared.is_recording.store(true, Ordering::SeqCst);
    shared.is_paused.store(false, Ordering::SeqCst);
    shared.duration_ms.store(0, Ordering::SeqCst);
    shared
        .sample_rate
        .store(sample_rate as u64, Ordering::SeqCst);
    shared
        .channels
        .store(output_channels as u64, Ordering::SeqCst);
    *shared.output_path.lock().unwrap() = Some(file_path);

    log::info!(
        "Recording started: {}Hz, {} channels",
        sample_rate,
        output_channels
    );

    if config.max_duration > 0 {
        spawn_max_duration_watcher(
            shared,
            cmd_tx,
            generation,
            std::time::Duration::from_secs(config.max_duration as u64),
        );
    }

    Ok((stream, writer, write_enabled))
}

/// Auto-stops a recording after `duration`, unless it's already stopped or a
/// newer recording (different `generation`) has started by then.
fn spawn_max_duration_watcher(
    shared: &Arc<SharedState>,
    cmd_tx: &mpsc::Sender<RecorderCommand>,
    generation: u64,
    duration: std::time::Duration,
) {
    let shared = Arc::clone(shared);
    let cmd_tx = cmd_tx.clone();
    thread::spawn(move || {
        thread::sleep(duration);
        // ponytail: check-then-send narrows but doesn't fully close the race
        // against a manual stop+restart in the same instant this fires; add
        // a generation-checked stop command if that ever matters in practice.
        if shared.is_recording.load(Ordering::SeqCst)
            && shared.generation.load(Ordering::SeqCst) == generation
        {
            let (reply_tx, _reply_rx) = mpsc::channel();
            let _ = cmd_tx.send(RecorderCommand::Stop(reply_tx));
        }
    });
}

fn stop_recording_internal(
    shared: &Arc<SharedState>,
    stream: &mut Option<cpal::Stream>,
    writer: &mut Option<SharedWavWriter>,
) -> Result<RecordingResult, Error> {
    if !shared.is_recording.load(Ordering::SeqCst) {
        return Err(Error::NotRecording);
    }

    // Stop the stream. Dropping alone is not enough: on macOS (cpal 0.15)
    // the stream's device-disconnect listener holds a clone of the stream,
    // so this drop never destroys the AudioUnit and the input device stays
    // open — the OS mic-in-use indicator (and e.g. the record LED on USB
    // dictation microphones) stays on for the app's lifetime.
    if let Some(s) = stream.take() {
        if let Err(e) = s.pause() {
            log::warn!("Failed to stop audio stream: {}", e);
        }
        drop(s);
    }

    // Finalize the WAV file.
    let finalize_result =
        writer
            .take()
            .and_then(|w| w.lock().ok()?.take())
            .map_or(Ok(()), |wav_writer| {
                wav_writer
                    .finalize()
                    .map_err(|e| Error::Recording(format!("Failed to finalize WAV: {}", e)))
            });

    let duration_ms = shared.duration_ms.load(Ordering::SeqCst);
    let sample_rate = shared.sample_rate.load(Ordering::SeqCst) as u32;
    let channels = shared.channels.load(Ordering::SeqCst) as u16;
    let output_path = shared
        .output_path
        .lock()
        .unwrap()
        .take()
        .unwrap_or_default();

    // Get file size
    let file_size = std::fs::metadata(&output_path)
        .map(|m| m.len())
        .unwrap_or(0);

    // Reset state
    shared.is_recording.store(false, Ordering::SeqCst);
    shared.is_paused.store(false, Ordering::SeqCst);
    shared.duration_ms.store(0, Ordering::SeqCst);

    finalize_result?;

    log::info!("Recording stopped: {} ({}ms)", output_path, duration_ms);

    Ok(RecordingResult {
        file_path: output_path,
        duration_ms,
        file_size,
        sample_rate,
        channels,
    })
}

/// Access to the audio-recorder APIs.
pub struct AudioRecorder<R: Runtime> {
    shared: Arc<SharedState>,
    cmd_tx: mpsc::Sender<RecorderCommand>,
    _thread_handle: Mutex<Option<JoinHandle<()>>>,
    _runtime: PhantomData<fn() -> R>,
}

impl<R: Runtime> AudioRecorder<R> {
    /// Start recording audio
    pub fn start_recording(&self, config: RecordingConfig) -> crate::Result<()> {
        let (reply_tx, reply_rx) = mpsc::channel();
        self.cmd_tx
            .send(RecorderCommand::Start(config, reply_tx))
            .map_err(|_| Error::Recording("Recording thread not available".to_string()))?;

        reply_rx
            .recv()
            .map_err(|_| Error::Recording("No response from recording thread".to_string()))?
    }

    /// Stop recording and return the result
    pub fn stop_recording(&self) -> crate::Result<RecordingResult> {
        let (reply_tx, reply_rx) = mpsc::channel();
        self.cmd_tx
            .send(RecorderCommand::Stop(reply_tx))
            .map_err(|_| Error::Recording("Recording thread not available".to_string()))?;

        reply_rx
            .recv()
            .map_err(|_| Error::Recording("No response from recording thread".to_string()))?
    }

    /// Pause recording
    pub fn pause_recording(&self) -> crate::Result<()> {
        let (reply_tx, reply_rx) = mpsc::channel();
        self.cmd_tx
            .send(RecorderCommand::Pause(reply_tx))
            .map_err(|_| Error::Recording("Recording thread not available".to_string()))?;

        reply_rx
            .recv()
            .map_err(|_| Error::Recording("No response from recording thread".to_string()))?
    }

    /// Resume recording
    pub fn resume_recording(&self) -> crate::Result<()> {
        let (reply_tx, reply_rx) = mpsc::channel();
        self.cmd_tx
            .send(RecorderCommand::Resume(reply_tx))
            .map_err(|_| Error::Recording("Recording thread not available".to_string()))?;

        reply_rx
            .recv()
            .map_err(|_| Error::Recording("No response from recording thread".to_string()))?
    }

    /// Get current recording status
    pub fn get_status(&self) -> crate::Result<RecordingStatus> {
        let is_recording = self.shared.is_recording.load(Ordering::SeqCst);
        let is_paused = self.shared.is_paused.load(Ordering::SeqCst);
        let duration_ms = self.shared.duration_ms.load(Ordering::SeqCst);
        let output_path = self.shared.output_path.lock().unwrap().clone();

        let state = if !is_recording {
            RecordingState::Idle
        } else if is_paused {
            RecordingState::Paused
        } else {
            RecordingState::Recording
        };

        Ok(RecordingStatus {
            state,
            duration_ms,
            output_path,
        })
    }

    /// List available audio input devices
    pub fn get_devices(&self) -> crate::Result<AudioDevicesResponse> {
        let host = cpal::default_host();
        let default_device = host.default_input_device();
        let default_name = default_device.as_ref().and_then(|d| d.name().ok());

        let devices = host
            .input_devices()
            .map_err(|e| Error::Recording(format!("Failed to enumerate devices: {}", e)))?;

        let mut result = Vec::new();
        for device in devices {
            if let Ok(name) = device.name() {
                let is_default = default_name.as_ref().map(|n| n == &name).unwrap_or(false);
                result.push(AudioDevice {
                    id: name.clone(),
                    name,
                    is_default,
                });
            }
        }

        Ok(AudioDevicesResponse { devices: result })
    }

    /// List the input channels of an audio device
    pub fn get_channels(&self, device_id: Option<String>) -> crate::Result<AudioChannelsResponse> {
        let host = cpal::default_host();
        let device = find_input_device(&host, device_id.as_deref())?;

        let channels = (0..device_channel_count(&device))
            .map(|index| AudioChannel {
                id: index.to_string(),
                name: format!("Channel {}", index + 1),
                is_default: index == 0,
            })
            .collect();

        Ok(AudioChannelsResponse { channels })
    }

    /// Check microphone permission (always granted on desktop)
    pub fn check_permission(&self) -> crate::Result<PermissionStatus> {
        // On desktop, microphone access is typically granted at the OS level
        Ok(PermissionStatus {
            granted: true,
            can_request: false,
        })
    }

    /// Request microphone permission (no-op on desktop)
    pub fn request_permission(&self) -> crate::Result<PermissionStatus> {
        // On desktop, this is handled by the OS
        Ok(PermissionStatus {
            granted: true,
            can_request: false,
        })
    }
}

impl<R: Runtime> Drop for AudioRecorder<R> {
    fn drop(&mut self) {
        // Send shutdown command
        let _ = self.cmd_tx.send(RecorderCommand::Shutdown);
        // Wait for thread to finish
        if let Ok(mut guard) = self._thread_handle.lock() {
            if let Some(handle) = guard.take() {
                let _ = handle.join();
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_resolve_channel_none() {
        assert_eq!(resolve_channel(None, 2).unwrap(), None);
    }

    #[test]
    fn test_resolve_channel_selected() {
        assert_eq!(resolve_channel(Some("0"), 2).unwrap(), Some(0));
        assert_eq!(resolve_channel(Some("1"), 2).unwrap(), Some(1));
    }

    #[test]
    fn test_resolve_channel_invalid() {
        assert!(matches!(
            resolve_channel(Some("2"), 2),
            Err(Error::InvalidChannel(_))
        ));
        assert!(matches!(
            resolve_channel(Some("abc"), 2),
            Err(Error::InvalidChannel(_))
        ));
    }

    #[test]
    fn test_wav_samples_all_channels() {
        let data = [1, 2, 3, 4];
        let samples: Vec<_> = wav_samples(&data, None, 2).copied().collect();
        assert_eq!(samples, vec![1, 2, 3, 4]);
    }

    #[test]
    fn test_negotiate_config_exact_match_preferred() {
        let configs = [(44100, 48000, 2), (16000, 48000, 8)];
        // Both configs cover 44100Hz; the one with the target channel count wins.
        assert_eq!(
            negotiate_config(configs.into_iter(), 44100, 8, true),
            Some((44100, 8))
        );
    }

    #[test]
    fn test_negotiate_config_no_channel_selected_falls_back_to_sample_rate() {
        // No config offers 8 channels at all; without a channel selected,
        // matching just the sample rate is an acceptable fallback.
        let configs = [(44100, 48000, 2)];
        assert_eq!(
            negotiate_config(configs.into_iter(), 44100, 8, false),
            Some((44100, 2))
        );
    }

    #[test]
    fn test_negotiate_config_channel_selected_rejects_narrower_fallback() {
        // Same input as above, but with a channel selected: falling back to
        // the 2-channel config would silently point the selected index at a
        // different physical channel, so this must return None instead.
        let configs = [(44100, 48000, 2)];
        assert_eq!(negotiate_config(configs.into_iter(), 44100, 8, true), None);
    }

    #[test]
    fn test_negotiate_config_no_sample_rate_match() {
        let configs = [(8000, 16000, 2)];
        assert_eq!(negotiate_config(configs.into_iter(), 44100, 2, false), None);
    }

    #[test]
    fn test_wav_samples_selected_channel() {
        // Two interleaved frames: [left, right, left, right]
        let data = [1, 2, 3, 4];
        let left: Vec<_> = wav_samples(&data, Some(0), 2).copied().collect();
        let right: Vec<_> = wav_samples(&data, Some(1), 2).copied().collect();
        assert_eq!(left, vec![1, 3]);
        assert_eq!(right, vec![2, 4]);
    }

    #[test]
    fn test_resolve_wav_path_empty_goes_to_temp_dir() {
        let path = resolve_wav_path("").unwrap();
        assert_eq!(path.parent().unwrap(), std::env::temp_dir());
        assert_eq!(path.extension().unwrap(), "wav");
    }

    #[test]
    fn test_resolve_wav_path_bare_filename_goes_to_temp_dir() {
        let path = resolve_wav_path("my-recording").unwrap();
        assert_eq!(path, std::env::temp_dir().join("my-recording.wav"));
    }

    #[test]
    fn test_resolve_wav_path_relative_subdir_goes_to_temp_dir_not_cwd() {
        // A relative path with a subdirectory used to be treated as "the
        // full path, use as-is" and resolved against the process's CWD.
        let path = resolve_wav_path("myfolder/myrecording").unwrap();
        assert_eq!(path, std::env::temp_dir().join("myfolder/myrecording.wav"));
    }

    #[test]
    fn test_resolve_wav_path_absolute_used_as_is() {
        let path = resolve_wav_path("/custom/path/recording").unwrap();
        assert_eq!(path, PathBuf::from("/custom/path/recording.wav"));
    }

    #[test]
    fn test_resolve_wav_path_does_not_double_extension() {
        let path = resolve_wav_path("/custom/path/recording.wav").unwrap();
        assert_eq!(path, PathBuf::from("/custom/path/recording.wav"));
    }

    #[test]
    fn test_resolve_wav_path_rejects_traversal() {
        assert!(matches!(
            resolve_wav_path("../escape"),
            Err(Error::InvalidPath(_))
        ));
        assert!(matches!(
            resolve_wav_path("/absolute/../../escape"),
            Err(Error::InvalidPath(_))
        ));
    }

    #[test]
    fn test_max_duration_watcher_stops_the_still_active_recording() {
        let shared = Arc::new(SharedState::default());
        shared.is_recording.store(true, Ordering::SeqCst);
        let generation = shared.generation.fetch_add(1, Ordering::SeqCst) + 1;
        let (tx, rx) = mpsc::channel();

        spawn_max_duration_watcher(
            &shared,
            &tx,
            generation,
            std::time::Duration::from_millis(5),
        );

        assert!(matches!(
            rx.recv_timeout(std::time::Duration::from_secs(1)),
            Ok(RecorderCommand::Stop(_))
        ));
    }

    #[test]
    fn test_max_duration_watcher_is_a_noop_after_manual_stop() {
        let shared = Arc::new(SharedState::default());
        shared.is_recording.store(true, Ordering::SeqCst);
        let generation = shared.generation.fetch_add(1, Ordering::SeqCst) + 1;
        let (tx, rx) = mpsc::channel();

        spawn_max_duration_watcher(
            &shared,
            &tx,
            generation,
            std::time::Duration::from_millis(5),
        );
        shared.is_recording.store(false, Ordering::SeqCst);

        assert!(rx
            .recv_timeout(std::time::Duration::from_millis(200))
            .is_err());
    }

    #[test]
    fn test_max_duration_watcher_is_a_noop_after_a_newer_recording_starts() {
        let shared = Arc::new(SharedState::default());
        shared.is_recording.store(true, Ordering::SeqCst);
        let generation = shared.generation.fetch_add(1, Ordering::SeqCst) + 1;
        let (tx, rx) = mpsc::channel();

        spawn_max_duration_watcher(
            &shared,
            &tx,
            generation,
            std::time::Duration::from_millis(5),
        );
        // A new recording started (and is still active) before the watcher fired.
        shared.generation.fetch_add(1, Ordering::SeqCst);

        assert!(rx
            .recv_timeout(std::time::Duration::from_millis(200))
            .is_err());
    }
}
