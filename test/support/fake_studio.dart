import 'dart:typed_data';
import 'package:atomusicator/core/di/studio_controller.dart';
import 'package:atomusicator/src/rust_bridge/api.dart';

class FakeStudio implements StudioGateway {
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
    status: cancelled ? 'cancelled' : 'completed',
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

  @override
  Future<UiPlaybackDto> playback() async =>
      UiPlaybackDto(playing: false, positionSeconds: 0, peak: 0);
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
    calls.add('master:$gain');
  }

  @override
  Future<void> track(String stem, MixTrackState state) async {
    calls.add('track:$stem:${state.mute}:${state.solo}');
  }
}
