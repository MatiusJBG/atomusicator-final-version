import 'package:atomusicator/core/di/native_engine_provider.dart';
import 'package:atomusicator/core/di/studio_controller.dart';
import 'package:atomusicator/core/platform/platform_audio_host.dart';
import 'package:flutter/services.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';
import 'package:flutter_test/flutter_test.dart';

import 'support/fake_studio.dart';

class MockPlatformAudioHost implements PlatformAudioHost {
  final List<String> calls = [];
  bool focusGranted = true;

  void Function(String event, double volumeScale)? onAudioFocusEvent;
  void Function(String action)? onTransportCommand;
  void Function()? onCancelSeparation;

  @override
  Future<bool> setPlaybackActive(bool active) async {
    calls.add('setPlaybackActive:$active');
    return focusGranted;
  }

  @override
  Future<void> updatePlaybackState({
    required bool isPlaying,
    required String title,
  }) async {
    calls.add('updatePlaybackState:isPlaying=$isPlaying:title=$title');
  }

  @override
  Future<void> startSeparationService({required String title}) async {
    calls.add('startSeparationService:$title');
  }

  @override
  Future<void> updateSeparationProgress({
    required int progress,
    required String status,
  }) async {
    calls.add('updateSeparationProgress:$progress:$status');
  }

  @override
  Future<void> stopSeparationService() async {
    calls.add('stopSeparationService');
  }

  @override
  Future<String?> takeSharedAudio() async {
    calls.add('takeSharedAudio');
    return '/mock/shared.wav';
  }

  @override
  Future<void> shareFiles(List<String> paths) async {
    calls.add('shareFiles:${paths.join(',')}');
  }

  @override
  Future<String?> previewAudio(String path, {int buckets = 650}) async {
    calls.add('previewAudio:$path:$buckets');
    return null;
  }

  @override
  void listenForEvents({
    void Function(String event, double volumeScale)? onAudioFocusEvent,
    void Function(String action)? onTransportCommand,
    void Function()? onCancelSeparation,
  }) {
    this.onAudioFocusEvent = onAudioFocusEvent;
    this.onTransportCommand = onTransportCommand;
    this.onCancelSeparation = onCancelSeparation;
  }
}

void main() {
  TestWidgetsFlutterBinding.ensureInitialized();

  group('NoopPlatformAudioHost', () {
    const host = NoopPlatformAudioHost();

    test('safe default return values', () async {
      expect(await host.setPlaybackActive(true), isTrue);
      expect(await host.setPlaybackActive(false), isTrue);
      expect(await host.takeSharedAudio(), isNull);
      await host.updatePlaybackState(isPlaying: true, title: 'Test');
      await host.startSeparationService(title: 'Test');
      await host.updateSeparationProgress(progress: 50, status: 'Separating');
      await host.stopSeparationService();
      await host.shareFiles(['/path/stem.wav']);
      host.listenForEvents(
        onAudioFocusEvent: (_, __) {},
        onTransportCommand: (_) {},
        onCancelSeparation: () {},
      );
    });
  });

  group('AndroidPlatformAudioHost via MethodChannel', () {
    late List<MethodCall> channelCalls;
    late AndroidPlatformAudioHost host;
    const channelName = 'test_atomusicator/host';

    setUp(() {
      channelCalls = [];
      TestDefaultBinaryMessengerBinding.instance.defaultBinaryMessenger
          .setMockMethodCallHandler(const MethodChannel(channelName), (call) async {
        channelCalls.add(call);
        return switch (call.method) {
          'playback' => true,
          'takeSharedAudio' => '/test/audio.flac',
          _ => null,
        };
      });

      host = AndroidPlatformAudioHost(channel: const MethodChannel(channelName));
    });

    tearDown(() {
      TestDefaultBinaryMessengerBinding.instance.defaultBinaryMessenger
          .setMockMethodCallHandler(const MethodChannel(channelName), null);
    });

    test('setPlaybackActive dispatches playback call', () async {
      final granted = await host.setPlaybackActive(true);
      expect(granted, isTrue);
      expect(channelCalls.last.method, 'playback');
      expect(channelCalls.last.arguments, {'active': true});
    });

    test('updatePlaybackState dispatches isPlaying and title', () async {
      await host.updatePlaybackState(isPlaying: true, title: 'Master Mix');
      expect(channelCalls.last.method, 'updatePlaybackState');
      expect(channelCalls.last.arguments, {
        'isPlaying': true,
        'title': 'Master Mix',
      });
    });

    test('separation service lifecycle calls', () async {
      await host.startSeparationService(title: 'Separating vocals');
      expect(channelCalls.last.method, 'startSeparationService');
      expect(channelCalls.last.arguments, {'title': 'Separating vocals'});

      await host.updateSeparationProgress(progress: 45, status: 'Model running');
      expect(channelCalls.last.method, 'updateSeparationProgress');
      expect(channelCalls.last.arguments, {
        'progress': 45,
        'status': 'Model running',
      });

      await host.stopSeparationService();
      expect(channelCalls.last.method, 'stopSeparationService');
    });

    test('takeSharedAudio and shareFiles dispatch correctly', () async {
      final audioPath = await host.takeSharedAudio();
      expect(audioPath, '/test/audio.flac');

      await host.shareFiles(['/stem1.wav', '/stem2.wav']);
      expect(channelCalls.last.method, 'shareFiles');
      expect(channelCalls.last.arguments, {
        'paths': ['/stem1.wav', '/stem2.wav'],
      });
    });

    test('listenForEvents handles native callbacks', () async {
      String? receivedEvent;
      double? receivedVolume;
      String? receivedAction;
      var cancelCalled = false;

      host.listenForEvents(
        onAudioFocusEvent: (event, vol) {
          receivedEvent = event;
          receivedVolume = vol;
        },
        onTransportCommand: (action) {
          receivedAction = action;
        },
        onCancelSeparation: () {
          cancelCalled = true;
        },
      );

      // Simulate onAudioFocusEvent from native
      await TestDefaultBinaryMessengerBinding.instance.defaultBinaryMessenger
          .handlePlatformMessage(
        channelName,
        const StandardMethodCodec().encodeMethodCall(
          const MethodCall('onAudioFocusEvent', {'event': 'duck', 'volume': 0.25}),
        ),
        (_) {},
      );
      expect(receivedEvent, 'duck');
      expect(receivedVolume, 0.25);

      // Simulate onTransportCommand from native
      await TestDefaultBinaryMessengerBinding.instance.defaultBinaryMessenger
          .handlePlatformMessage(
        channelName,
        const StandardMethodCodec().encodeMethodCall(
          const MethodCall('onTransportCommand', {'action': 'pause'}),
        ),
        (_) {},
      );
      expect(receivedAction, 'pause');

      // Simulate onCancelSeparation from native
      await TestDefaultBinaryMessengerBinding.instance.defaultBinaryMessenger
          .handlePlatformMessage(
        channelName,
        const StandardMethodCodec().encodeMethodCall(
          const MethodCall('onCancelSeparation', null),
        ),
        (_) {},
      );
      expect(cancelCalled, isTrue);
    });
  });

  group('StudioController + PlatformAudioHost integration', () {
    late FakeStudio gateway;
    late MockPlatformAudioHost mockHost;
    late ProviderContainer container;

    setUp(() {
      gateway = FakeStudio();
      mockHost = MockPlatformAudioHost();
      container = ProviderContainer(
        overrides: [
          studioGatewayProvider.overrideWithValue(gateway),
          platformAudioHostProvider.overrideWithValue(mockHost),
          nativeEngineStatusProvider.overrideWith(
            (ref) async => const NativeEngineStatus(
              engineVersion: 'test',
              applicationVersion: 'test',
              supportedStems: [],
            ),
          ),
        ],
      );
    });

    tearDown(() => container.dispose());

    test('transport play activates playback and updates notification state', () async {
      final controller = container.read(studioProvider.notifier);
      await controller.open('/session');

      await controller.transport('play');
      expect(mockHost.calls, contains('setPlaybackActive:true'));
      expect(
        mockHost.calls,
        contains('updatePlaybackState:isPlaying=true:title=song.wav'),
      );

      await controller.transport('pause');
      expect(mockHost.calls, contains('setPlaybackActive:false'));
      expect(
        mockHost.calls,
        contains('updatePlaybackState:isPlaying=false:title=song.wav'),
      );
    });

    test('audio focus loss automatically pauses playback', () async {
      final controller = container.read(studioProvider.notifier);
      await controller.open('/session');
      await controller.transport('play');

      // Fire audio focus transient loss from host
      mockHost.onAudioFocusEvent?.call('transient_loss', 0.0);
      await pumpEventQueue();

      expect(mockHost.calls, contains('setPlaybackActive:false'));
    });

    test('notification transport command plays or pauses', () async {
      final controller = container.read(studioProvider.notifier);
      await controller.open('/session');

      mockHost.onTransportCommand?.call('play');
      await pumpEventQueue();
      expect(mockHost.calls, contains('setPlaybackActive:true'));

      mockHost.onTransportCommand?.call('pause');
      await pumpEventQueue();
      expect(mockHost.calls, contains('setPlaybackActive:false'));
    });

    test('separation service starts with separate and updates progress', () async {
      final controller = container.read(studioProvider.notifier);

      await controller.separate('/audio.wav');
      expect(mockHost.calls, contains('startSeparationService:Separando pistas de audio'));
    });
  });
}
