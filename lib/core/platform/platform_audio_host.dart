import 'dart:io';
import 'package:flutter/services.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';

/// Abstract port for platform-specific audio host capabilities (DIP / Clean Architecture).
abstract interface class PlatformAudioHost {
  Future<bool> setPlaybackActive(bool active);
  Future<void> updatePlaybackState({required bool isPlaying, required String title});
  Future<void> startSeparationService({required String title});
  Future<void> updateSeparationProgress({required int progress, required String status});
  Future<void> stopSeparationService();
  Future<String?> takeSharedAudio();
  Future<void> shareFiles(List<String> paths);
  Future<String?> previewAudio(String path, {int buckets = 650});
  void listenForEvents({
    void Function(String event, double volumeScale)? onAudioFocusEvent,
    void Function(String action)? onTransportCommand,
    void Function()? onCancelSeparation,
  });
}

/// Concrete adapter for Android audio lifecycle and foreground services.
class AndroidPlatformAudioHost implements PlatformAudioHost {
  AndroidPlatformAudioHost({MethodChannel? channel})
      : _channel = channel ?? const MethodChannel('atomusicator/host') {
    _channel.setMethodCallHandler(_handleNativeCall);
  }

  final MethodChannel _channel;
  void Function(String event, double volumeScale)? _onAudioFocusEvent;
  void Function(String action)? _onTransportCommand;
  void Function()? _onCancelSeparation;

  Future<void> _handleNativeCall(MethodCall call) async {
    switch (call.method) {
      case 'onAudioFocusEvent':
        final args = call.arguments as Map<Object?, Object?>?;
        final event = args?['event'] as String? ?? 'transient_loss';
        final volume = (args?['volume'] as num?)?.toDouble() ?? 1.0;
        _onAudioFocusEvent?.call(event, volume);
      case 'audioFocusLost':
        _onAudioFocusEvent?.call('transient_loss', 0.0);
      case 'onTransportCommand':
        final args = call.arguments as Map<Object?, Object?>?;
        final action = args?['action'] as String?;
        if (action != null) {
          _onTransportCommand?.call(action);
        }
      case 'onCancelSeparation':
        _onCancelSeparation?.call();
    }
  }

  @override
  Future<bool> setPlaybackActive(bool active) async {
    final granted = await _channel.invokeMethod<bool>('playback', {'active': active});
    return granted ?? false;
  }

  @override
  Future<void> updatePlaybackState({
    required bool isPlaying,
    required String title,
  }) async {
    await _channel.invokeMethod<void>('updatePlaybackState', {
      'isPlaying': isPlaying,
      'title': title,
    });
  }

  @override
  Future<void> startSeparationService({required String title}) async {
    await _channel.invokeMethod<void>('startSeparationService', {'title': title});
  }

  @override
  Future<void> updateSeparationProgress({
    required int progress,
    required String status,
  }) async {
    await _channel.invokeMethod<void>('updateSeparationProgress', {
      'progress': progress,
      'status': status,
    });
  }

  @override
  Future<void> stopSeparationService() async {
    await _channel.invokeMethod<void>('stopSeparationService');
  }

  @override
  Future<String?> takeSharedAudio() async =>
      _channel.invokeMethod<String>('takeSharedAudio');

  @override
  Future<void> shareFiles(List<String> paths) async {
    if (paths.isEmpty) return;
    await _channel.invokeMethod<void>('shareFiles', {'paths': paths});
  }

  @override
  Future<String?> previewAudio(String path, {int buckets = 650}) async =>
      _channel.invokeMethod<String>('previewAudio', {
        'path': path,
        'buckets': buckets,
      });

  @override
  void listenForEvents({
    void Function(String event, double volumeScale)? onAudioFocusEvent,
    void Function(String action)? onTransportCommand,
    void Function()? onCancelSeparation,
  }) {
    _onAudioFocusEvent = onAudioFocusEvent;
    _onTransportCommand = onTransportCommand;
    _onCancelSeparation = onCancelSeparation;
  }
}

/// No-op adapter for desktop, test, and non-Android hosts.
class NoopPlatformAudioHost implements PlatformAudioHost {
  const NoopPlatformAudioHost();

  @override
  Future<bool> setPlaybackActive(bool active) async => true;

  @override
  Future<void> updatePlaybackState({
    required bool isPlaying,
    required String title,
  }) async {}

  @override
  Future<void> startSeparationService({required String title}) async {}

  @override
  Future<void> updateSeparationProgress({
    required int progress,
    required String status,
  }) async {}

  @override
  Future<void> stopSeparationService() async {}

  @override
  Future<String?> takeSharedAudio() async => null;

  @override
  Future<void> shareFiles(List<String> paths) async {}

  @override
  Future<String?> previewAudio(String path, {int buckets = 650}) async => null;

  @override
  void listenForEvents({
    void Function(String event, double volumeScale)? onAudioFocusEvent,
    void Function(String action)? onTransportCommand,
    void Function()? onCancelSeparation,
  }) {}
}

/// Riverpod provider delivering the appropriate PlatformAudioHost port implementation.
final platformAudioHostProvider = Provider<PlatformAudioHost>((ref) {
  if (Platform.isAndroid) {
    return AndroidPlatformAudioHost();
  }
  return const NoopPlatformAudioHost();
});
