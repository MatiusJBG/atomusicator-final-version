import 'dart:async';
import 'package:flutter_riverpod/flutter_riverpod.dart';
import '../../src/rust_bridge/api.dart' as native;
import 'native_engine_provider.dart';

abstract class StudioGateway {
  Future<List<native.UiRecoverableJobDto>> recoverableJobs() async => [];
  Future<String> resume(String id) =>
      throw UnsupportedError('Recovery is unavailable');
  Future<void> discard(String id) =>
      throw UnsupportedError('Recovery is unavailable');
  Future<String> separate(String source, bool download);
  Future<native.UiJobDto> job(String id);
  Future<void> cancel(String id);
  Future<native.UiSessionDto> open(String directory);
  Future<String> pitch(int semitones, bool formants);
  Future<String> export(
    String destination,
    List<String> stems,
    String format,
    int rate,
    int channels,
    int bits,
    bool normalize,
  );
  Future<native.UiPlaybackDto> playback();
  Future<void> play();
  Future<void> pause();
  Future<void> stop();
  Future<void> seek(double seconds);
  Future<void> master(double gain);
  Future<void> track(String stem, MixTrackState state);
}

class NativeStudioGateway implements StudioGateway {
  @override
  Future<List<native.UiRecoverableJobDto>> recoverableJobs() =>
      native.recoverableJobs();
  @override
  Future<String> resume(String id) => native.resumeJob(jobId: id);
  @override
  Future<void> discard(String id) => native.discardJob(jobId: id);
  @override
  Future<String> separate(String source, bool download) =>
      native.startSeparation(sourcePath: source, allowModelDownload: download);
  @override
  Future<native.UiJobDto> job(String id) => native.jobStatus(jobId: id);
  @override
  Future<void> cancel(String id) => native.cancelJob(jobId: id);
  @override
  Future<native.UiSessionDto> open(String directory) =>
      native.openAudioSession(directory: directory);
  @override
  Future<String> pitch(int semitones, bool formants) =>
      native.startPitch(semitones: semitones, preserveFormants: formants);
  @override
  Future<String> export(
    String destination,
    List<String> stems,
    String format,
    int rate,
    int channels,
    int bits,
    bool normalize,
  ) => native.startExport(
    destination: destination,
    selectedStems: stems,
    format: format,
    sampleRate: rate,
    channels: channels,
    bitDepth: bits,
    normalize: normalize,
  );
  @override
  Future<native.UiPlaybackDto> playback() => native.playbackStatus();
  @override
  Future<void> play() => native.play();
  @override
  Future<void> pause() => native.pause();
  @override
  Future<void> stop() => native.stop();
  @override
  Future<void> seek(double seconds) => native.seek(positionSeconds: seconds);
  @override
  Future<void> master(double gain) => native.setMaster(gain: gain);
  @override
  Future<void> track(String stem, MixTrackState state) => native.setTrack(
    stemType: stem,
    gainDb: state.gainDb,
    pan: state.pan,
    mute: state.mute,
    solo: state.solo,
  );
}

class MixTrackState {
  const MixTrackState({
    this.gainDb = 0,
    this.pan = 0,
    this.mute = false,
    this.solo = false,
  });
  final double gainDb, pan;
  final bool mute, solo;
  MixTrackState copyWith({
    double? gainDb,
    double? pan,
    bool? mute,
    bool? solo,
  }) => MixTrackState(
    gainDb: gainDb ?? this.gainDb,
    pan: pan ?? this.pan,
    mute: mute ?? this.mute,
    solo: solo ?? this.solo,
  );
}

const _deviceUnavailableMessage =
    'Audio output device was disconnected. Reconnect it and press Play to retry.';

class StudioState {
  const StudioState({
    this.session,
    this.job,
    this.busy = false,
    this.error,
    this.playing = false,
    this.position = 0,
    this.peak = 0,
    this.trackPeaks = const {},
    this.master = 1,
    this.tracks = const {},
    this.exportDirectory,
  });
  final native.UiSessionDto? session;
  final native.UiJobDto? job;
  final bool busy, playing;
  final String? error, exportDirectory;
  final double position, peak, master;
  final Map<String, double> trackPeaks;
  final Map<String, MixTrackState> tracks;
  double get duration => session?.tracks.firstOrNull?.durationSeconds ?? 0;
  StudioState copyWith({
    native.UiSessionDto? session,
    native.UiJobDto? job,
    bool clearJob = false,
    bool? busy,
    String? error,
    bool clearError = false,
    bool? playing,
    double? position,
    double? peak,
    Map<String, double>? trackPeaks,
    double? master,
    Map<String, MixTrackState>? tracks,
    String? exportDirectory,
  }) => StudioState(
    session: session ?? this.session,
    job: clearJob ? null : job ?? this.job,
    busy: busy ?? this.busy,
    error: clearError ? null : error ?? this.error,
    playing: playing ?? this.playing,
    position: position ?? this.position,
    peak: peak ?? this.peak,
    trackPeaks: trackPeaks ?? this.trackPeaks,
    master: master ?? this.master,
    tracks: tracks ?? this.tracks,
    exportDirectory: exportDirectory ?? this.exportDirectory,
  );
}

final studioGatewayProvider = Provider<StudioGateway>(
  (ref) => NativeStudioGateway(),
);
final studioProvider = NotifierProvider<StudioController, StudioState>(
  StudioController.new,
);

bool jobIsActive(native.UiJobStatus? status) => switch (status) {
  native.UiJobStatus.queued ||
  native.UiJobStatus.running ||
  native.UiJobStatus.cancelling ||
  native.UiJobStatus.committing => true,
  _ => false,
};

final recoverableJobsProvider =
    FutureProvider<List<native.UiRecoverableJobDto>>((ref) async {
      if (ref.watch(studioProvider.select((s) => s.busy))) return [];
      await ref.watch(nativeEngineStatusProvider.future);
      return ref.read(studioGatewayProvider).recoverableJobs();
    });

class StudioController extends Notifier<StudioState> {
  bool _polling = false, _disposed = false;
  int _commands = 0;
  int _epoch = 0, _playbackRevision = 0;
  StudioGateway get _gateway => ref.read(studioGatewayProvider);
  @override
  StudioState build() {
    final timer = Timer.periodic(
      const Duration(milliseconds: 180),
      (_) => refresh(),
    );
    ref.onDispose(() {
      _disposed = true;
      timer.cancel();
    });
    return const StudioState();
  }

  Future<void> _guard(
    Future<void> Function() action, {
    bool ownsBusy = false,
  }) async {
    final epoch = _epoch;
    try {
      await ref.read(nativeEngineStatusProvider.future);
      if (_disposed || epoch != _epoch) return;
      await action();
    } catch (error) {
      if (!_disposed && epoch == _epoch) {
        state = state.copyWith(
          error: error.toString(),
          busy: ownsBusy ? false : state.busy,
        );
      }
    }
  }

  Future<void> _start(Future<String> Function() action) async {
    if (state.busy) return;
    final epoch = ++_epoch;
    _playbackRevision++;
    state = state.copyWith(
      busy: true,
      clearJob: true,
      clearError: true,
      playing: false,
    );
    await _guard(() async {
      final id = await action();
      if (!_disposed && epoch == _epoch) {
        state = state.copyWith(
          job: native.UiJobDto(
            id: id,
            status: native.UiJobStatus.running,
            progress: 0,
            message: 'Starting…',
          ),
        );
      }
    }, ownsBusy: true);
  }

  Future<void> separate(String source, {bool allowDownload = false}) =>
      _start(() => _gateway.separate(source, allowDownload));
  Future<void> applyPitch(int semitones, bool formants) =>
      _start(() => _gateway.pitch(semitones, formants));
  Future<void> resumeSavedJob(String id) => _start(() => _gateway.resume(id));
  Future<void> discardSavedJob(String id) => _guard(() async {
    await _gateway.discard(id);
    ref.invalidate(recoverableJobsProvider);
  });
  Future<void> export(
    String destination,
    List<String> stems,
    String format,
    int rate,
    int channels,
    int bits,
    bool normalize,
  ) => _start(
    () => _gateway.export(
      destination,
      stems,
      format,
      rate,
      channels,
      bits,
      normalize,
    ),
  );
  Future<void> cancel() => _guard(() async {
    final id = state.job?.id;
    if (id != null) {
      await _gateway.cancel(id);
      _epoch++;
    }
  });
  Future<void> open(String directory) async {
    if (state.busy) return;
    final epoch = ++_epoch;
    _playbackRevision++;
    state = state.copyWith(busy: true, clearJob: true, clearError: true);
    await _guard(() async {
      final session = await _gateway.open(directory);
      if (_disposed || epoch != _epoch) return;
      await _accept(session, epoch);
      if (!_disposed && epoch == _epoch) state = state.copyWith(busy: false);
    }, ownsBusy: true);
  }

  Future<void> _accept(native.UiSessionDto session, int epoch) async {
    final same = state.session?.id == session.id;
    final tracks = {
      for (final track in session.tracks)
        track.stemType:
            same
                ? state.tracks[track.stemType] ?? const MixTrackState()
                : const MixTrackState(),
    };
    for (final entry in tracks.entries) {
      if (_disposed || epoch != _epoch) return;
      await _gateway.track(entry.key, entry.value);
    }
    if (!_disposed && epoch == _epoch) {
      state = state.copyWith(
        session: session,
        tracks: Map.unmodifiable(tracks),
        playing: false,
        peak: 0,
        trackPeaks: const {},
        position: same ? state.position : 0,
      );
    }
  }

  Future<void> refresh() async {
    if (_disposed ||
        _polling ||
        _commands > 0 ||
        (state.session == null && state.job == null)) {
      return;
    }
    _polling = true;
    final epoch = _epoch;
    final revision = _playbackRevision;
    try {
      if (state.busy && jobIsActive(state.job?.status)) {
        final jobId = state.job!.id;
        final job = await _gateway.job(jobId);
        if (_disposed || epoch != _epoch || state.job?.id != jobId) return;
        if (job.status == native.UiJobStatus.completed && job.session != null) {
          await _accept(job.session!, epoch);
        }
        if (_disposed || epoch != _epoch) return;
        state = state.copyWith(
          job: job,
          busy: jobIsActive(job.status),
          error: job.status == native.UiJobStatus.failed ? job.message : null,
          exportDirectory: job.outputDirectory,
        );
      }
      if (state.session != null && !state.busy) {
        final status = await _gateway.playback();
        if (!_disposed &&
            epoch == _epoch &&
            revision == _playbackRevision &&
            !state.busy) {
          state = state.copyWith(
            playing: status.playing,
            position: status.positionSeconds,
            peak: status.peak,
            trackPeaks: Map.unmodifiable({
              for (final meter in status.trackMeters)
                meter.stemType: meter.peak,
            }),
            error:
                status.failure == native.UiPlaybackFailure.deviceUnavailable
                    ? _deviceUnavailableMessage
                    : null,
            clearError:
                status.failure == null &&
                state.error == _deviceUnavailableMessage,
          );
        }
      }
    } catch (error) {
      if (!_disposed && epoch == _epoch) {
        state = state.copyWith(error: error.toString());
      }
    } finally {
      _polling = false;
    }
  }

  Future<void> transport(String action) => _guard(() async {
    if (state.busy || state.session == null) return;
    _playbackRevision++;
    _commands++;
    try {
      if (action == 'play') {
        if (state.position >= state.duration) await _gateway.seek(0);
        await _gateway.play();
      }
      if (action == 'pause') await _gateway.pause();
      if (action == 'stop') await _gateway.stop();
    } finally {
      _commands--;
    }
    await refresh();
  });
  Future<void> seek(double seconds) => _guard(() async {
    if (!state.busy) {
      final epoch = _epoch;
      final revision = ++_playbackRevision;
      await _gateway.seek(seconds);
      if (!_disposed && epoch == _epoch && revision == _playbackRevision) {
        state = state.copyWith(position: seconds.clamp(0, state.duration));
      }
    }
  });
  Future<void> setMaster(double gain) => _guard(() async {
    await _gateway.master(gain);
    if (!_disposed) state = state.copyWith(master: gain);
  });
  Future<void> setTrack(String stem, MixTrackState track) => _guard(() async {
    await _gateway.track(stem, track);
    if (!_disposed) {
      state = state.copyWith(
        tracks: Map.unmodifiable({...state.tracks, stem: track}),
      );
    }
  });
}
