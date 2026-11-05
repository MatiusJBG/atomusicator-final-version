import 'dart:async';
import 'package:flutter_riverpod/flutter_riverpod.dart';
import '../../src/rust_bridge/api.dart' as native;
import 'native_engine_provider.dart';

abstract class StudioGateway {
  Future<List<double>> waveform(
    String sessionId,
    String stemType,
    String filePath,
    int buckets,
  );
  Future<List<native.UiRecoverableJobDto>> recoverableJobs() async => [];
  Future<String> resume(String id) =>
      throw UnsupportedError('Recovery is unavailable');
  Future<void> discard(String id) =>
      throw UnsupportedError('Recovery is unavailable');
  Future<String> separate(String source, bool download);
  Future<native.UiJobDto> job(String id);
  Stream<native.UiJobDto> jobChanges(native.UiJobDto previous);
  Future<void> cancel(String id);
  Future<native.UiSessionDto> open(String directory);
  Future<void> close();
  Future<String> pitch(int semitones);
  Future<native.UiLivePitchDto> livePitchStatus();
  Future<native.UiLivePitchDto> livePitch(
    String sessionId,
    BigInt epoch,
    BigInt revision,
    int semitones,
  );
  Future<String> export(
    String destination,
    List<String> stems,
    String format,
    int rate,
    int channels,
    int bits,
    bool normalize, {
    native.UiPitchExportDto? pitch,
  });
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
  Future<void> close() => native.closeAudioSession();
  @override
  Stream<native.UiJobDto> jobChanges(native.UiJobDto previous) async* {
    var current = previous;
    while (jobIsActive(current.status)) {
      final next = await native.waitJobChange(previous: current);
      if (next.status != current.status ||
          next.progress != current.progress ||
          next.message != current.message) {
        yield next;
      }
      current = next;
    }
  }

  @override
  Future<native.UiLivePitchDto> livePitchStatus() => native.livePitchStatus();
  @override
  Future<native.UiLivePitchDto> livePitch(
    String sessionId,
    BigInt epoch,
    BigInt revision,
    int semitones,
  ) => native.setLivePitch(
    sessionId: sessionId,
    epoch: epoch,
    revision: revision,
    semitones: semitones,
  );
  @override
  Future<List<double>> waveform(
    String sessionId,
    String stemType,
    String filePath,
    int buckets,
  ) => native.waveformPeaks(
    sessionId: sessionId,
    stemType: stemType,
    filePath: filePath,
    maxBuckets: buckets,
  );
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
  Future<String> pitch(int semitones) =>
      native.startPitch(semitones: semitones);
  @override
  Future<String> export(
    String destination,
    List<String> stems,
    String format,
    int rate,
    int channels,
    int bits,
    bool normalize, {
    native.UiPitchExportDto? pitch,
  }) => native.startExportWithPitch(
    destination: destination,
    selectedStems: stems,
    format: format,
    sampleRate: rate,
    channels: channels,
    bitDepth: bits,
    normalize: normalize,
    pitch: pitch,
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
const _stemUnreadableMessage =
    'A stem could not be read. Reopen the session to verify its files before retrying playback.';

class StudioState {
  const StudioState({
    this.session,
    this.job,
    this.busy = false,
    this.cancelling = false,
    this.error,
    this.playing = false,
    this.position = 0,
    this.peak = 0,
    this.trackPeaks = const {},
    this.master = 1,
    this.tracks = const {},
    this.exportDirectory,
    this.livePitch,
    this.requestedPitch = 0,
  });
  final native.UiSessionDto? session;
  final native.UiLivePitchDto? livePitch;
  final int requestedPitch;
  final native.UiJobDto? job;
  final bool busy, playing, cancelling;
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
    bool? cancelling,
    String? error,
    bool clearError = false,
    bool? playing,
    double? position,
    double? peak,
    Map<String, double>? trackPeaks,
    double? master,
    Map<String, MixTrackState>? tracks,
    String? exportDirectory,
    native.UiLivePitchDto? livePitch,
    int? requestedPitch,
  }) => StudioState(
    session: session ?? this.session,
    job: clearJob ? null : job ?? this.job,
    busy: busy ?? this.busy,
    cancelling: (busy ?? this.busy) && (cancelling ?? this.cancelling),
    error: clearError ? null : error ?? this.error,
    playing: playing ?? this.playing,
    position: position ?? this.position,
    peak: peak ?? this.peak,
    trackPeaks: trackPeaks ?? this.trackPeaks,
    master: master ?? this.master,
    tracks: tracks ?? this.tracks,
    exportDirectory: exportDirectory ?? this.exportDirectory,
    livePitch: livePitch ?? this.livePitch,
    requestedPitch: requestedPitch ?? this.requestedPitch,
  );
}

final studioGatewayProvider = Provider<StudioGateway>(
  (ref) => NativeStudioGateway(),
);
final studioProvider = NotifierProvider<StudioController, StudioState>(
  StudioController.new,
);

typedef WaveformRequest =
    ({String sessionId, String stemType, String filePath, int buckets});
final waveformDetailProvider = FutureProvider.autoDispose.family<
  List<double>,
  WaveformRequest
>((ref, request) {
  // Reopening/pitching the same session also invalidates outstanding detail reads.
  ref.watch(studioProvider.select((state) => state.session));
  return ref
      .watch(studioGatewayProvider)
      .waveform(
        request.sessionId,
        request.stemType,
        request.filePath,
        request.buckets,
      );
});

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
  int _pitchGeneration = 0;
  ({int semitones, int generation})? _pendingPitch;
  Future<void>? _pitchFlight;
  bool _polling = false, _disposed = false;
  int _commands = 0;
  int _epoch = 0, _playbackRevision = 0;
  StreamSubscription<native.UiJobDto>? _jobEvents;
  Timer? _playbackTimer;
  StudioGateway get _gateway => ref.read(studioGatewayProvider);
  @override
  StudioState build() {
    listenSelf((previous, next) {
      if (next.playing && !next.busy) {
        _playbackTimer ??= Timer.periodic(
          const Duration(milliseconds: 180),
          (_) => refresh(),
        );
      } else {
        _playbackTimer?.cancel();
        _playbackTimer = null;
      }
    });
    ref.onDispose(() {
      _disposed = true;
      _playbackTimer?.cancel();
      _jobEvents?.cancel();
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
        await _jobEvents?.cancel();
        if (!_disposed && epoch == _epoch) {
          _jobEvents = _gateway
              .jobChanges(state.job!)
              .listen(
                (job) => _handleJobEvent(job, epoch),
                onError: (Object error) {
                  if (!_disposed && epoch == _epoch) {
                    state = state.copyWith(error: error.toString());
                  }
                },
              );
        }
      }
    }, ownsBusy: true);
  }

  Future<void> separate(String source, {bool allowDownload = false}) =>
      _start(() => _gateway.separate(source, allowDownload));

  Future<void> _handleJobEvent(native.UiJobDto job, int epoch) async {
    if (_disposed || epoch != _epoch || state.job?.id != job.id) return;
    try {
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
    } catch (error) {
      if (!_disposed && epoch == _epoch) {
        state = state.copyWith(busy: false, error: error.toString());
      }
    }
  }

  Future<void> applyPitch(int semitones) {
    if (state.session == null || state.livePitch == null || _disposed) {
      return Future.value();
    }
    state = state.copyWith(requestedPitch: semitones, clearError: true);
    _pendingPitch = (semitones: semitones, generation: _pitchGeneration);
    return _pitchFlight ??= _drainPitch().whenComplete(
      () => _pitchFlight = null,
    );
  }

  Future<void> _drainPitch() async {
    while (!_disposed && _pendingPitch != null) {
      final target = _pendingPitch!;
      _pendingPitch = null;
      final previous = state.livePitch;
      final session = state.session;
      if (previous == null ||
          session == null ||
          target.generation != _pitchGeneration) {
        continue;
      }
      try {
        final accepted = await _gateway.livePitch(
          session.id,
          previous.epoch,
          previous.revision + BigInt.one,
          target.semitones,
        );
        if (!_disposed &&
            target.generation == _pitchGeneration &&
            accepted.sessionId == state.session?.id) {
          state = state.copyWith(livePitch: accepted);
        }
      } catch (error) {
        if (!_disposed && target.generation == _pitchGeneration) {
          state = state.copyWith(
            error: error.toString(),
            requestedPitch: _pendingPitch == null ? previous.semitones : null,
          );
        }
      }
    }
  }

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
  ) {
    final pitch =
        state.livePitch == null || state.session == null
            ? null
            : native.UiPitchExportDto(
              sessionId: state.session!.id,
              epoch: state.livePitch!.epoch,
              semitones: state.requestedPitch,
            );
    return _start(
      () => _gateway.export(
        destination,
        stems,
        format,
        rate,
        channels,
        bits,
        normalize,
        pitch: pitch,
      ),
    );
  }

  Future<void> cancel() async {
    if (state.cancelling ||
        !state.busy ||
        state.job?.status != native.UiJobStatus.running) {
      return;
    }
    final id = state.job?.id;
    if (id == null) return;
    state = state.copyWith(cancelling: true);
    await _guard(() async {
      try {
        await _gateway.cancel(id);
      } catch (_) {
        if (!_disposed && state.job?.id == id) {
          state = state.copyWith(cancelling: false);
        }
        rethrow;
      }
    });
  }

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

  Future<void> close() async {
    if (state.busy) return;
    final epoch = ++_epoch;
    _playbackRevision++;
    state = state.copyWith(busy: true);
    await _guard(() async {
      await _gateway.close();
      if (_disposed || epoch != _epoch) return;
      _pitchGeneration++;
      _pendingPitch = null;
      await _jobEvents?.cancel();
      if (!_disposed && epoch == _epoch) state = const StudioState();
    }, ownsBusy: true);
  }

  Future<void> _accept(native.UiSessionDto session, int epoch) async {
    _pitchGeneration++;
    _pendingPitch = null;
    final livePitch = await _gateway.livePitchStatus();
    if (_disposed || epoch != _epoch || livePitch.sessionId != session.id) {
      return;
    }
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
        clearError: true,
        livePitch: livePitch,
        requestedPitch: livePitch.semitones,
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
            error: switch (status.failure) {
              native.UiPlaybackFailure.deviceUnavailable =>
                _deviceUnavailableMessage,
              native.UiPlaybackFailure.stemUnreadable => _stemUnreadableMessage,
              null => null,
            },
            clearError:
                status.failure == null &&
                (state.error == _deviceUnavailableMessage ||
                    state.error == _stemUnreadableMessage),
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
