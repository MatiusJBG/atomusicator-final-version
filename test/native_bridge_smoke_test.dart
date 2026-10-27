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
        final frames = 44100;
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

        final pitched = await finish(
          await native.startPitch(semitones: 3, preserveFormants: true),
        );
        expect(pitched.session?.pitchSemitones, 3);
        final exported = await finish(
          await native.startExport(
            destination: directory.path,
            selectedStems: ['bass'],
            format: 'flac',
            sampleRate: 48000,
            channels: 1,
            bitDepth: 24,
            normalize: false,
          ),
        );
        expect(
          File('${exported.outputDirectory}/bass.flac').existsSync(),
          isTrue,
        );
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
