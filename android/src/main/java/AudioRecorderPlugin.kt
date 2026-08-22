package io.affex.audio_recorder

import android.Manifest
import android.app.Activity
import android.content.Context
import android.content.pm.PackageManager
import android.media.AudioAttributes
import android.media.AudioFocusRequest
import android.media.AudioManager
import android.media.MediaRecorder
import android.os.Build
import android.os.Handler
import android.os.Looper
import android.util.Log
import androidx.core.app.ActivityCompat
import androidx.core.content.ContextCompat
import app.tauri.annotation.Command
import app.tauri.annotation.InvokeArg
import app.tauri.annotation.Permission
import app.tauri.annotation.PermissionCallback
import app.tauri.annotation.TauriPlugin
import app.tauri.plugin.JSArray
import app.tauri.plugin.JSObject
import app.tauri.plugin.Plugin
import app.tauri.plugin.Invoke
import java.io.File

@InvokeArg
class RecordingConfigArgs {
    var outputPath: String = ""
    var format: String = "wav"
    var quality: String = "medium"
    var maxDuration: Int = 0
}

@TauriPlugin(
    permissions = [
        Permission(strings = [Manifest.permission.RECORD_AUDIO], alias = "microphone")
    ]
)
class AudioRecorderPlugin(private val activity: Activity) : Plugin(activity) {
    private var mediaRecorder: MediaRecorder? = null
    private var currentFilePath: String? = null
    private var recordingStartTime: Long = 0
    private var pausedDuration: Long = 0
    private var pauseStartTime: Long = 0
    private var isPaused: Boolean = false
    private var isRecording: Boolean = false
    private var isStopping: Boolean = false  // Guard against concurrent stop calls
    private var currentSampleRate: Int = 44100
    private var currentChannels: Int = 1
    private var maxDurationHandler: Handler? = null
    private var maxDurationRunnable: Runnable? = null
    
    // Audio focus handling
    private var audioManager: AudioManager? = null
    private var audioFocusRequest: AudioFocusRequest? = null
    private var hasAudioFocus: Boolean = false

    companion object {
        private const val TAG = "AudioRecorderPlugin"
        private const val REQUEST_RECORD_AUDIO = 1001
    }
    
    init {
        Log.d(TAG, "============================================")
        Log.d(TAG, "AudioRecorderPlugin INIT")
        Log.d(TAG, "  Package: ${activity.packageName}")
        Log.d(TAG, "  Android SDK: ${Build.VERSION.SDK_INT}")
        Log.d(TAG, "  Device: ${Build.MANUFACTURER} ${Build.MODEL}")
        audioManager = activity.getSystemService(Context.AUDIO_SERVICE) as? AudioManager
        Log.d(TAG, "  AudioManager initialized: ${audioManager != null}")
        Log.d(TAG, "============================================")
    }
    
    /**
     * Request audio focus before recording.
     * Returns true if focus was granted, false otherwise.
     */
    private fun requestAudioFocus(): Boolean {
        Log.d(TAG, "requestAudioFocus() called")
        Log.d(TAG, "  Current hasAudioFocus: $hasAudioFocus")
        
        if (hasAudioFocus) {
            Log.d(TAG, "  Already have audio focus, returning true")
            return true
        }
        
        val result = if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.O) {
            Log.d(TAG, "  Using AudioFocusRequest (API 26+)")
            val focusRequest = AudioFocusRequest.Builder(AudioManager.AUDIOFOCUS_GAIN_TRANSIENT_EXCLUSIVE)
                .setAudioAttributes(
                    AudioAttributes.Builder()
                        .setUsage(AudioAttributes.USAGE_MEDIA)
                        .setContentType(AudioAttributes.CONTENT_TYPE_SPEECH)
                        .build()
                )
                .setOnAudioFocusChangeListener { focusChange ->
                    Log.d(TAG, "Audio focus changed: $focusChange")
                    when (focusChange) {
                        AudioManager.AUDIOFOCUS_LOSS -> Log.w(TAG, "  AUDIOFOCUS_LOSS")
                        AudioManager.AUDIOFOCUS_LOSS_TRANSIENT -> Log.w(TAG, "  AUDIOFOCUS_LOSS_TRANSIENT")
                        AudioManager.AUDIOFOCUS_LOSS_TRANSIENT_CAN_DUCK -> Log.d(TAG, "  AUDIOFOCUS_LOSS_TRANSIENT_CAN_DUCK")
                        AudioManager.AUDIOFOCUS_GAIN -> Log.d(TAG, "  AUDIOFOCUS_GAIN")
                    }
                }
                .build()
            audioFocusRequest = focusRequest
            audioManager?.requestAudioFocus(focusRequest)
        } else {
            Log.d(TAG, "  Using deprecated requestAudioFocus (API < 26)")
            @Suppress("DEPRECATION")
            audioManager?.requestAudioFocus(
                null,
                AudioManager.STREAM_MUSIC,
                AudioManager.AUDIOFOCUS_GAIN_TRANSIENT_EXCLUSIVE
            )
        }
        
        hasAudioFocus = result == AudioManager.AUDIOFOCUS_REQUEST_GRANTED
        Log.d(TAG, "  Audio focus request result: $result (granted=$hasAudioFocus)")
        
        if (!hasAudioFocus) {
            Log.w(TAG, "Could not obtain audio focus for recording")
        }
        return hasAudioFocus
    }
    
    /**
     * Release audio focus after recording stops.
     */
    private fun releaseAudioFocus() {
        Log.d(TAG, "releaseAudioFocus() called")
        Log.d(TAG, "  Current hasAudioFocus: $hasAudioFocus")
        
        if (!hasAudioFocus) {
            Log.d(TAG, "  No audio focus to release")
            return
        }
        
        if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.O) {
            audioFocusRequest?.let { 
                audioManager?.abandonAudioFocusRequest(it)
                Log.d(TAG, "  Audio focus released (API 26+)")
            }
        } else {
            @Suppress("DEPRECATION")
            audioManager?.abandonAudioFocus(null)
            Log.d(TAG, "  Audio focus released (deprecated API)")
        }
        hasAudioFocus = false
        audioFocusRequest = null
        Log.d(TAG, "  Audio focus released successfully")
    }

    @Command
    fun startRecording(invoke: Invoke) {
        Log.i(TAG, "============================================")
        Log.i(TAG, "startRecording() CALLED")
        val config = invoke.parseArgs(RecordingConfigArgs::class.java)
        Log.d(TAG, "  Config:")
        Log.d(TAG, "    outputPath: ${config.outputPath}")
        Log.d(TAG, "    format: ${config.format}")
        Log.d(TAG, "    quality: ${config.quality}")
        Log.d(TAG, "    maxDuration: ${config.maxDuration}")

        // Check permission
        val permissionStatus = ContextCompat.checkSelfPermission(activity, Manifest.permission.RECORD_AUDIO)
        Log.d(TAG, "  Permission RECORD_AUDIO: ${if (permissionStatus == PackageManager.PERMISSION_GRANTED) "GRANTED" else "DENIED"}")
        
        if (permissionStatus != PackageManager.PERMISSION_GRANTED) {
            Log.w(TAG, "  Microphone permission not granted, requesting...")
            ActivityCompat.requestPermissions(
                activity,
                arrayOf(Manifest.permission.RECORD_AUDIO),
                REQUEST_RECORD_AUDIO
            )
            invoke.reject("Microphone permission required. Please grant permission and try again.")
            return
        }

        if (isRecording) {
            Log.w(TAG, "  Already recording, rejecting")
            invoke.reject("Already recording")
            return
        }
        
        Log.d(TAG, "  Current state: isRecording=$isRecording, isPaused=$isPaused")

        try {
            when (config.quality.lowercase()) {
                "low" -> {
                    currentSampleRate = 16000
                    currentChannels = 1
                }
                "high" -> {
                    currentSampleRate = 48000
                    currentChannels = 2
                }
                else -> {
                    currentSampleRate = 44100
                    currentChannels = 1
                }
            }

            // Always use cache directory to avoid EROFS (Read-only file system) errors
            val cacheDir = activity.cacheDir
            val filename = if (config.outputPath.isNotEmpty()) {
                // Extract only the filename, removing any path separators and extensions
                config.outputPath
                    .substringAfterLast('/')
                    .removeSuffix(".m4a")
                    .removeSuffix(".wav")
                    .removeSuffix(".aac")
            } else {
                // Generate timestamp-based filename
                val timestamp = System.currentTimeMillis()
                "recording_${timestamp}"
            }
            // Build full absolute path in cache directory
            val filePath = "${cacheDir.absolutePath}/${filename}.m4a"
            currentFilePath = filePath

            Log.i(TAG, "Recording to: $filePath (cache dir) with quality: ${config.quality}")
            
            // Request audio focus before recording
            if (!requestAudioFocus()) {
                Log.w(TAG, "Starting recording without audio focus")
            }

            mediaRecorder = if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.S) {
                MediaRecorder(activity)
            } else {
                @Suppress("DEPRECATION")
                MediaRecorder()
            }

            mediaRecorder?.apply {
                try {
                    setAudioSource(MediaRecorder.AudioSource.MIC)
                    Log.d(TAG, "Audio source set: MIC")
                    
                    // Use MPEG_4 container format for better compatibility
                    setOutputFormat(MediaRecorder.OutputFormat.MPEG_4)
                    Log.d(TAG, "Output format set: MPEG_4")
                    
                    setAudioEncoder(MediaRecorder.AudioEncoder.AAC)
                    Log.d(TAG, "Audio encoder set: AAC")
                    
                    setAudioSamplingRate(currentSampleRate)
                    setAudioEncodingBitRate(128000)
                    setAudioChannels(currentChannels)

                    setOutputFile(filePath)
                    Log.d(TAG, "Output file set: $filePath")

                    Log.i(TAG, "MediaRecorder configured: format=MPEG_4, encoder=AAC, sampleRate=$currentSampleRate, channels=$currentChannels, bitRate=128000")

                    prepare()
                    Log.d(TAG, "MediaRecorder prepared")
                    
                    start()
                    Log.i(TAG, "MediaRecorder started successfully")
                } catch (e: Exception) {
                    Log.e(TAG, "Error configuring MediaRecorder: ${e.message}", e)
                    throw e
                }
            }

            isRecording = true
            isPaused = false
            recordingStartTime = System.currentTimeMillis()
            pausedDuration = 0

            if (config.maxDuration > 0) {
                maxDurationHandler = Handler(Looper.getMainLooper())
                maxDurationRunnable = Runnable {
                    try {
                        stopRecordingInternal()
                    } catch (e: Exception) {
                        Log.e(TAG, "Error auto-stopping at maxDuration: ${e.message}", e)
                    }
                }
                maxDurationHandler?.postDelayed(maxDurationRunnable!!, config.maxDuration * 1000L)
            }

            Log.i(TAG, "Started recording to $filePath")
            invoke.resolve()

        } catch (e: Exception) {
            Log.e(TAG, "Failed to start recording: ${e.message}")
            cleanup()
            invoke.reject("Failed to start recording: ${e.message}")
        }
    }

    @Command
    fun stopRecording(invoke: Invoke) {
        Log.i(TAG, "============================================")
        Log.i(TAG, "stopRecording() CALLED")
        Log.d(TAG, "  Current state: isRecording=$isRecording, isPaused=$isPaused, isStopping=$isStopping")
        
        if (!isRecording) {
            Log.w(TAG, "  Not recording, rejecting")
            invoke.reject("Not recording")
            return
        }

        try {
            val result = stopRecordingInternal()
            if (result != null) {
                Log.i(TAG, "  Recording stopped successfully")
                Log.d(TAG, "  Result: filePath=${result.getString("filePath")}")
                Log.d(TAG, "  Result: durationMs=${result.getLong("durationMs")}")
                Log.d(TAG, "  Result: fileSize=${result.getLong("fileSize")}")
                invoke.resolve(result)
            } else {
                Log.e(TAG, "  stopRecordingInternal returned null")
                invoke.reject("Failed to stop recording")
            }
        } catch (e: Exception) {
            Log.e(TAG, "Failed to stop recording: ${e.message}", e)
            cleanup()
            invoke.reject("Failed to stop recording: ${e.message}")
        }
        Log.i(TAG, "============================================")
    }

    private fun stopRecordingInternal(): JSObject? {
        // Guard against concurrent stop calls (e.g., maxDuration timer + manual stop)
        synchronized(this) {
            if (isStopping) {
                Log.d(TAG, "Stop already in progress, ignoring duplicate call")
                return null
            }
            isStopping = true
        }
        
        try {
            maxDurationRunnable?.let { maxDurationHandler?.removeCallbacks(it) }
            maxDurationHandler = null
            maxDurationRunnable = null
            
            // Release audio focus
            releaseAudioFocus()

            val filePath = currentFilePath ?: return null

            try {
                mediaRecorder?.apply {
                    stop()
                    Log.d(TAG, "MediaRecorder stopped")
                    release()
                    Log.d(TAG, "MediaRecorder released")
                }
                mediaRecorder = null

                val endTime = System.currentTimeMillis()
                val totalDuration = if (isPaused) {
                    pauseStartTime - recordingStartTime - pausedDuration
                } else {
                    endTime - recordingStartTime - pausedDuration
                }

                val file = File(filePath)
                val fileSize = if (file.exists()) {
                    val size = file.length()
                    Log.i(TAG, "Recording file size: $size bytes")
                    size
                } else {
                    Log.w(TAG, "Recording file does not exist: $filePath")
                    0L
                }

                val ret = JSObject()
                ret.put("filePath", filePath)
                ret.put("durationMs", totalDuration)
                ret.put("fileSize", fileSize)
                ret.put("sampleRate", currentSampleRate)
                ret.put("channels", currentChannels)

                Log.i(TAG, "Stopped recording: $filePath (${totalDuration}ms, $fileSize bytes)")

                cleanup()
                return ret

            } catch (e: Exception) {
                cleanup()
                throw e
            }
        } finally {
            isStopping = false
        }
    }

    @Command
    fun pauseRecording(invoke: Invoke) {
        Log.i(TAG, "pauseRecording() CALLED")
        Log.d(TAG, "  Current state: isRecording=$isRecording, isPaused=$isPaused")
        Log.d(TAG, "  Android SDK: ${Build.VERSION.SDK_INT} (need >= ${Build.VERSION_CODES.N} for pause)")
        
        if (!isRecording) {
            Log.w(TAG, "  Not recording, rejecting")
            invoke.reject("Not recording")
            return
        }

        if (isPaused) {
            Log.w(TAG, "  Already paused, rejecting")
            invoke.reject("Already paused")
            return
        }

        try {
            if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.N) {
                mediaRecorder?.pause()
                isPaused = true
                pauseStartTime = System.currentTimeMillis()
                Log.i(TAG, "Recording paused at ${pauseStartTime}")
                invoke.resolve()
            } else {
                Log.w(TAG, "  Pause not supported on this Android version")
                invoke.reject("Pause not supported on this Android version")
            }
        } catch (e: Exception) {
            Log.e(TAG, "Failed to pause: ${e.message}", e)
            invoke.reject("Failed to pause: ${e.message}")
        }
    }

    @Command
    fun resumeRecording(invoke: Invoke) {
        Log.i(TAG, "resumeRecording() CALLED")
        Log.d(TAG, "  Current state: isRecording=$isRecording, isPaused=$isPaused")
        
        if (!isRecording) {
            Log.w(TAG, "  Not recording, rejecting")
            invoke.reject("Not recording")
            return
        }

        if (!isPaused) {
            Log.w(TAG, "  Not paused, rejecting")
            invoke.reject("Not paused")
            return
        }

        try {
            if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.N) {
                mediaRecorder?.resume()
                val pauseDuration = System.currentTimeMillis() - pauseStartTime
                pausedDuration += pauseDuration
                isPaused = false
                Log.i(TAG, "Recording resumed (was paused for ${pauseDuration}ms, total paused: ${pausedDuration}ms)")
                invoke.resolve()
            } else {
                Log.w(TAG, "  Resume not supported on this Android version")
                invoke.reject("Resume not supported on this Android version")
            }
        } catch (e: Exception) {
            Log.e(TAG, "Failed to resume: ${e.message}", e)
            invoke.reject("Failed to resume: ${e.message}")
        }
    }

    @Command
    fun getStatus(invoke: Invoke) {
        Log.d(TAG, "getStatus() CALLED")
        val ret = JSObject()

        val state = when {
            !isRecording -> "idle"
            isPaused -> "paused"
            else -> "recording"
        }
        ret.put("state", state)

        val durationMs = if (isRecording) {
            if (isPaused) {
                pauseStartTime - recordingStartTime - pausedDuration
            } else {
                System.currentTimeMillis() - recordingStartTime - pausedDuration
            }
        } else {
            0L
        }
        ret.put("durationMs", durationMs)
        ret.put("outputPath", currentFilePath)
        
        Log.d(TAG, "  State: $state, Duration: ${durationMs}ms, Path: $currentFilePath")

        invoke.resolve(ret)
    }

    @Command
    fun getDevices(invoke: Invoke) {
        Log.d(TAG, "getDevices() CALLED")
        val devicesArray = JSArray()

        val defaultDevice = JSObject()
        defaultDevice.put("id", "default")
        defaultDevice.put("name", "Default Microphone")
        defaultDevice.put("isDefault", true)
        devicesArray.put(defaultDevice)

        val ret = JSObject()
        ret.put("devices", devicesArray)
        Log.d(TAG, "  Returning ${devicesArray.length()} devices")
        invoke.resolve(ret)
    }

    @Command
    fun checkPermission(invoke: Invoke) {
        Log.d(TAG, "checkPermission() CALLED")
        val granted = ContextCompat.checkSelfPermission(activity, Manifest.permission.RECORD_AUDIO) ==
                PackageManager.PERMISSION_GRANTED

        // shouldShowRequestPermissionRationale() is true only once the user has already
        // denied once but not permanently ("don't ask again" / 2nd+ denial on newer
        // Android). It's false both before the very first request and after a
        // permanent denial — Android has no API to tell those two apart — so treat
        // "granted, or Android says a rationale makes sense" as requestable.
        val canRequest = granted || ActivityCompat.shouldShowRequestPermissionRationale(
            activity,
            Manifest.permission.RECORD_AUDIO
        )

        Log.d(TAG, "  Permission RECORD_AUDIO: granted=$granted, canRequest=$canRequest")

        val ret = JSObject()
        ret.put("granted", granted)
        ret.put("canRequest", canRequest)
        invoke.resolve(ret)
    }

    @Command
    fun requestPermission(invoke: Invoke) {
        val granted = ContextCompat.checkSelfPermission(activity, Manifest.permission.RECORD_AUDIO) ==
                PackageManager.PERMISSION_GRANTED

        if (granted) {
            val ret = JSObject()
            ret.put("granted", true)
            ret.put("canRequest", false)
            invoke.resolve(ret)
            return
        }

        requestPermissionForAlias("microphone", invoke, "permissionCallback")
    }

    @Command
    @PermissionCallback
    fun permissionCallback(invoke: Invoke) {
        val granted = ContextCompat.checkSelfPermission(activity, Manifest.permission.RECORD_AUDIO) ==
                PackageManager.PERMISSION_GRANTED
        val canRequest = granted || ActivityCompat.shouldShowRequestPermissionRationale(
            activity,
            Manifest.permission.RECORD_AUDIO
        )
        Log.d(TAG, "  permissionCallback: granted=$granted, canRequest=$canRequest")

        val ret = JSObject()
        ret.put("granted", granted)
        ret.put("canRequest", canRequest)
        invoke.resolve(ret)
    }

    private fun cleanup() {
        isRecording = false
        isPaused = false
        currentFilePath = null
        recordingStartTime = 0
        pausedDuration = 0
        pauseStartTime = 0
        try {
            mediaRecorder?.release()
        } catch (e: Exception) {
            Log.w(TAG, "Error releasing MediaRecorder during cleanup: ${e.message}")
        }
        mediaRecorder = null
    }

    override fun onDestroy() {
        maxDurationRunnable?.let { maxDurationHandler?.removeCallbacks(it) }
        mediaRecorder?.release()
        super.onDestroy()
    }
}
