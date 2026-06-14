import { useState, useEffect, useRef } from "react";
import { convertFileSrc } from "@tauri-apps/api/core";
import {
  startRecording,
  stopRecording,
  pauseRecording,
  resumeRecording,
  getStatus,
  getDevices,
  checkPermission,
  requestPermission,
  type RecordingStatus,
  type RecordingResult,
  type AudioDevice,
  type AudioQuality,
  type PermissionResponse,
} from "tauri-plugin-audio-recorder-api";
import "./App.css";

function App() {
  const [quality, setQuality] = useState<AudioQuality>("medium");
  const [status, setStatus] = useState<RecordingStatus | null>(null);
  const [result, setResult] = useState<RecordingResult | null>(null);
  const [devices, setDevices] = useState<AudioDevice[]>([]);
  const [permission, setPermission] = useState<PermissionResponse | null>(null);
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [success, setSuccess] = useState<string | null>(null);
  const pollingRef = useRef<ReturnType<typeof setInterval> | null>(null);

  const audioRef = useRef<HTMLAudioElement | null>(null);
  const [isPlaying, setIsPlaying] = useState(false);
  const [playbackProgress, setPlaybackProgress] = useState(0);
  const [playbackTime, setPlaybackTime] = useState(0);
  const [volume, setVolume] = useState(1);

  useEffect(() => {
    loadDevices();
    checkPerm();
    return () => {
      if (pollingRef.current) clearInterval(pollingRef.current);
    };
  }, []);

  const startPolling = () => {
    if (pollingRef.current) return;
    pollingRef.current = setInterval(async () => {
      try {
        const s = await getStatus();
        setStatus(s);
        if (s.state === "idle" && pollingRef.current) {
          clearInterval(pollingRef.current);
          pollingRef.current = null;
        }
      } catch {
        // ignore polling errors
      }
    }, 100);
  };

  const stopPolling = () => {
    if (pollingRef.current) {
      clearInterval(pollingRef.current);
      pollingRef.current = null;
    }
  };

  const checkPerm = async () => {
    try {
      setPermission(await checkPermission());
    } catch (err) {
      setError(`Failed to check permission: ${err}`);
    }
  };

  const handleRequestPermission = async () => {
    try {
      const perm = await requestPermission();
      setPermission(perm);
      if (perm.granted) {
        setSuccess("Permission granted!");
        setTimeout(() => setSuccess(null), 3000);
      }
    } catch (err) {
      setError(`Failed to request permission: ${err}`);
    }
  };

  const loadDevices = async () => {
    setLoading(true);
    setError(null);
    try {
      const res = await getDevices();
      setDevices(res.devices);
    } catch (err) {
      setError(`Failed to load devices: ${err}`);
    } finally {
      setLoading(false);
    }
  };

  const handleStartRecording = async () => {
    setError(null);
    setResult(null);
    try {
      const timestamp = new Date().toISOString().replace(/[:.]/g, "-");
      await startRecording({
        outputPath: `recording-${timestamp}`,
        quality,
        format: "wav",
        maxDuration: 0,
      });
      startPolling();
      setSuccess("Recording started!");
      setTimeout(() => setSuccess(null), 2000);
    } catch (err) {
      const msg = String(err);
      if (msg.includes("permission")) {
        setError(`Permission required: ${err}`);
      } else {
        setError(`Failed to start recording: ${err}`);
      }
    }
  };

  const handleStopRecording = async () => {
    setError(null);
    try {
      stopPolling();
      const res = await stopRecording();
      setResult(res);
      setStatus(null);
      setSuccess("Recording saved!");
      setTimeout(() => setSuccess(null), 2000);
    } catch (err) {
      setError(`Failed to stop recording: ${err}`);
    }
  };

  const handlePauseRecording = async () => {
    setError(null);
    try {
      await pauseRecording();
      setStatus(await getStatus());
    } catch (err) {
      setError(`Failed to pause: ${err}`);
    }
  };

  const handleResumeRecording = async () => {
    setError(null);
    try {
      await resumeRecording();
      setStatus(await getStatus());
    } catch (err) {
      setError(`Failed to resume: ${err}`);
    }
  };

  const formatTime = (ms: number) => {
    const totalSeconds = Math.floor(ms / 1000);
    const minutes = Math.floor(totalSeconds / 60);
    const seconds = totalSeconds % 60;
    const tenths = Math.floor((ms % 1000) / 100);
    return `${String(minutes).padStart(2, "0")}:${String(seconds).padStart(2, "0")}.${tenths}`;
  };

  const formatSize = (bytes: number) => {
    if (bytes < 1024) return `${bytes} B`;
    if (bytes < 1024 * 1024) return `${(bytes / 1024).toFixed(1)} KB`;
    return `${(bytes / (1024 * 1024)).toFixed(2)} MB`;
  };

  const handlePlayRecording = async () => {
    if (!result) return;
    if (audioRef.current) {
      if (isPlaying) {
        audioRef.current.pause();
        setIsPlaying(false);
      } else {
        audioRef.current.play();
        setIsPlaying(true);
      }
      return;
    }
    try {
      const audioUrl = convertFileSrc(result.filePath);
      const audio = new Audio(audioUrl);
      audioRef.current = audio;
      audio.volume = volume;
      audio.addEventListener("timeupdate", () => {
        if (audio.duration > 0) {
          setPlaybackProgress((audio.currentTime / audio.duration) * 100);
          setPlaybackTime(audio.currentTime * 1000);
        }
      });
      audio.addEventListener("ended", () => {
        setIsPlaying(false);
        setPlaybackProgress(0);
        setPlaybackTime(0);
      });
      audio.addEventListener("error", () => {
        const codes: Record<number, string> = {
          1: "Playback aborted",
          2: "Network error",
          3: "Decode error — file may be corrupted",
          4: "Format not supported",
        };
        setError(`Playback error: ${codes[audio.error?.code ?? 0] ?? "Unknown"}`);
        setIsPlaying(false);
      });
      await audio.play();
      setIsPlaying(true);
    } catch (err) {
      setError(`Failed to start playback: ${err}`);
      setIsPlaying(false);
    }
  };

  const handleStopPlayback = () => {
    if (audioRef.current) {
      audioRef.current.pause();
      audioRef.current.currentTime = 0;
      setIsPlaying(false);
      setPlaybackProgress(0);
      setPlaybackTime(0);
    }
  };

  const handleReplayRecording = () => {
    if (audioRef.current) {
      audioRef.current.currentTime = 0;
      audioRef.current.play();
      setIsPlaying(true);
    }
  };

  const handleSeek = (value: number) => {
    if (audioRef.current && audioRef.current.duration) {
      audioRef.current.currentTime = (value / 100) * audioRef.current.duration;
      setPlaybackProgress(value);
    }
  };

  const handleVolume = (value: number) => {
    setVolume(value);
    if (audioRef.current) audioRef.current.volume = value;
  };

  useEffect(() => {
    return () => {
      if (audioRef.current) {
        audioRef.current.pause();
        audioRef.current = null;
      }
    };
  }, [result]);

  const isRecording = status?.state === "recording";
  const isPaused = status?.state === "paused";
  const isIdle = !status || status.state === "idle";

  return (
    <div className="page">
      <header className="header">
        <div className="header-inner">
          <h1 className="header-title">Audio Recorder</h1>
          <p className="header-subtitle">Tauri Plugin — native recording on all platforms</p>
        </div>
      </header>

      <main className="main">
        {error && (
          <div className="alert alert-error">
            <span>{error}</span>
            <button className="alert-close" onClick={() => setError(null)}>×</button>
          </div>
        )}
        {success && (
          <div className="alert alert-success">
            <span>{success}</span>
            <button className="alert-close" onClick={() => setSuccess(null)}>×</button>
          </div>
        )}

        {/* Permission */}
        <div className="card">
          <div className="card-row">
            <h2 className="card-title">Permission</h2>
          </div>
          {permission === null ? (
            <span className="spinner" />
          ) : (
            <div className="perm-row">
              <span className={`badge ${permission.granted ? "badge--green" : "badge--orange"}`}>
                {permission.granted ? "Granted" : "Not Granted"}
              </span>
              {!permission.granted && (
                <button className="btn btn-primary btn-sm" onClick={handleRequestPermission}>
                  Request Permission
                </button>
              )}
            </div>
          )}
        </div>

        {/* Quality */}
        <div className="card">
          <h2 className="card-title">Recording Quality</h2>
          <div className="quality-group">
            {(["low", "medium", "high"] as AudioQuality[]).map((q) => (
              <button
                key={q}
                className={`quality-btn${quality === q ? " quality-btn--active" : ""}`}
                onClick={() => setQuality(q)}
                disabled={!isIdle}
              >
                {q === "low" ? "Low · 16 kHz" : q === "medium" ? "Medium · 44 kHz" : "High · 48 kHz"}
              </button>
            ))}
          </div>
        </div>

        {/* Recording */}
        <div className="card">
          <div className="rec-panel">
            <div
              className={`rec-circle ${
                isRecording
                  ? "rec-circle--recording"
                  : isPaused
                    ? "rec-circle--paused"
                    : "rec-circle--idle"
              }`}
            >
              🎙
            </div>

            <span className="rec-time">{formatTime(status?.durationMs ?? 0)}</span>

            <span
              className={`badge ${
                isRecording ? "badge--red" : isPaused ? "badge--orange" : "badge--gray"
              }`}
            >
              {isRecording ? "Recording" : isPaused ? "Paused" : "Idle"}
            </span>

            {(isRecording || isPaused) && (
              <div className="rec-progress">
                <div
                  className={`rec-progress-fill ${
                    isPaused ? "rec-progress-fill--paused" : "rec-progress-fill--recording"
                  }`}
                />
              </div>
            )}

            <div className="rec-actions">
              {isIdle ? (
                <button
                  className="btn btn-record"
                  onClick={handleStartRecording}
                  disabled={!permission?.granted}
                >
                  ● Start Recording
                </button>
              ) : (
                <>
                  <button
                    className="btn btn-warn"
                    onClick={isPaused ? handleResumeRecording : handlePauseRecording}
                  >
                    {isPaused ? "▶ Resume" : "⏸ Pause"}
                  </button>
                  <button className="btn btn-primary" onClick={handleStopRecording}>
                    ■ Stop
                  </button>
                </>
              )}
            </div>
          </div>
        </div>

        {/* Result */}
        {result && (
          <div className="card">
            <div className="card-row">
              <h2 className="card-title">Recording Saved</h2>
              <span className="badge badge--green">✓ Done</span>
            </div>

            <p className="result-file">{result.filePath}</p>

            <div className="result-badges">
              <span className="badge badge--gray">{formatTime(result.durationMs)}</span>
              <span className="badge badge--gray">{formatSize(result.fileSize)}</span>
              <span className="badge badge--gray">{result.sampleRate} Hz</span>
              <span className="badge badge--gray">{result.channels}ch</span>
            </div>

            <div className="player">
              <span className="player-label">Playback</span>

              <div className="player-seek">
                <span className="player-time">{formatTime(playbackTime)}</span>
                <input
                  type="range"
                  min={0}
                  max={100}
                  step={0.1}
                  value={playbackProgress}
                  onChange={(e) => handleSeek(Number(e.target.value))}
                />
                <span className="player-time player-time--right">
                  {formatTime(result.durationMs)}
                </span>
              </div>

              <div className="player-controls">
                <button
                  className="btn btn-secondary btn-sm"
                  onClick={handleReplayRecording}
                  disabled={!audioRef.current}
                >
                  ↺ Replay
                </button>
                <button
                  className="btn btn-primary btn-sm"
                  onClick={handlePlayRecording}
                >
                  {isPlaying ? "⏸ Pause" : "▶ Play"}
                </button>
                <button
                  className="btn btn-secondary btn-sm"
                  onClick={handleStopPlayback}
                  disabled={!isPlaying && playbackProgress === 0}
                >
                  ■ Stop
                </button>
              </div>

              <div className="player-volume">
                <span className="player-label">Vol</span>
                <input
                  type="range"
                  min={0}
                  max={1}
                  step={0.05}
                  value={volume}
                  onChange={(e) => handleVolume(Number(e.target.value))}
                  style={{ maxWidth: 120 }}
                />
                <span className="player-volume-label">{Math.round(volume * 100)}%</span>
              </div>
            </div>
          </div>
        )}

        {/* Devices */}
        <div className="card">
          <div className="card-row">
            <h2 className="card-title">Audio Input Devices</h2>
            <button className="btn btn-ghost btn-sm" onClick={loadDevices} disabled={loading}>
              {loading ? <span className="spinner" /> : "↻ Refresh"}
            </button>
          </div>

          {devices.length === 0 && !loading ? (
            <p className="empty">No devices found. Click Refresh.</p>
          ) : (
            <div className="device-list">
              {devices.map((device) => (
                <div key={device.id} className="device-row">
                  <div className="device-info">
                    <span className="device-name">{device.name}</span>
                    {device.isDefault && <span className="device-sub">Default device</span>}
                  </div>
                  {device.isDefault && <span className="badge">Default</span>}
                </div>
              ))}
            </div>
          )}
        </div>
      </main>
    </div>
  );
}

export default App;
