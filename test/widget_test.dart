import 'package:atomusicator/core/theme/app_theme.dart';
import 'package:atomusicator/src/features/workbench/audio_workbench.dart';
import 'package:flutter/material.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';
import 'package:flutter_test/flutter_test.dart';

void main() {
  testWidgets('workbench opens with an empty audio session', (tester) async {
    await tester.pumpWidget(
      ProviderScope(
        child: MaterialApp(theme: AppTheme.dark, home: const AudioWorkbench()),
      ),
    );

    expect(find.text('Your mix starts here'), findsOneWidget);
    expect(find.text('Choose audio files'), findsOneWidget);
    expect(find.text('Not separated'), findsNWidgets(5));
  });
}
