package com.matiusjbg.atomusicator

import android.Manifest
import android.content.Intent
import android.content.pm.ActivityInfo
import android.database.Cursor
import android.net.Uri
import android.os.Build
import android.os.Bundle
import android.provider.OpenableColumns
import io.flutter.embedding.android.FlutterActivity
import io.flutter.embedding.engine.FlutterEngine
import io.flutter.plugin.common.MethodChannel
import java.io.File
import java.util.concurrent.Executors

class MainActivity : FlutterActivity() {
    private var hostChannel: MethodChannel? = null
    private var pendingShared: String? = null
    private val backgroundExecutor = Executors.newFixedThreadPool(2) { runnable ->
        Thread {
            try {
                // Priority foreground avoids demotion by Android's energy-aware scheduler
                android.os.Process.setThreadPriority(android.os.Process.THREAD_PRIORITY_FOREGROUND)
            } catch (_: Throwable) {}
            runnable.run()
        }.apply {
            name = "Atomusicator-Background"
            priority = Thread.NORM_PRIORITY + 2
        }
    }

    override fun onCreate(savedInstanceState: Bundle?) {
        requestedOrientation = ActivityInfo.SCREEN_ORIENTATION_SENSOR_LANDSCAPE
        System.loadLibrary("c++_shared")
        System.loadLibrary("onnxruntime")
        System.loadLibrary("atomusicator_bootstrap_ffi")
        nativeInit(this)
        val appCtx = applicationContext
        backgroundExecutor.execute {
            try {
                ensureBundledModels(appCtx)
            } finally {
                nativeModelsReady()
            }
        }
        super.onCreate(savedInstanceState)
        if (Build.VERSION.SDK_INT >= 33) {
            requestPermissions(arrayOf(Manifest.permission.POST_NOTIFICATIONS), 41)
        }
        consumeShareIntent(intent)
        maybeRunSelfTest(intent)
    }

    override fun onNewIntent(intent: Intent) {
        super.onNewIntent(intent)
        setIntent(intent)
        consumeShareIntent(intent)
        maybeRunSelfTest(intent)
    }

    private fun maybeRunSelfTest(intent: Intent?) {
        if (intent?.getStringExtra("atomusicator_self_test") == "run") {
            backgroundExecutor.execute {
                android.util.Log.i("AtomusicatorE2E", "Starting native E2E verification...")
                val report = try {
                    nativeVerifyPipeline()
                } catch (t: Throwable) {
                    "E2E_EXC: ${t.message}"
                }
                android.util.Log.i("AtomusicatorE2E", report ?: "E2E_NULL")
            }
        }
    }

    override fun configureFlutterEngine(flutterEngine: FlutterEngine) {
        super.configureFlutterEngine(flutterEngine)
        val channel = MethodChannel(flutterEngine.dartExecutor.binaryMessenger, CHANNEL)
        hostChannel = channel

        PlaybackForegroundService.onFocusLost = {
            runOnUiThread { channel.invokeMethod("audioFocusLost", null) }
        }

        PlaybackForegroundService.onAudioFocusEvent = { event, volume ->
            runOnUiThread {
                channel.invokeMethod(
                    "onAudioFocusEvent",
                    mapOf("event" to event, "volume" to volume),
                )
            }
        }

        PlaybackForegroundService.onTransportCommand = { action ->
            runOnUiThread {
                channel.invokeMethod("onTransportCommand", mapOf("action" to action))
            }
        }

        SeparationForegroundService.onCancelSeparationCommand = {
            runOnUiThread {
                channel.invokeMethod("onCancelSeparation", null)
            }
        }

        channel.setMethodCallHandler { call, result ->
            when (call.method) {
                "takeSharedAudio" -> {
                    val path = pendingShared
                    pendingShared = null
                    result.success(path)
                }
                "shareFiles" -> {
                    val paths = call.argument<List<String>>("paths").orEmpty()
                    shareFiles(paths)
                    result.success(null)
                }
                "playback" -> {
                    val active = call.argument<Boolean>("active") == true
                    val serviceIntent = Intent(this, PlaybackForegroundService::class.java)
                    if (!active) {
                        stopService(serviceIntent)
                        result.success(true)
                    } else {
                        if (Build.VERSION.SDK_INT >= 26) {
                            startForegroundService(serviceIntent)
                        } else {
                            startService(serviceIntent)
                        }
                        result.success(true)
                    }
                }
                "updatePlaybackState" -> {
                    val isPlaying = call.argument<Boolean>("isPlaying") ?: false
                    val title = call.argument<String>("title") ?: "Atomusicator"
                    val intent = Intent(this, PlaybackForegroundService::class.java).apply {
                        action = PlaybackForegroundService.ACTION_UPDATE_STATE
                        putExtra(PlaybackForegroundService.EXTRA_IS_PLAYING, isPlaying)
                        putExtra(PlaybackForegroundService.EXTRA_TITLE, title)
                    }
                    startService(intent)
                    result.success(null)
                }
                "startSeparationService" -> {
                    try {
                        // Boost thread priority so Android's energy-aware scheduler keeps inference on performance cores
                        android.os.Process.setThreadPriority(android.os.Process.THREAD_PRIORITY_FOREGROUND)
                    } catch (_: Throwable) {}
                    val title = call.argument<String>("title") ?: "Separando audio"
                    val intent = Intent(this, SeparationForegroundService::class.java).apply {
                        putExtra(SeparationForegroundService.EXTRA_TITLE, title)
                    }
                    if (Build.VERSION.SDK_INT >= 26) {
                        startForegroundService(intent)
                    } else {
                        startService(intent)
                    }
                    result.success(true)
                }
                "boostInferencePriority" -> {
                    try {
                        android.os.Process.setThreadPriority(android.os.Process.THREAD_PRIORITY_FOREGROUND)
                        result.success(true)
                    } catch (_: Throwable) {
                        result.success(false)
                    }
                }
                "updateSeparationProgress" -> {
                    val progress = call.argument<Int>("progress") ?: 0
                    val status = call.argument<String>("status") ?: "Procesando..."
                    val intent = Intent(this, SeparationForegroundService::class.java).apply {
                        action = SeparationForegroundService.ACTION_UPDATE_PROGRESS
                        putExtra(SeparationForegroundService.EXTRA_PROGRESS, progress)
                        putExtra(SeparationForegroundService.EXTRA_STATUS, status)
                    }
                    startService(intent)
                    result.success(null)
                }
                "stopSeparationService" -> {
                    val intent = Intent(this, SeparationForegroundService::class.java).apply {
                        action = SeparationForegroundService.ACTION_STOP
                    }
                    startService(intent)
                    result.success(true)
                }
                "previewAudio" -> {
                    val path = call.argument<String>("path")
                    val buckets = call.argument<Int>("buckets") ?: 650
                    if (path.isNullOrBlank()) {
                        result.success(null)
                    } else {
                        backgroundExecutor.execute {
                            val json = try {
                                nativePreviewAudio(path, buckets)
                            } catch (_: Throwable) {
                                null
                            }
                            runOnUiThread { result.success(json) }
                        }
                    }
                }
                else -> result.notImplemented()
            }
        }
    }

    override fun onDestroy() {
        PlaybackForegroundService.onFocusLost = null
        PlaybackForegroundService.onAudioFocusEvent = null
        PlaybackForegroundService.onTransportCommand = null
        SeparationForegroundService.onCancelSeparationCommand = null
        hostChannel = null
        backgroundExecutor.shutdown()
        super.onDestroy()
    }

    private fun consumeShareIntent(intent: Intent?) {
        if (intent == null) return
        val uri =
            when (intent.action) {
                Intent.ACTION_SEND -> extraStream(intent)
                Intent.ACTION_VIEW -> intent.data
                else -> null
            } ?: return
        val rawName = displayName(uri)?.ifBlank { null } ?: "audio.mp3"
        val safeName = rawName.replace(Regex("[\\\\/:*?\"<>|]"), "_")
        val directory = File(filesDir, "imports/${System.currentTimeMillis()}").apply { mkdirs() }
        val target = File(directory, safeName)
        contentResolver.openInputStream(uri)?.use { input ->
            target.outputStream().use { output -> input.copyTo(output) }
        } ?: return
        pendingShared = target.absolutePath
    }

    private fun extraStream(intent: Intent): Uri? =
        if (Build.VERSION.SDK_INT >= 33) {
            intent.getParcelableExtra(Intent.EXTRA_STREAM, Uri::class.java)
        } else {
            @Suppress("DEPRECATION")
            intent.getParcelableExtra(Intent.EXTRA_STREAM)
        }

    private fun displayName(uri: Uri): String? {
        if (uri.scheme == "file") return uri.lastPathSegment
        var cursor: Cursor? = null
        return try {
            cursor = contentResolver.query(uri, arrayOf(OpenableColumns.DISPLAY_NAME), null, null, null)
            if (cursor != null && cursor.moveToFirst()) cursor.getString(0) else uri.lastPathSegment
        } catch (_: Exception) {
            uri.lastPathSegment
        } finally {
            cursor?.close()
        }
    }

    private fun shareFiles(paths: List<String>) {
        val cache = File(cacheDir, "share").apply { mkdirs() }
        val uris = ArrayList<Uri>()
        for (path in paths) {
            val source = File(path)
            if (!source.isFile) continue
            val copy = File(cache, source.name)
            source.copyTo(copy, overwrite = true)
            uris += androidx.core.content.FileProvider.getUriForFile(
                this,
                "$packageName.fileprovider",
                copy,
            )
        }
        if (uris.isEmpty()) return
        val send =
            Intent(Intent.ACTION_SEND_MULTIPLE).apply {
                type = "audio/*"
                putParcelableArrayListExtra(Intent.EXTRA_STREAM, uris)
                addFlags(Intent.FLAG_GRANT_READ_URI_PERMISSION)
            }
        startActivity(Intent.createChooser(send, "Compartir audio"))
    }

    companion object {
        private const val CHANNEL = "atomusicator/host"
        private val MODEL_LOCK = Any()
        private val MODEL_FILENAMES = listOf(
            "UVR_MDXNET_Main.onnx",
            "UVR_MDXNET_KARA_2.onnx",
            "kuielab_a_drums.onnx",
            "kuielab_a_bass.onnx",
        )

        @JvmStatic
        fun ensureBundledModels(context: android.content.Context) {
            synchronized(MODEL_LOCK) {
                val modelsDir = File(context.filesDir, "Atomusicator/models").apply { mkdirs() }
                for (name in MODEL_FILENAMES) {
                    val target = File(modelsDir, name)
                    if (target.isFile && target.length() > 1_000_000L) continue
                    val temp = File(modelsDir, "$name.tmp")
                    var copied = false
                    try {
                        context.assets.open("models/$name").use { input ->
                            temp.outputStream().use { output -> input.copyTo(output, 262_144) }
                        }
                        if (temp.length() > 1_000_000L && temp.renameTo(target)) {
                            copied = true
                        }
                    } catch (_: Throwable) {
                        temp.delete()
                    }
                    if (!copied) {
                        val sdcardFallback = File("/sdcard/Download/AtomusicatorModels", name)
                        if (sdcardFallback.isFile && sdcardFallback.length() > 1_000_000L) {
                            try {
                                sdcardFallback.copyTo(temp, overwrite = true)
                                temp.renameTo(target)
                            } catch (_: Throwable) {
                                temp.delete()
                            }
                        }
                    }
                }
                val jobsDir = File(context.filesDir, "Atomusicator/jobs")
                if (jobsDir.isDirectory) {
                    jobsDir.listFiles()?.forEach { jobFile ->
                        try {
                            if (jobFile.isFile && jobFile.name.endsWith(".json")) {
                                val text = jobFile.readText()
                                if (text.contains("Models are missing or corrupt")) {
                                    jobFile.delete()
                                }
                            }
                        } catch (_: Throwable) {}
                    }
                }
            }
        }

        @JvmStatic
        private external fun nativeInit(context: android.content.Context)

        @JvmStatic
        private external fun nativeModelsReady()

        @JvmStatic
        private external fun nativePreviewAudio(path: String, buckets: Int): String?

        @JvmStatic
        private external fun nativeVerifyPipeline(): String?
    }
}
