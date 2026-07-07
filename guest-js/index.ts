import { invoke } from "@tauri-apps/api/core";

// Note: input `format` is currently limited to "wav".
// On mobile, the native recorder still outputs M4A/AAC due to platform APIs.
export type AudioFormat = "wav";
export type AudioQuality = "low" | "medium" | "high";

export interface RecordingConfig {
  outputPath: string;
  format?: AudioFormat;
  quality?: AudioQuality;
  /** Maximum recording duration in seconds. 0 means no limit. */
  maxDuration?: number;
  /**
   * Input device to record from, identified by the `id` returned from
   * `getDevices()`. Defaults to the system default input device.
   * Desktop only; ignored on mobile. Falls back to the default device
   * if the requested device is no longer available.
   */
  deviceId?: string;
}

export type RecordingState = "idle" | "recording" | "paused";

export interface RecordingStatus {
  state: RecordingState;
  durationMs: number;
  outputPath: string | null;
}

export interface RecordingResult {
  filePath: string;
  durationMs: number;
  sampleRate: number;
  channels: number;
  /** File size in bytes */
  fileSize: number;
}

export interface PermissionResponse {
  /** Whether the permission is currently granted */
  granted: boolean;
  /** Whether the permission can be requested (not permanently denied) */
  canRequest: boolean;
}

export interface AudioDevice {
  id: string;
  name: string;
  isDefault: boolean;
}

export interface DevicesResponse {
  devices: AudioDevice[];
}

export async function startRecording(config: RecordingConfig): Promise<void> {
  await invoke("plugin:audio-recorder|start_recording", { config });
}

export async function stopRecording(): Promise<RecordingResult> {
  return await invoke("plugin:audio-recorder|stop_recording");
}

export async function pauseRecording(): Promise<void> {
  await invoke("plugin:audio-recorder|pause_recording");
}

export async function resumeRecording(): Promise<void> {
  await invoke("plugin:audio-recorder|resume_recording");
}

export async function getStatus(): Promise<RecordingStatus> {
  return await invoke("plugin:audio-recorder|get_status");
}

/**
 * Check if microphone permission is granted.
 * @returns Permission status with granted state and whether it can be requested
 */
export async function checkPermission(): Promise<PermissionResponse> {
  return await invoke("plugin:audio-recorder|check_permission");
}

/**
 * Request microphone permission from the user.
 * On Android, this will show a permission dialog and wait for the user's response.
 * @returns Permission status after the user responds
 */
export async function requestPermission(): Promise<PermissionResponse> {
  return await invoke("plugin:audio-recorder|request_permission");
}

/**
 * Get available audio input devices.
 * @returns List of available audio input devices
 */
export async function getDevices(): Promise<DevicesResponse> {
  return await invoke("plugin:audio-recorder|get_devices");
}
