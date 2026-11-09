package com.matiusjbg.atomusicator

import android.app.Notification
import android.app.NotificationChannel
import android.app.NotificationManager
import android.app.PendingIntent
import android.app.Service
import android.content.Context
import android.content.Intent
import android.content.pm.ServiceInfo
import android.os.Build
import android.os.IBinder
import android.os.PowerManager
import androidx.core.app.NotificationCompat

class SeparationForegroundService : Service() {
    private var wakeLock: PowerManager.WakeLock? = null

    override fun onBind(intent: Intent?): IBinder? = null

    override fun onCreate() {
        super.onCreate()
        try {
            // Promote foreground service thread priority to avoid energy-aware scheduler throttling
            android.os.Process.setThreadPriority(android.os.Process.THREAD_PRIORITY_FOREGROUND)
        } catch (_: Throwable) {}
        ensureNotificationChannel()
    }

    override fun onStartCommand(intent: Intent?, flags: Int, startId: Int): Int {
        try {
            android.os.Process.setThreadPriority(android.os.Process.THREAD_PRIORITY_FOREGROUND)
        } catch (_: Throwable) {}
        when (intent?.action) {
            ACTION_CANCEL -> {
                onCancelSeparationCommand?.invoke()
                releaseWakeLock()
                stopSelf()
                return START_NOT_STICKY
            }
            ACTION_UPDATE_PROGRESS -> {
                acquireBoundedWakeLock()
                val progress = intent.getIntExtra(EXTRA_PROGRESS, 0)
                val status = intent.getStringExtra(EXTRA_STATUS) ?: "Procesando..."
                val manager = getSystemService(NotificationManager::class.java)
                manager.notify(NOTIFICATION_ID, buildProgressNotification(progress, status))
            }
            ACTION_STOP -> {
                releaseWakeLock()
                stopSelf()
                return START_NOT_STICKY
            }
            else -> {
                // Initial start
                acquireBoundedWakeLock()
                val title = intent?.getStringExtra(EXTRA_TITLE) ?: "Separación de pistas"
                val notification = buildProgressNotification(0, "Iniciando modelos neuronales...")
                if (Build.VERSION.SDK_INT >= 34) {
                    startForeground(
                        NOTIFICATION_ID,
                        notification,
                        ServiceInfo.FOREGROUND_SERVICE_TYPE_MEDIA_PROCESSING,
                    )
                } else if (Build.VERSION.SDK_INT >= 29) {
                    startForeground(
                        NOTIFICATION_ID,
                        notification,
                        ServiceInfo.FOREGROUND_SERVICE_TYPE_DATA_SYNC,
                    )
                } else {
                    startForeground(NOTIFICATION_ID, notification)
                }
            }
        }
        return START_NOT_STICKY
    }

    private fun acquireBoundedWakeLock() {
        val power = getSystemService(PowerManager::class.java)
        if (wakeLock == null) {
            wakeLock = power.newWakeLock(
                PowerManager.PARTIAL_WAKE_LOCK,
                "Atomusicator:SeparationWakeLock",
            ).apply {
                setReferenceCounted(false)
            }
        }
        if (wakeLock?.isHeld != true) {
            // Strictly bounded to 30 minutes to guarantee no battery leak on hang
            wakeLock?.acquire(MAX_SEPARATION_TIMEOUT_MS)
        }
    }

    private fun releaseWakeLock() {
        if (wakeLock?.isHeld == true) {
            wakeLock?.release()
        }
    }

    private fun buildProgressNotification(progress: Int, statusMessage: String): Notification {
        val contentIntent = PendingIntent.getActivity(
            this,
            0,
            Intent(this, MainActivity::class.java).apply {
                flags = Intent.FLAG_ACTIVITY_SINGLE_TOP or Intent.FLAG_ACTIVITY_CLEAR_TOP
            },
            PendingIntent.FLAG_IMMUTABLE or PendingIntent.FLAG_UPDATE_CURRENT,
        )

        val cancelIntent = PendingIntent.getService(
            this,
            1,
            Intent(this, SeparationForegroundService::class.java).apply {
                action = ACTION_CANCEL
            },
            PendingIntent.FLAG_IMMUTABLE or PendingIntent.FLAG_UPDATE_CURRENT,
        )

        val title = if (progress in 1..99) {
            "Separando pistas ($progress%)"
        } else if (progress >= 100) {
            "Separación completada (100%)"
        } else {
            "Separando pistas de audio"
        }

        val builder = NotificationCompat.Builder(this, CHANNEL_ID)
            .setContentTitle(title)
            .setContentText(statusMessage)
            .setStyle(NotificationCompat.BigTextStyle().bigText(statusMessage))
            .setSmallIcon(android.R.drawable.stat_notify_sync)
            .setContentIntent(contentIntent)
            .setOngoing(true)
            .setOnlyAlertOnce(true)
            .setCategory(NotificationCompat.CATEGORY_PROGRESS)
            .setForegroundServiceBehavior(NotificationCompat.FOREGROUND_SERVICE_IMMEDIATE)
            .addAction(
                android.R.drawable.ic_menu_close_clear_cancel,
                "Cancelar",
                cancelIntent,
            )

        if (progress in 1..100) {
            builder.setProgress(100, progress, false)
            builder.setSubText("$progress%")
        } else {
            builder.setProgress(0, 0, true) // Indeterminate
        }

        return builder.build()
    }

    private fun ensureNotificationChannel() {
        val manager = getSystemService(NotificationManager::class.java)
        if (manager.getNotificationChannel(CHANNEL_ID) == null) {
            val channel = NotificationChannel(
                CHANNEL_ID,
                "Separación de pistas",
                NotificationManager.IMPORTANCE_LOW,
            ).apply {
                description = "Notificaciones de progreso para la separación de audio con IA"
                setShowBadge(false)
            }
            manager.createNotificationChannel(channel)
        }
    }

    override fun onDestroy() {
        releaseWakeLock()
        super.onDestroy()
    }

    companion object {
        const val CHANNEL_ID = "atomusicator.separation"
        const val NOTIFICATION_ID = 42
        const val MAX_SEPARATION_TIMEOUT_MS = 30 * 60 * 1000L // 30 minutes bounded ceiling

        const val ACTION_CANCEL = "com.matiusjbg.atomusicator.ACTION_CANCEL_SEPARATION"
        const val ACTION_UPDATE_PROGRESS = "com.matiusjbg.atomusicator.ACTION_UPDATE_SEPARATION_PROGRESS"
        const val ACTION_STOP = "com.matiusjbg.atomusicator.ACTION_STOP_SEPARATION"

        const val EXTRA_PROGRESS = "extra_progress"
        const val EXTRA_STATUS = "extra_status"
        const val EXTRA_TITLE = "extra_title"

        var onCancelSeparationCommand: (() -> Unit)? = null
    }
}
