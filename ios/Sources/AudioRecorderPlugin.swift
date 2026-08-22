import AVFoundation
import SwiftRs
import Tauri
import UIKit
import WebKit

class RecordingConfig: Decodable {
    let outputPath: String
    let format: String?
    let quality: String?
    let maxDuration: Int?
}

class AudioRecorderPlugin: Plugin {
    private var audioRecorder: AVAudioRecorder?
    private var recordingStartTime: Date?
    private var pausedDuration: TimeInterval = 0
    private var pauseStartTime: Date?
    private var isPaused: Bool = false
    private var isRecording: Bool = false
    private var isStopping: Bool = false  // Guard against concurrent stop calls
    private let stopLock = NSLock()
    private var currentFilePath: String?
    private var currentSampleRate: Int = 44100
    private var currentChannels: Int = 1
    private var maxDurationTimer: Timer?
    private var wasRecordingBeforeInterruption: Bool = false
    
    override init() {
        super.init()
        NSLog("[AudioRecorder] ============================================")
        NSLog("[AudioRecorder] PLUGIN INIT")
        NSLog("[AudioRecorder]   iOS Version: \(UIDevice.current.systemVersion)")
        NSLog("[AudioRecorder]   Device: \(UIDevice.current.model)")
        setupInterruptionHandling()
        NSLog("[AudioRecorder] ============================================")
    }
    
    /// Sets up observers for audio session interruptions (phone calls, Siri, etc.)
    private func setupInterruptionHandling() {
        NSLog("[AudioRecorder] Setting up interruption handling...")
        NotificationCenter.default.addObserver(
            self,
            selector: #selector(handleAudioSessionInterruption),
            name: AVAudioSession.interruptionNotification,
            object: AVAudioSession.sharedInstance()
        )
        
        NotificationCenter.default.addObserver(
            self,
            selector: #selector(handleAudioRouteChange),
            name: AVAudioSession.routeChangeNotification,
            object: AVAudioSession.sharedInstance()
        )
        NSLog("[AudioRecorder]   Observers registered")
    }
    
    /// Handles audio interruptions such as phone calls, Siri activation, etc.
    @objc private func handleAudioSessionInterruption(notification: Notification) {
        guard let userInfo = notification.userInfo,
              let typeValue = userInfo[AVAudioSessionInterruptionTypeKey] as? UInt,
              let type = AVAudioSession.InterruptionType(rawValue: typeValue) else {
            return
        }
        
        switch type {
        case .began:
            // Interruption began - pause recording if active
            if isRecording && !isPaused {
                wasRecordingBeforeInterruption = true
                audioRecorder?.pause()
                isPaused = true
                pauseStartTime = Date()
                NSLog("[AudioRecorder] Recording paused due to interruption")
            }
            
        case .ended:
            // Interruption ended - check if we should resume
            guard let optionsValue = userInfo[AVAudioSessionInterruptionOptionKey] as? UInt else { return }
            let options = AVAudioSession.InterruptionOptions(rawValue: optionsValue)
            
            if options.contains(.shouldResume) && wasRecordingBeforeInterruption {
                // Reactivate audio session and resume recording
                do {
                    try AVAudioSession.sharedInstance().setActive(true, options: .notifyOthersOnDeactivation)
                    audioRecorder?.record()
                    if let pauseStart = pauseStartTime {
                        pausedDuration += Date().timeIntervalSince(pauseStart)
                    }
                    isPaused = false
                    pauseStartTime = nil
                    NSLog("[AudioRecorder] Recording resumed after interruption")
                } catch {
                    NSLog("[AudioRecorder] Failed to resume after interruption: \(error.localizedDescription)")
                }
            }
            wasRecordingBeforeInterruption = false
            
        @unknown default:
            break
        }
    }
    
    /// Handles audio route changes (headphones plugged/unplugged, Bluetooth, etc.)
    @objc private func handleAudioRouteChange(notification: Notification) {
        guard let userInfo = notification.userInfo,
              let reasonValue = userInfo[AVAudioSessionRouteChangeReasonKey] as? UInt,
              let reason = AVAudioSession.RouteChangeReason(rawValue: reasonValue) else {
            return
        }
        
        switch reason {
        case .oldDeviceUnavailable:
            // Previous audio device (e.g., headphones) was disconnected
            NSLog("[AudioRecorder] Audio route changed: old device unavailable")
        case .newDeviceAvailable:
            NSLog("[AudioRecorder] Audio route changed: new device available")
        default:
            break
        }
    }
    
    deinit {
        NotificationCenter.default.removeObserver(self)
    }
    
    @objc public func startRecording(_ invoke: Invoke) throws {
        NSLog("[AudioRecorder] ============================================")
        NSLog("[AudioRecorder] startRecording() CALLED")
        
        let config = try invoke.parseArgs(RecordingConfig.self)
        NSLog("[AudioRecorder]   outputPath: \(config.outputPath)")
        NSLog("[AudioRecorder]   format: \(config.format ?? "default")")
        NSLog("[AudioRecorder]   quality: \(config.quality ?? "medium")")
        NSLog("[AudioRecorder]   maxDuration: \(config.maxDuration ?? 0)")
        
        if isRecording {
            NSLog("[AudioRecorder]   ERROR: Already recording")
            invoke.reject("Already recording")
            return
        }
        
        let permission = AVAudioSession.sharedInstance().recordPermission
        NSLog("[AudioRecorder]   Permission status: \(permission.rawValue)")
        
        switch permission {
        case .granted:
            NSLog("[AudioRecorder]   Permission granted, starting...")
            startRecordingWithConfig(config, invoke: invoke)
            
        case .denied:
            NSLog("[AudioRecorder]   ERROR: Permission denied")
            invoke.reject("Microphone permission denied. Please enable it in Settings.")
            
        case .undetermined:
            NSLog("[AudioRecorder]   Permission undetermined, requesting...")
            AVAudioSession.sharedInstance().requestRecordPermission { [weak self] granted in
                DispatchQueue.main.async {
                    NSLog("[AudioRecorder]   Permission request result: \(granted)")
                    if granted {
                        self?.startRecordingWithConfig(config, invoke: invoke)
                    } else {
                        invoke.reject("Microphone permission not granted")
                    }
                }
            }
            
        @unknown default:
            NSLog("[AudioRecorder]   ERROR: Unknown permission status")
            invoke.reject("Unknown permission status")
        }
    }
    
    private func startRecordingWithConfig(_ config: RecordingConfig, invoke: Invoke) {
        NSLog("[AudioRecorder] startRecordingWithConfig() CALLED")
        
        let quality = config.quality?.lowercased() ?? "medium"
        NSLog("[AudioRecorder]   Quality setting: \(quality)")
        
        switch quality {
        case "low":
            currentSampleRate = 16000
            currentChannels = 1
        case "high":
            currentSampleRate = 48000
            currentChannels = 2
        default: // medium
            currentSampleRate = 44100
            currentChannels = 1
        }
        NSLog("[AudioRecorder]   Sample rate: \(currentSampleRate), Channels: \(currentChannels)")
        
        // Always use cache directory to avoid file permission issues
        let cacheDir = FileManager.default.urls(for: .cachesDirectory, in: .userDomainMask).first!
        let filename: String
        if config.outputPath.isEmpty {
            let timestamp = Int(Date().timeIntervalSince1970 * 1000)
            filename = "recording_\(timestamp)"
        } else {
            // Extract only the filename, removing any path separators and extensions
            let baseName = (config.outputPath.components(separatedBy: "/").last ?? config.outputPath)
            filename = baseName
                .replacingOccurrences(of: ".m4a", with: "")
                .replacingOccurrences(of: ".wav", with: "")
                .replacingOccurrences(of: ".aac", with: "")
        }
        // Build full absolute path in cache directory
        let filePath = cacheDir.appendingPathComponent("\(filename).m4a").path
        currentFilePath = filePath
        let fileUrl = URL(fileURLWithPath: filePath)
        
        let directory = fileUrl.deletingLastPathComponent()
        try? FileManager.default.createDirectory(at: directory, withIntermediateDirectories: true)
        
        do {
            let session = AVAudioSession.sharedInstance()
            try session.setCategory(.playAndRecord, mode: .default, options: [.defaultToSpeaker, .allowBluetooth])
            try session.setActive(true, options: .notifyOthersOnDeactivation)
            
            let settings: [String: Any] = [
                AVFormatIDKey: Int(kAudioFormatMPEG4AAC),
                AVSampleRateKey: currentSampleRate,
                AVNumberOfChannelsKey: currentChannels,
                AVEncoderAudioQualityKey: AVAudioQuality.high.rawValue,
                AVEncoderBitRateKey: 128000
            ]
            
            audioRecorder = try AVAudioRecorder(url: fileUrl, settings: settings)
            audioRecorder?.isMeteringEnabled = true
            audioRecorder?.record()
            
            isRecording = true
            isPaused = false
            recordingStartTime = Date()
            pausedDuration = 0
            
            if let maxDuration = config.maxDuration, maxDuration > 0 {
                maxDurationTimer = Timer.scheduledTimer(withTimeInterval: TimeInterval(maxDuration), repeats: false) { [weak self] _ in
                    self?.stopRecordingInternal()
                }
            }
            
            invoke.resolve()
        } catch {
            cleanup()
            invoke.reject("Failed to start recording: \(error.localizedDescription)")
        }
    }
    
    @objc public func stopRecording(_ invoke: Invoke) throws {
        NSLog("[AudioRecorder] ============================================")
        NSLog("[AudioRecorder] stopRecording() CALLED")
        NSLog("[AudioRecorder]   isRecording: \(isRecording)")
        NSLog("[AudioRecorder]   isPaused: \(isPaused)")
        
        guard isRecording else {
            NSLog("[AudioRecorder]   ERROR: Not recording")
            invoke.reject("Not recording")
            return
        }
        
        guard let result = stopRecordingInternal() else {
            NSLog("[AudioRecorder]   ERROR: Failed to stop recording")
            invoke.reject("Failed to stop recording")
            return
        }
        
        NSLog("[AudioRecorder]   Recording stopped successfully")
        invoke.resolve(result)
    }
    
    private func stopRecordingInternal() -> [String: Any]? {
        NSLog("[AudioRecorder] stopRecordingInternal() CALLED")
        
        // Guard against concurrent stop calls (e.g., maxDuration timer + manual stop).
        // The timer fires on the main run loop while a manual stop comes through
        // Tauri's invoke handling, so the check-then-set needs a lock, not just a
        // plain Bool (which a plain `guard ... isStopping = true` isn't safe against).
        stopLock.lock()
        if isStopping {
            stopLock.unlock()
            NSLog("[AudioRecorder]   Stop already in progress, ignoring duplicate call")
            return nil
        }
        isStopping = true
        stopLock.unlock()

        defer {
            isStopping = false
        }
        
        maxDurationTimer?.invalidate()
        maxDurationTimer = nil
        NSLog("[AudioRecorder]   Max duration timer invalidated")
        
        guard let recorder = audioRecorder,
              let startTime = recordingStartTime,
              let filePath = currentFilePath else {
            NSLog("[AudioRecorder]   ERROR: Missing recorder, startTime, or filePath")
            return nil
        }
        
        recorder.stop()
        NSLog("[AudioRecorder]   Recorder stopped")
        
        let endTime = Date()
        let totalDuration: TimeInterval
        if isPaused, let pauseStart = pauseStartTime {
            totalDuration = pauseStart.timeIntervalSince(startTime) - pausedDuration
        } else {
            totalDuration = endTime.timeIntervalSince(startTime) - pausedDuration
        }
        
        let durationMs = Int(totalDuration * 1000)
        
        let fileSize: Int
        if let attrs = try? FileManager.default.attributesOfItem(atPath: filePath) {
            fileSize = (attrs[.size] as? Int) ?? 0
        } else {
            fileSize = 0
        }
        
        let result: [String: Any] = [
            "filePath": filePath,
            "durationMs": durationMs,
            "fileSize": fileSize,
            "sampleRate": currentSampleRate,
            "channels": currentChannels
        ]
        
        cleanup()
        return result
    }
    
    @objc public func pauseRecording(_ invoke: Invoke) throws {
        NSLog("[AudioRecorder] ============================================")
        NSLog("[AudioRecorder] pauseRecording() CALLED")
        NSLog("[AudioRecorder]   isRecording: \(isRecording), isPaused: \(isPaused)")
        
        guard isRecording else {
            NSLog("[AudioRecorder]   ERROR: Not recording")
            invoke.reject("Not recording")
            return
        }
        
        guard !isPaused else {
            NSLog("[AudioRecorder]   ERROR: Already paused")
            invoke.reject("Already paused")
            return
        }
        
        audioRecorder?.pause()
        isPaused = true
        pauseStartTime = Date()
        NSLog("[AudioRecorder]   Recording paused at \(pauseStartTime!)")
        
        invoke.resolve()
    }
    
    @objc public func resumeRecording(_ invoke: Invoke) throws {
        NSLog("[AudioRecorder] ============================================")
        NSLog("[AudioRecorder] resumeRecording() CALLED")
        NSLog("[AudioRecorder]   isRecording: \(isRecording), isPaused: \(isPaused)")
        
        guard isRecording else {
            NSLog("[AudioRecorder]   ERROR: Not recording")
            invoke.reject("Not recording")
            return
        }
        
        guard isPaused else {
            NSLog("[AudioRecorder]   ERROR: Not paused")
            invoke.reject("Not paused")
            return
        }
        
        audioRecorder?.record()
        
        if let pauseStart = pauseStartTime {
            let pauseDuration = Date().timeIntervalSince(pauseStart)
            pausedDuration += pauseDuration
            NSLog("[AudioRecorder]   Paused for \(pauseDuration)s, total paused: \(pausedDuration)s")
        }
        isPaused = false
        pauseStartTime = nil
        NSLog("[AudioRecorder]   Recording resumed")
        
        invoke.resolve()
    }
    
    @objc public func getStatus(_ invoke: Invoke) throws {
        NSLog("[AudioRecorder] getStatus() CALLED")
        
        let state: String
        if !isRecording {
            state = "idle"
        } else if isPaused {
            state = "paused"
        } else {
            state = "recording"
        }
        
        var durationMs: Int = 0
        if isRecording, let startTime = recordingStartTime {
            let currentDuration: TimeInterval
            if isPaused, let pauseStart = pauseStartTime {
                currentDuration = pauseStart.timeIntervalSince(startTime) - pausedDuration
            } else {
                currentDuration = Date().timeIntervalSince(startTime) - pausedDuration
            }
            durationMs = Int(currentDuration * 1000)
        }
        
        NSLog("[AudioRecorder]   State: \(state), Duration: \(durationMs)ms")
        NSLog("[AudioRecorder]   OutputPath: \(currentFilePath ?? "nil")")
        
        invoke.resolve([
            "state": state,
            "durationMs": durationMs,
            "outputPath": currentFilePath as Any
        ])
    }
    
    @objc public func getDevices(_ invoke: Invoke) throws {
        let session = AVAudioSession.sharedInstance()
        var devices: [[String: Any]] = []
        
        if let currentInput = session.currentRoute.inputs.first {
            devices.append([
                "id": currentInput.uid,
                "name": currentInput.portName,
                "isDefault": true
            ])
        } else {
            devices.append([
                "id": "default",
                "name": "Default Microphone",
                "isDefault": true
            ])
        }
        
        invoke.resolve(["devices": devices])
    }
    
    @objc public func checkPermission(_ invoke: Invoke) throws {
        NSLog("[AudioRecorder] checkPermission() CALLED")
        
        let permission = AVAudioSession.sharedInstance().recordPermission

        let granted = permission == .granted
        // iOS never lets you re-prompt after an explicit denial (no rationale API
        // like Android's), so "can request" is simply "not permanently denied".
        let canRequest = permission != .denied

        NSLog("[AudioRecorder]   Permission: \(permission.rawValue)")
        NSLog("[AudioRecorder]   Granted: \(granted), CanRequest: \(canRequest)")

        invoke.resolve([
            "granted": granted,
            "canRequest": canRequest
        ])
    }

    @objc public func requestPermission(_ invoke: Invoke) throws {
        NSLog("[AudioRecorder] requestPermission() CALLED")

        let session = AVAudioSession.sharedInstance()

        if session.recordPermission == .granted {
            NSLog("[AudioRecorder]   Already granted")
            invoke.resolve([
                "granted": true,
                "canRequest": false
            ])
            return
        }

        NSLog("[AudioRecorder]   Requesting permission...")
        session.requestRecordPermission { granted in
            // Apple doesn't guarantee this completion runs on the main thread,
            // and invoke.resolve() touches the WKWebView, so hop back like
            // startRecording()'s handling of the same API does.
            DispatchQueue.main.async {
                NSLog("[AudioRecorder]   Permission result: \(granted)")
                invoke.resolve([
                    "granted": granted,
                    "canRequest": granted
                ])
            }
        }
    }
    
    private func cleanup() {
        NSLog("[AudioRecorder] cleanup() CALLED")
        NSLog("[AudioRecorder]   Resetting state...")
        
        audioRecorder = nil
        isRecording = false
        isPaused = false
        currentFilePath = nil
        recordingStartTime = nil
        pausedDuration = 0
        pauseStartTime = nil
        wasRecordingBeforeInterruption = false
        
        // Properly deactivate audio session with error handling
        do {
            try AVAudioSession.sharedInstance().setActive(false, options: .notifyOthersOnDeactivation)
            NSLog("[AudioRecorder]   Audio session deactivated")
        } catch {
            NSLog("[AudioRecorder]   Failed to deactivate audio session: \(error.localizedDescription)")
            // Continue cleanup even if deactivation fails
        }
        NSLog("[AudioRecorder]   Cleanup complete")
    }
}

@_cdecl("init_plugin_audio_recorder")
func initPlugin() -> Plugin {
    return AudioRecorderPlugin()
}
