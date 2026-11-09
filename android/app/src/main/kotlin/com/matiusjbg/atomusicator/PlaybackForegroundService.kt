package com.matiusjbg.atomusicator

import android.app.Notification
import android.app.NotificationChannel
import android.app.NotificationManager
import android.app.PendingIntent
import android.app.Service
import android.content.BroadcastReceiver
import android.content.Context
import android.content.Intent
import android.content.IntentFilter
import android.content.pm.ServiceInfo
import android.media.AudioAttributes
import android.media.AudioFocusRequest
import android.media.AudioManager
import android.os.Build
import android.os.Handler
import android.os.IBinder
import android.os.Looper
import android.os.PowerManager
import android.support.v4.media.session.MediaSessionCompat
import android.support.v4.media.session.PlaybackStateCompat
import androidx.core.app.NotificationCompat
import androidx.media.app.NotificationCompat.MediaStyle

class PlaybackForegroundService : Service() {
    private var mediaSession: MediaSessionCompat? = null
    private var audioFocusRequest: AudioFocusRequest? = null
    private var wakeLock: PowerManager.WakeLock? = null
    private var isPlaying = false
    private var wasPlayingBeforeTransientLoss = false
    private var isDucked = false
    private var becomingNoisyRegistered = false
    private var currentTitle = "Reproduciendo la mezcla"

    private val becomingNoisyReceiver = object : BroadcastReceiver() {
        override fun onReceive(context: Context?, intent: Intent?) {
            if (intent?.action == AudioManager.ACTION_AUDIO_BECOMING_NOISY) {
                dispatchTransportAction("pause")
            }
        }
    }

    override fun onBind(intent: Intent?): IBinder? = null

    override fun onCreate() {
        super.onCreate()
        ensureNotificationChannel()
        initMediaSession()
    }

    override fun onStartCommand(intent: Intent?, flags: Int, startId: Int): Int {
        when (intent?.action) {
            ACTION_PLAY -> dispatchTransportAction("play")
            ACTION_PAUSE -> dispatchTransportAction("pause")
            ACTION_STOP -> {
                dispatchTransportAction("stop")
                stopSelf()
                return START_NOT_STICKY
            }
            ACTION_UPDATE_STATE -> {
                val playing = intent.getBooleanExtra(EXTRA_IS_PLAYING, false)
                val title = intent.getStringExtra(EXTRA_TITLE) ?: currentTitle
                currentTitle = title
                updateState(playing, title)
            }
            else -> {
                // Initial start
                val granted = requestAudioFocus()
                if (granted) {
                    isPlaying = true
                    acquireWakeLock()
                    registerNoisyReceiver()
                    val notification = buildMediaNotification(isPlaying, currentTitle)
                    if (Build.VERSION.SDK_INT >= 29) {
                        startForeground(
                            NOTIFICATION_ID,
                            notification,
                            ServiceInfo.FOREGROUND_SERVICE_TYPE_MEDIA_PLAYBACK,
                        )
                    } else {
                        startForeground(NOTIFICATION_ID, notification)
                    }
                    updatePlaybackState(PlaybackStateCompat.STATE_PLAYING)
                } else {
                    stopSelf()
                    return START_NOT_STICKY
                }
            }
        }
        return START_NOT_STICKY
    }

    private fun initMediaSession() {
        mediaSession = MediaSessionCompat(this, "AtomusicatorSession").apply {
            setCallback(object : MediaSessionCompat.Callback() {
                override fun onPlay() = dispatchTransportAction("play")
                override fun onPause() = dispatchTransportAction("pause")
                override fun onStop() {
                    dispatchTransportAction("stop")
                    stopSelf()
                }
            })
            isActive = true
        }
    }

    private fun updateState(playing: Boolean, title: String) {
        isPlaying = playing
        updatePlaybackState(
            if (playing) PlaybackStateCompat.STATE_PLAYING else PlaybackStateCompat.STATE_PAUSED
        )
        if (playing) {
            acquireWakeLock()
            registerNoisyReceiver()
        } else {
            releaseWakeLock()
            unregisterNoisyReceiver()
        }
        val manager = getSystemService(NotificationManager::class.java)
        manager.notify(NOTIFICATION_ID, buildMediaNotification(playing, title))
    }

    private fun buildMediaNotification(playing: Boolean, trackTitle: String): Notification {
        val session = mediaSession ?: return Notification.Builder(this, CHANNEL_ID).build()

        val contentIntent = PendingIntent.getActivity(
            this,
            0,
            Intent(this, MainActivity::class.java).apply {
                flags = Intent.FLAG_ACTIVITY_SINGLE_TOP or Intent.FLAG_ACTIVITY_CLEAR_TOP
            },
            PendingIntent.FLAG_IMMUTABLE or PendingIntent.FLAG_UPDATE_CURRENT,
        )

        val toggleAction = if (playing) {
            val pauseIntent = PendingIntent.getService(
                this,
                1,
                Intent(this, PlaybackForegroundService::class.java).apply { action = ACTION_PAUSE },
                PendingIntent.FLAG_IMMUTABLE or PendingIntent.FLAG_UPDATE_CURRENT,
            )
            NotificationCompat.Action(
                android.R.drawable.ic_media_pause,
                "Pausar",
                pauseIntent,
            )
        } else {
            val playIntent = PendingIntent.getService(
                this,
                2,
                Intent(this, PlaybackForegroundService::class.java).apply { action = ACTION_PLAY },
                PendingIntent.FLAG_IMMUTABLE or PendingIntent.FLAG_UPDATE_CURRENT,
            )
            NotificationCompat.Action(
                android.R.drawable.ic_media_play,
                "Reproducir",
                playIntent,
            )
        }

        val stopIntent = PendingIntent.getService(
            this,
            3,
            Intent(this, PlaybackForegroundService::class.java).apply { action = ACTION_STOP },
            PendingIntent.FLAG_IMMUTABLE or PendingIntent.FLAG_UPDATE_CURRENT,
        )
        val stopAction = NotificationCompat.Action(
            android.R.drawable.ic_menu_close_clear_cancel,
            "Detener",
            stopIntent,
        )

        val mediaStyle = MediaStyle()
            .setMediaSession(session.sessionToken)
            .setShowActionsInCompactView(0, 1)

        return NotificationCompat.Builder(this, CHANNEL_ID)
            .setContentTitle("Atomusicator")
            .setContentText(trackTitle)
            .setSmallIcon(if (playing) android.R.drawable.ic_media_play else android.R.drawable.ic_media_pause)
            .setContentIntent(contentIntent)
            .setOngoing(playing)
            .setVisibility(NotificationCompat.VISIBILITY_PUBLIC)
            .setStyle(mediaStyle)
            .addAction(toggleAction)
            .addAction(stopAction)
            .build()
    }

    private fun updatePlaybackState(state: Int) {
        val actions = PlaybackStateCompat.ACTION_PLAY or
                PlaybackStateCompat.ACTION_PAUSE or
                PlaybackStateCompat.ACTION_STOP or
                PlaybackStateCompat.ACTION_PLAY_PAUSE

        val stateBuilder = PlaybackStateCompat.Builder()
            .setActions(actions)
            .setState(state, PlaybackStateCompat.PLAYBACK_POSITION_UNKNOWN, 1.0f)

        mediaSession?.setPlaybackState(stateBuilder.build())
    }

    private fun requestAudioFocus(): Boolean {
        val audioManager = getSystemService(AudioManager::class.java)
        val focusListener = AudioManager.OnAudioFocusChangeListener { focusChange ->
            handleAudioFocusChange(focusChange)
        }

        return if (Build.VERSION.SDK_INT >= 26) {
            val attributes = AudioAttributes.Builder()
                .setUsage(AudioAttributes.USAGE_MEDIA)
                .setContentType(AudioAttributes.CONTENT_TYPE_MUSIC)
                .build()

            val request = AudioFocusRequest.Builder(AudioManager.AUDIOFOCUS_GAIN)
                .setAudioAttributes(attributes)
                .setAcceptsDelayedFocusGain(true)
                .setWillPauseWhenDucked(false)
                .setOnAudioFocusChangeListener(focusListener, Handler(Looper.getMainLooper()))
                .build()

            audioFocusRequest = request
            audioManager.requestAudioFocus(request) == AudioManager.AUDIOFOCUS_REQUEST_GRANTED
        } else {
            @Suppress("DEPRECATION")
            audioManager.requestAudioFocus(
                focusListener,
                AudioManager.STREAM_MUSIC,
                AudioManager.AUDIOFOCUS_GAIN,
            ) == AudioManager.AUDIOFOCUS_REQUEST_GRANTED
        }
    }

    private fun handleAudioFocusChange(focusChange: Int) {
        when (focusChange) {
            AudioManager.AUDIOFOCUS_GAIN -> {
                if (isDucked) {
                    isDucked = false
                    dispatchAudioFocusEvent("unduck", 1.0)
                }
                if (wasPlayingBeforeTransientLoss) {
                    wasPlayingBeforeTransientLoss = false
                    dispatchTransportAction("play")
                }
            }
            AudioManager.AUDIOFOCUS_LOSS_TRANSIENT_CAN_DUCK -> {
                isDucked = true
                dispatchAudioFocusEvent("duck", 0.25)
            }
            AudioManager.AUDIOFOCUS_LOSS_TRANSIENT -> {
                if (isPlaying) {
                    wasPlayingBeforeTransientLoss = true
                    dispatchTransportAction("pause")
                }
            }
            AudioManager.AUDIOFOCUS_LOSS -> {
                wasPlayingBeforeTransientLoss = false
                isDucked = false
                dispatchAudioFocusEvent("permanent_loss", 0.0)
                dispatchTransportAction("stop")
                stopSelf()
            }
        }
    }

    private fun abandonAudioFocus() {
        val audioManager = getSystemService(AudioManager::class.java)
        if (Build.VERSION.SDK_INT >= 26) {
            audioFocusRequest?.let { audioManager.abandonAudioFocusRequest(it) }
            audioFocusRequest = null
        } else {
            @Suppress("DEPRECATION")
            audioManager.abandonAudioFocus(null)
        }
    }

    private fun registerNoisyReceiver() {
        if (!becomingNoisyRegistered) {
            registerReceiver(
                becomingNoisyReceiver,
                IntentFilter(AudioManager.ACTION_AUDIO_BECOMING_NOISY),
            )
            becomingNoisyRegistered = true
        }
    }

    private fun unregisterNoisyReceiver() {
        if (becomingNoisyRegistered) {
            try {
                unregisterReceiver(becomingNoisyReceiver)
            } catch (_: Exception) {}
            becomingNoisyRegistered = false
        }
    }

    private fun acquireWakeLock() {
        val power = getSystemService(PowerManager::class.java)
        if (wakeLock == null) {
            wakeLock = power.newWakeLock(PowerManager.PARTIAL_WAKE_LOCK, "Atomusicator:playback").apply {
                setReferenceCounted(false)
            }
        }
        if (wakeLock?.isHeld != true) {
            wakeLock?.acquire(MAX_PLAYBACK_WAKE_LOCK_MS)
        }
    }

    private fun releaseWakeLock() {
        if (wakeLock?.isHeld == true) {
            wakeLock?.release()
        }
    }

    private fun ensureNotificationChannel() {
        val manager = getSystemService(NotificationManager::class.java)
        if (manager.getNotificationChannel(CHANNEL_ID) == null) {
            val channel = NotificationChannel(
                CHANNEL_ID,
                "Reproducción Multimedia",
                NotificationManager.IMPORTANCE_LOW,
            ).apply {
                description = "Controles de reproducción multimedia de Atomusicator"
                setShowBadge(false)
            }
            manager.createNotificationChannel(channel)
        }
    }

    private fun dispatchTransportAction(action: String) {
        onTransportCommand?.invoke(action)
    }

    private fun dispatchAudioFocusEvent(event: String, volumeScale: Double) {
        onAudioFocusEvent?.invoke(event, volumeScale)
        if (event == "transient_loss" || event == "permanent_loss") {
            onFocusLost?.invoke()
        }
    }

    override fun onDestroy() {
        releaseWakeLock()
        unregisterNoisyReceiver()
        abandonAudioFocus()
        mediaSession?.release()
        mediaSession = null
        super.onDestroy()
    }

    companion object {
        const val CHANNEL_ID = "atomusicator.playback"
        const val NOTIFICATION_ID = 41
        const val MAX_PLAYBACK_WAKE_LOCK_MS = 2 * 60 * 60 * 1000L // 2-hour ceiling while active

        const val ACTION_PLAY = "com.matiusjbg.atomusicator.ACTION_PLAY"
        const val ACTION_PAUSE = "com.matiusjbg.atomusicator.ACTION_PAUSE"
        const val ACTION_STOP = "com.matiusjbg.atomusicator.ACTION_STOP"
        const val ACTION_UPDATE_STATE = "com.matiusjbg.atomusicator.ACTION_UPDATE_STATE"

        const val EXTRA_IS_PLAYING = "extra_is_playing"
        const val EXTRA_TITLE = "extra_title"

        var onTransportCommand: ((String) -> Unit)? = null
        var onAudioFocusEvent: ((String, Double) -> Unit)? = null
        var onFocusLost: (() -> Unit)? = null

        fun requestFocus(context: Context): Boolean {
            val audio = context.getSystemService(AudioManager::class.java)
            if (Build.VERSION.SDK_INT >= 26) {
                val request = AudioFocusRequest.Builder(AudioManager.AUDIOFOCUS_GAIN)
                    .setAudioAttributes(
                        AudioAttributes.Builder()
                            .setUsage(AudioAttributes.USAGE_MEDIA)
                            .setContentType(AudioAttributes.CONTENT_TYPE_MUSIC)
                            .build(),
                    )
                    .setOnAudioFocusChangeListener { change ->
                        if (
                            change == AudioManager.AUDIOFOCUS_LOSS ||
                            change == AudioManager.AUDIOFOCUS_LOSS_TRANSIENT
                        ) {
                            onFocusLost?.invoke()
                        }
                    }
                    .build()
                return audio.requestAudioFocus(request) == AudioManager.AUDIOFOCUS_REQUEST_GRANTED
            } else {
                @Suppress("DEPRECATION")
                return audio.requestAudioFocus(
                    { change ->
                        if (
                            change == AudioManager.AUDIOFOCUS_LOSS ||
                            change == AudioManager.AUDIOFOCUS_LOSS_TRANSIENT
                        ) {
                            onFocusLost?.invoke()
                        }
                    },
                    AudioManager.STREAM_MUSIC,
                    AudioManager.AUDIOFOCUS_GAIN,
                ) == AudioManager.AUDIOFOCUS_REQUEST_GRANTED
            }
        }
    }
}
