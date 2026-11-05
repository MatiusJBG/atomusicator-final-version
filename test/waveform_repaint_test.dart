import 'package:atomusicator/src/features/workbench/studio_panels.dart';
import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';

void main() {
  test(
    'static waveform ignores playback position and cursor repaints separately',
    () {
      final peaks = [-.4, .6, -.2, .3];
      final old = WaveformPainter(
        peaks: peaks,
        color: Colors.blue,
        progress: 0,
        showPlayhead: false,
      );
      final next = WaveformPainter(
        peaks: peaks,
        color: Colors.blue,
        progress: .5,
        showPlayhead: false,
      );
      expect(next.shouldRepaint(old), isFalse);
      expect(PlayheadPainter(.5).shouldRepaint(PlayheadPainter(0)), isTrue);
      expect(
        WaveformPainter(
          peaks: List.of(peaks),
          color: Colors.blue,
          progress: 0,
          showPlayhead: false,
        ).shouldRepaint(old),
        isTrue,
      );
      expect(
        WaveformPainter(
          peaks: peaks,
          color: Colors.red,
          progress: 0,
          showPlayhead: false,
        ).shouldRepaint(old),
        isTrue,
      );
      expect(
        WaveformPainter(
          peaks: peaks,
          color: Colors.blue,
          showGrid: false,
          progress: 0,
          showPlayhead: false,
        ).shouldRepaint(old),
        isTrue,
      );
    },
  );
}
