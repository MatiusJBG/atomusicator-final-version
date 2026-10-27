import 'dart:typed_data';
import 'dart:async';
import 'package:atomusicator/core/di/studio_controller.dart';
import 'package:atomusicator/src/rust_bridge/api.dart';

class FakeStudio implements StudioGateway {
  @override
  Future<List<UiRecoverableJobDto>> recoverableJobs() async => recoveryItems;
  List<UiRecoverableJobDto> recoveryItems = [];
  @override
  Future<String> resume(String id) async {
    calls.add('resume:$id');
    return 'retry';
  }

  @override
  Future<void> discard(String id) async {
    calls.add('discard:$id');
  }

  Completer<UiPlaybackDto>? pendingPlayback;
  bool failMaster = false;
  bool fail = false, cancelled = false;
  String failureMessage = "bad codec";
  final calls = <String>[];
  UiSessionDto get session => UiSessionDto(
    id: 'session',
    directory: '/session',
    name: 'song.wav',
    pitchSemitones: 0,
    tracks: [
      UiStemTrackDto(
        stemType: 'bass',
        filePath: '/bass.wav',
        sampleRate: 44100,
        channels: 2,
        durationSeconds: 4,
        peaks: Float32List(0),
      ),
    ],
  );
  @override
  Future<String> separate(String source, bool download) async {
    calls.add('separate:$source:$download');
    if (fail) throw StateError(failureMessage);
    return 'job';
  }

  @override
  Future<UiJobDto> job(String id) async => UiJobDto(
    id: id,
    status: cancelled ? UiJobStatus.cancelled : UiJobStatus.completed,
    progress: 1,
    message: 'done',
    session: cancelled ? null : session,
  );
  @override
  Future<void> cancel(String id) async {
    cancelled = true;
  }

  @override
  Future<UiSessionDto> open(String directory) async => session;
  @override
  Future<String> pitch(int semitones, bool formants) async {
    calls.add('pitch:$semitones:$formants');
    return 'pitch';
  }

  @override
  Future<String> export(
    String destination,
    List<String> stems,
    String format,
    int rate,
    int channels,
    int bits,
    bool normalize,
  ) async {
    calls.add(
      'export:${stems.join(',')}:$format:$rate:$channels:$bits:$normalize',
    );
    return 'export';
  }

  UiPlaybackDto playbackValue = const UiPlaybackDto(
    playing: false,
    positionSeconds: 0,
    peak: 0,
    trackMeters: [],
    failure: null,
  );
  @override
  Future<UiPlaybackDto> playback() async =>
      pendingPlayback == null ? playbackValue : pendingPlayback!.future;
  @override
  Future<void> play() async {
    calls.add('play');
  }

  @override
  Future<void> pause() async {
    calls.add('pause');
  }

  @override
  Future<void> stop() async {
    calls.add('stop');
  }

  @override
  Future<void> seek(double seconds) async {
    calls.add('seek:$seconds');
  }

  @override
  Future<void> master(double gain) async {
    if (failMaster) throw StateError('master failed');
    calls.add('master:$gain');
  }

  @override
  Future<void> track(String stem, MixTrackState state) async {
    calls.add('track:$stem:${state.mute}:${state.solo}');
  }
}
