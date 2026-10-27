import 'package:atomusicator/src/features/workbench/studio_panels.dart';
import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';

void main() {
  testWidgets(
    'track meter updates actual levels and clamps clipping visually',
    (tester) async {
      Future<void> show(double peak) => tester.pumpWidget(
        MaterialApp(
          home: Scaffold(body: TrackLevelMeter(stemType: 'bass', peak: peak)),
        ),
      );
      await show(0.25);
      expect(
        tester
            .widget<LinearProgressIndicator>(
              find.byType(LinearProgressIndicator),
            )
            .value,
        0.25,
      );
      await show(1.2);
      expect(
        tester
            .widget<LinearProgressIndicator>(
              find.byType(LinearProgressIndicator),
            )
            .value,
        1.0,
      );
      expect(find.byTooltip('Bass level: clipping'), findsOneWidget);
      await show(0);
      expect(
        tester
            .widget<LinearProgressIndicator>(
              find.byType(LinearProgressIndicator),
            )
            .value,
        0,
      );
    },
  );
}
