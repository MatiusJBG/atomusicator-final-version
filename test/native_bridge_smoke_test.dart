import 'dart:convert';
import 'dart:io';
import 'package:flutter_rust_bridge/flutter_rust_bridge_for_generated.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:atomusicator/src/rust_bridge/api.dart' as native;
import 'package:atomusicator/src/rust_bridge/frb_generated.dart';

void main() {
  final library = Platform.environment['ATOMUSICATOR_NATIVE_LIBRARY'];
  test(
    'compiled bridge restores stems and runs pitch/export through generated bindings',
    () async {
      await RustLib.init(externalLibrary: ExternalLibrary.open(library!));
      final directory = Directory.systemTemp.createTempSync(
        'atomusicator-ffi-',
      );
      final outputs = Directory('${directory.path}/outputs')..createSync();
      try {
        final frames = 44100 * 4;
        final data = ByteData(44 + frames * 2 * 4);
        void ascii(int offset, String value) {
          for (var i = 0; i < value.length; i++) {
            data.setUint8(offset + i, value.codeUnitAt(i));
          }
        }

        ascii(0, 'RIFF');
        data.setUint32(4, data.lengthInBytes - 8, Endian.little);
        ascii(8, 'WAVE');
        ascii(12, 'fmt ');
        data.setUint32(16, 16, Endian.little);
        data.setUint16(20, 3, Endian.little);
        data.setUint16(22, 2, Endian.little);
        data.setUint32(24, 44100, Endian.little);
        data.setUint32(28, 44100 * 8, Endian.little);
        data.setUint16(32, 8, Endian.little);
        data.setUint16(34, 32, Endian.little);
        ascii(36, 'data');
        data.setUint32(40, frames * 8, Endian.little);
        for (var i = 0; i < frames * 2; i++) {
          data.setFloat32(
            44 + i * 4,
            (i % 40 < 20 ? 0.05 : -0.05),
            Endian.little,
          );
        }
        const stems = [
          'lead_vocal',
          'backing_vocals',
          'drums',
          'bass',
          'instrumental',
        ];
        for (final stem in stems) {
          File(
            '${outputs.path}/$stem.wav',
          ).writeAsBytesSync(data.buffer.asUint8List());
        }
        File('${directory.path}/session.json').writeAsStringSync(
          jsonEncode({
            'session_id': 'ffi-smoke',
            'file_name': 'synthetic.wav',
            'separation': {
              'stems': [
                for (final stem in stems)
                  {'kind': stem, 'path': 'outputs/$stem.wav'},
              ],
            },
          }),
        );
        final session = await native.openAudioSession(
          directory: directory.path,
        );
        expect(session.tracks.length, 5);
        expect(session.tracks.first.peaks, isNotEmpty);
        final track = session.tracks.first;
        expect(
          await native.waveformPeaks(
            sessionId: session.id,
            stemType: track.stemType,
            filePath: track.filePath,
            maxBuckets: 4000,
          ),
          isNotEmpty,
        );
        await native.setTrack(
          stemType: 'bass',
          gainDb: -6,
          pan: 0.3,
          mute: false,
          solo: true,
        );
        await native.setMaster(gain: 0.5);
        await native.seek(positionSeconds: 0.2);
        expect(
          (await native.playbackStatus()).positionSeconds,
          closeTo(0.2, 0.001),
        );
        Future<native.UiJobDto> finish(String id) async {
          final deadline = DateTime.now().add(const Duration(seconds: 90));
          while (DateTime.now().isBefore(deadline)) {
            final job = await native.jobStatus(jobId: id);
            if (!const {
              native.UiJobStatus.queued,
              native.UiJobStatus.running,
              native.UiJobStatus.cancelling,
              native.UiJobStatus.committing,
            }.contains(job.status)) {
              expect(
                job.status,
                native.UiJobStatus.completed,
                reason: job.message,
              );
              return job;
            }
            await Future<void>.delayed(const Duration(milliseconds: 50));
          }
          throw StateError('native job timed out');
        }

        final initialPitch = await native.livePitchStatus();
        final accepted = await native.setLivePitch(
          sessionId: session.id,
          epoch: initialPitch.epoch,
          revision: initialPitch.revision + BigInt.one,
          semitones: 3,
          preserveFormants: true,
        );
        expect(accepted.semitones, 3);
        expect((await native.livePitchStatus()).revision, accepted.revision);
        await native.setMaster(gain: 0);
        await native.play();
        var live = accepted;
        for (final semitones in [12, -12, 7, 0]) {
          final before = (await native.playbackStatus()).positionSeconds;
          final elapsed = Stopwatch()..start();
          live = await native.setLivePitch(
            sessionId: session.id,
            epoch: live.epoch,
            revision: live.revision + BigInt.one,
            semitones: semitones,
            preserveFormants: true,
          );
          await Future<void>.delayed(const Duration(milliseconds: 450));
          final playback = await native.playbackStatus();
          expect(playback.failure, isNull);
          expect(playback.playing, isTrue);
          expect(
            playback.positionSeconds - before,
            closeTo(elapsed.elapsedMicroseconds / 1e6, 0.10),
            reason: 'Live pitch must preserve the shared playback clock',
          );
          expect(
            (await native.livePitchStatus()).appliedRevision,
            live.revision,
          );
        }
        await native.pause();
        final paused = (await native.playbackStatus()).positionSeconds;
        await Future<void>.delayed(const Duration(milliseconds: 100));
        expect((await native.playbackStatus()).positionSeconds, paused);
        await native.seek(positionSeconds: 3.8);
        await native.play();
        await Future<void>.delayed(const Duration(milliseconds: 400));
        final eof = await native.playbackStatus();
        expect(eof.failure, isNull);
        expect(eof.playing, isFalse);
        expect(eof.positionSeconds, closeTo(4, 0.002));
        final exported = await finish(
          await native.startExportWithPitch(
            destination: directory.path,
            selectedStems: ['bass'],
            format: 'flac',
            sampleRate: 48000,
            channels: 1,
            bitDepth: 24,
            normalize: false,
            pitch: native.UiPitchExportDto(
              sessionId: session.id,
              epoch: accepted.epoch,
              semitones: 3,
              preserveFormants: true,
            ),
          ),
        );
        expect(
          File('${exported.outputDirectory}/bass.flac').existsSync(),
          isTrue,
        );
        final floatExport = await finish(
          await native.startExport(
            destination: directory.path,
            selectedStems: ['bass'],
            format: 'wav',
            sampleRate: 44100,
            channels: 2,
            bitDepth: 32,
            normalize: false,
          ),
        );
        expect(
          File('${floatExport.outputDirectory}/bass.wav').lengthSync(),
          greaterThan(1000),
        );
        final localSession = Platform.environment['ATOMUSICATOR_TEST_SESSION'];
        if (localSession != null) {
          final real = await native.openAudioSession(directory: localSession);
          final original = await native.livePitchStatus();
          final live = await native.setLivePitch(
            sessionId: real.id,
            epoch: original.epoch,
            revision: original.revision + BigInt.one,
            semitones: -7,
            preserveFormants: true,
          );
          await native.setMaster(gain: 0);
          await native.play();
          await Future<void>.delayed(const Duration(seconds: 2));
          final playback = await native.playbackStatus();
          expect(playback.failure, isNull);
          expect(playback.positionSeconds, closeTo(2, 0.10));
          await native.setLivePitch(
            sessionId: real.id,
            epoch: live.epoch,
            revision: live.revision + BigInt.one,
            semitones: original.semitones,
            preserveFormants: original.preserveFormants,
          );
          await native.stop();
        }
        await native.stop();
      } finally {
        RustLib.dispose();
        directory.deleteSync(recursive: true);
      }
    },
    skip:
        library == null
            ? 'Set ATOMUSICATOR_NATIVE_LIBRARY to the built bridge and ATOMUSICATOR_FFMPEG to the pinned runtime.'
            : false,
  );
}
