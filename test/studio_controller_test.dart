import 'support/fake_studio.dart';
import 'dart:async';
import 'package:atomusicator/core/di/native_engine_provider.dart';
import 'package:atomusicator/core/di/studio_controller.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:atomusicator/src/rust_bridge/api.dart';

void main() {
  late FakeStudio gateway;
  late ProviderContainer container;
  setUp(() {
    gateway = FakeStudio();
    container = ProviderContainer(
      overrides: [
        studioGatewayProvider.overrideWithValue(gateway),
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
  test(
    'export freezes the displayed pitch even before queued live updates finish',
    () async {
      final controller = container.read(studioProvider.notifier);
      await controller.open('/session');
      final pending = Completer<UiLivePitchDto>();
      gateway.pendingPitch = (_, _) => pending.future;
      final update = controller.applyPitch(7, false);
      final export = controller.export(
        '/exports',
        ['bass'],
        'wav',
        44100,
        2,
        24,
        false,
      );
      controller.applyPitch(-3, true);
      await export;
      expect(gateway.calls, contains('export-pitch:7:false'));
      gateway.pendingPitch = null;
      pending.complete(
        UiLivePitchDto(
          sessionId: 'session',
          epoch: BigInt.one,
          revision: BigInt.one,
          appliedRevision: BigInt.zero,
          semitones: 7,
          preserveFormants: false,
        ),
      );
      await update;
      expect(container.read(studioProvider).requestedPitch, -3);
    },
  );
  test(
    'live pitch coalesces rapid changes without owning a background job',
    () async {
      final controller = container.read(studioProvider.notifier);
      await controller.open('/session');
      final first = Completer<UiLivePitchDto>();
      var calls = 0;
      gateway.pendingPitch = (semitones, formants) {
        if (calls++ == 0) return first.future;
        return Future.value(
          UiLivePitchDto(
            sessionId: 'session',
            epoch: BigInt.one,
            revision: BigInt.two,
            appliedRevision: BigInt.one,
            semitones: semitones,
            preserveFormants: formants,
          ),
        );
      };
      final complete = controller.applyPitch(1, true);
      controller.applyPitch(2, true);
      controller.applyPitch(3, false);
      expect(container.read(studioProvider).requestedPitch, 3);
      expect(container.read(studioProvider).busy, isFalse);
      first.complete(
        UiLivePitchDto(
          sessionId: 'session',
          epoch: BigInt.one,
          revision: BigInt.one,
          appliedRevision: BigInt.zero,
          semitones: 1,
          preserveFormants: true,
        ),
      );
      await complete;
      expect(gateway.calls.where((v) => v.startsWith('live-pitch:')).toList(), [
        'live-pitch:1:true',
        'live-pitch:3:false',
      ]);
      expect(container.read(studioProvider).requestedPitch, 3);
      expect(container.read(studioProvider).livePitch?.semitones, 3);
      expect(container.read(studioProvider).job, isNull);
    },
  );
  test(
    'an old live pitch acknowledgment cannot change a reopened session',
    () async {
      final controller = container.read(studioProvider.notifier);
      await controller.open('/session');
      final stale = Completer<UiLivePitchDto>();
      gateway.pendingPitch = (_, _) => stale.future;
      final update = controller.applyPitch(7, true);
      await controller.open('/session');
      stale.complete(
        UiLivePitchDto(
          sessionId: 'session',
          epoch: BigInt.one,
          revision: BigInt.one,
          appliedRevision: BigInt.one,
          semitones: 7,
          preserveFormants: true,
        ),
      );
      await update;
      expect(container.read(studioProvider).requestedPitch, 0);
      expect(container.read(studioProvider).livePitch?.semitones, 0);
    },
  );
  test(
    'live pitch remains available during export and rolls back rejected settings',
    () async {
      final controller = container.read(studioProvider.notifier);
      await controller.open('/session');
      await controller.export('/exports', ['bass'], 'wav', 44100, 2, 24, false);
      expect(container.read(studioProvider).busy, isTrue);
      await controller.applyPitch(4, true);
      expect(container.read(studioProvider).requestedPitch, 4);
      expect(container.read(studioProvider).busy, isTrue);
      gateway.fail = true;
      await controller.applyPitch(30, false);
      expect(container.read(studioProvider).requestedPitch, 4);
      expect(container.read(studioProvider).error, isNotNull);
    },
  );
  test('late waveform detail cannot replace a reopened session', () async {
    final controller = container.read(studioProvider.notifier);
    await controller.open('/session');
    final stale = Completer<List<double>>();
    final current = Completer<List<double>>();
    var requests = 0;
    gateway.pendingWaveform =
        () => requests++ == 0 ? stale.future : current.future;
    const request = (
      sessionId: 'session',
      stemType: 'bass',
      filePath: '/bass.wav',
      buckets: 4000,
    );
    final subscription = container.listen(
      waveformDetailProvider(request),
      (_, _) {},
    );
    await Future<void>.delayed(Duration.zero);
    await controller.open('/session');
    final newest = container.read(waveformDetailProvider(request).future);
    stale.complete(const [-0.9, 0.9]);
    current.complete(const [-0.2, 0.2]);
    expect(await newest, const [-0.2, 0.2]);
    await Future<void>.delayed(Duration.zero);
    expect(container.read(waveformDetailProvider(request)).requireValue, const [
      -0.2,
      0.2,
    ]);
    subscription.close();
  });
  test('a late playback poll cannot undo a seek', () async {
    final controller = container.read(studioProvider.notifier);
    await controller.open('/session');
    gateway.pendingPlayback = Completer<UiPlaybackDto>();
    final poll = controller.refresh();
    await Future<void>.delayed(Duration.zero);
    await controller.seek(3);
    gateway.pendingPlayback!.complete(gateway.playbackValue);
    await poll;
    expect(container.read(studioProvider).position, 3);
  });
  test(
    'a late playback result cannot restart transport during a new job',
    () async {
      final controller = container.read(studioProvider.notifier);
      await controller.open('/session');
      gateway.pendingPlayback = Completer<UiPlaybackDto>();
      final poll = controller.refresh();
      await Future<void>.delayed(Duration.zero);
      await controller.separate('/song.wav');
      gateway.pendingPlayback!.complete(
        const UiPlaybackDto(
          playing: true,
          positionSeconds: 2,
          peak: 1,
          trackMeters: [],
        ),
      );
      await poll;
      expect(container.read(studioProvider).busy, isTrue);
      expect(container.read(studioProvider).playing, isFalse);
    },
  );
  test(
    'an independent control failure cannot release an active heavy job',
    () async {
      final controller = container.read(studioProvider.notifier);
      await controller.open('/session');
      await controller.separate('/song.wav');
      gateway.failMaster = true;
      await controller.setMaster(0.5);
      expect(container.read(studioProvider).busy, isTrue);
      await controller.separate('/overlap.wav');
      expect(
        gateway.calls.where((call) => call.startsWith('separate:')).length,
        1,
      );
    },
  );
  test(
    'native meters retain stem identity and device failure is actionable',
    () async {
      final controller = container.read(studioProvider.notifier);
      await controller.open('/session');
      gateway.playbackValue = const UiPlaybackDto(
        playing: false,
        positionSeconds: 2,
        peak: 0.5,
        trackMeters: [UiTrackMeterDto(stemType: 'bass', peak: 0.25)],
        failure: UiPlaybackFailure.deviceUnavailable,
      );
      await controller.refresh();
      final state = container.read(studioProvider);
      expect(state.trackPeaks['bass'], 0.25);
      expect(state.position, 2);
      expect(state.playing, false);
      expect(state.error, contains('Reconnect'));
      gateway.playbackValue = const UiPlaybackDto(
        playing: true,
        positionSeconds: 2,
        peak: 0.5,
        trackMeters: [],
        failure: null,
      );
      await controller.transport('play');
      expect(container.read(studioProvider).error, isNull);
    },
  );
  test(
    'separation publishes real session state and rejects overlapping UI jobs',
    () async {
      final controller = container.read(studioProvider.notifier);
      await controller.separate('/song.wav');
      await controller.separate('/other.wav');
      expect(gateway.calls.where((c) => c.startsWith('separate:')), [
        'separate:/song.wav:false',
      ]);
      await controller.refresh();
      expect(container.read(studioProvider).session?.name, 'song.wav');
      expect(container.read(studioProvider).busy, false);
    },
  );
  test(
    'failed separation preserves loaded session and exposes the codec error',
    () async {
      final controller = container.read(studioProvider.notifier);
      await controller.open('/session');
      gateway.fail = true;
      await controller.separate('/broken.mp3');
      expect(container.read(studioProvider).error, contains('bad codec'));
      expect(container.read(studioProvider).session?.id, 'session');
      expect(container.read(studioProvider).busy, false);
    },
  );
  test(
    'cancellation clears busy state and mixer controls reach the gateway',
    () async {
      final controller = container.read(studioProvider.notifier);
      await controller.open('/session');
      await controller.setTrack(
        'bass',
        const MixTrackState(mute: true, solo: true),
      );
      await controller.setMaster(0.5);
      await controller.seek(2);
      await controller.transport('play');
      await controller.separate('/song.wav');
      await controller.cancel();
      await controller.refresh();
      expect(container.read(studioProvider).job?.status, UiJobStatus.cancelled);
      expect(container.read(studioProvider).busy, false);
      expect(
        gateway.calls,
        containsAll(['track:bass:true:true', 'master:0.5', 'seek:2.0', 'play']),
      );
    },
  );
}
