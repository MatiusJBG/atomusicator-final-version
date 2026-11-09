import 'dart:typed_data';
import 'package:atomusicator/core/di/native_engine_provider.dart';
import 'package:atomusicator/core/di/studio_controller.dart';
import 'package:atomusicator/core/di/desktop_studio_state.dart';
import 'package:atomusicator/core/theme/app_theme.dart';
import 'package:atomusicator/src/features/workbench/audio_workbench.dart';
import 'package:atomusicator/src/features/workbench/studio_panels.dart';
import 'package:atomusicator/src/rust_bridge/api.dart';
import 'package:flutter/material.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';
import 'package:flutter_test/flutter_test.dart';
import 'support/fake_studio.dart';

class TestMobileStudio extends FakeStudio {
  @override
  UiSessionDto get session => UiSessionDto(
        id: 'mobile-session',
        directory: '/session',
        name: 'Mobile Test Song',
        pitchSemitones: 0,
        tracks: [
          for (final kind in stemNames.keys)
            UiStemTrackDto(
              stemType: kind,
              filePath: '/$kind.wav',
              sampleRate: 44100,
              channels: 2,
              durationSeconds: 180,
              peaks: Float32List.fromList(
                List.generate(400, (i) => i.isEven ? -0.4 : 0.6),
              ),
            ),
        ],
      );
}

Future<({ProviderContainer container, TestMobileStudio gateway})> _mount(
  WidgetTester tester, {
  TestMobileStudio? gateway,
  Size size = const Size(360, 640),
}) async {
  tester.view.devicePixelRatio = 1;
  tester.view.physicalSize = size;
  addTearDown(tester.view.resetPhysicalSize);
  addTearDown(tester.view.resetDevicePixelRatio);

  final fakeGateway = gateway ?? TestMobileStudio();
  final container = ProviderContainer(
    overrides: [
      studioGatewayProvider.overrideWithValue(fakeGateway),
      nativeEngineStatusProvider.overrideWith(
        (ref) async => const NativeEngineStatus(
          engineVersion: 'test',
          applicationVersion: 'test',
          supportedStems: [],
        ),
      ),
    ],
  );

  await container.read(studioProvider.notifier).open('/session');
  container.read(studioViewProvider.notifier).state = StudioView.mixer;

  await tester.pumpWidget(
    UncontrolledProviderScope(
      container: container,
      child: MaterialApp(
        theme: AppTheme.dark,
        home: const AudioWorkbench(),
      ),
    ),
  );
  await tester.pumpAndSettle();

  addTearDown(() async {
    await tester.pumpWidget(const SizedBox.shrink());
    container.dispose();
  });

  return (container: container, gateway: fakeGateway);
}

void main() {
  testWidgets(
    'Mixer and transport layout renders without overflow on 360x640 dp',
    (tester) async {
      await _mount(tester, size: const Size(360, 640));

      expect(tester.takeException(), isNull);
      expect(find.byType(MobileStudioTracks), findsOneWidget);
      expect(find.byType(StudioTransport), findsOneWidget);
      expect(find.byKey(const ValueKey('transport-scrub-slider')), findsOneWidget);
      expect(find.text('MEZCLADOR'), findsOneWidget);
      expect(find.byKey(const ValueKey('transport-play-pause')), findsOneWidget);
    },
  );

  testWidgets(
    'Mobile transport and mixer renders with zero overflow on 390x844 dp',
    (tester) async {
      await _mount(tester, size: const Size(390, 844));

      expect(tester.takeException(), isNull);
      expect(find.byType(MobileStudioTracks), findsOneWidget);
      expect(find.byType(StudioTransport), findsOneWidget);
      expect(find.byKey(const ValueKey('transport-scrub-slider')), findsOneWidget);
      expect(find.byKey(const ValueKey('transport-play-pause')), findsOneWidget);
    },
  );

  testWidgets(
    'Mobile transport and mixer renders with zero overflow on 412x915 dp',
    (tester) async {
      await _mount(tester, size: const Size(412, 915));

      expect(tester.takeException(), isNull);
      expect(find.byType(MobileStudioTracks), findsOneWidget);
      expect(find.byType(StudioTransport), findsOneWidget);
      expect(find.byKey(const ValueKey('transport-scrub-slider')), findsOneWidget);
      expect(find.byKey(const ValueKey('transport-play-pause')), findsOneWidget);
    },
  );

  testWidgets(
    'All interactive track and transport controls satisfy touch target >= 48dp',
    (tester) async {
      await _mount(tester, size: const Size(360, 640));

      // Track Mute and Solo buttons in header
      final muteSize = tester.getSize(find.byKey(const ValueKey('mute-lead_vocal')));
      expect(muteSize.width, greaterThanOrEqualTo(48.0));
      expect(muteSize.height, greaterThanOrEqualTo(48.0));

      final soloSize = tester.getSize(find.byKey(const ValueKey('solo-lead_vocal')));
      expect(soloSize.width, greaterThanOrEqualTo(48.0));
      expect(soloSize.height, greaterThanOrEqualTo(48.0));

      // Zoom controls in header
      final zoomOutSize = tester.getSize(find.byKey(const ValueKey('zoom-out')));
      expect(zoomOutSize.width, greaterThanOrEqualTo(48.0));
      expect(zoomOutSize.height, greaterThanOrEqualTo(48.0));

      final zoomInSize = tester.getSize(find.byKey(const ValueKey('zoom-in')));
      expect(zoomInSize.width, greaterThanOrEqualTo(48.0));
      expect(zoomInSize.height, greaterThanOrEqualTo(48.0));

      // Transport buttons
      final seekSize = tester.getSize(find.byKey(const ValueKey('transport-seek-0')));
      expect(seekSize.width, greaterThanOrEqualTo(48.0));
      expect(seekSize.height, greaterThanOrEqualTo(48.0));

      final playSize = tester.getSize(find.byKey(const ValueKey('transport-play-pause')));
      expect(playSize.width, greaterThanOrEqualTo(54.0));
      expect(playSize.height, greaterThanOrEqualTo(54.0));

      final stopSize = tester.getSize(find.byKey(const ValueKey('transport-stop')));
      expect(stopSize.width, greaterThanOrEqualTo(48.0));
      expect(stopSize.height, greaterThanOrEqualTo(48.0));

      final pitchSize = tester.getSize(find.byKey(const ValueKey('transport-pitch')));
      expect(pitchSize.width, greaterThanOrEqualTo(48.0));
      expect(pitchSize.height, greaterThanOrEqualTo(48.0));

      final masterSize = tester.getSize(find.byKey(const ValueKey('transport-master')));
      expect(masterSize.width, greaterThanOrEqualTo(48.0));
      expect(masterSize.height, greaterThanOrEqualTo(48.0));

      final exportSize = tester.getSize(find.byKey(const ValueKey('transport-export')));
      expect(exportSize.width, greaterThanOrEqualTo(48.0));
      expect(exportSize.height, greaterThanOrEqualTo(48.0));

      // Volume and Pan sliders inside expanded track
      final volSliderSize = tester.getSize(find.byKey(const ValueKey('volume-slider-lead_vocal')));
      expect(volSliderSize.height, greaterThanOrEqualTo(48.0));

      final panSliderSize = tester.getSize(find.byKey(const ValueKey('pan-slider-lead_vocal')));
      expect(panSliderSize.height, greaterThanOrEqualTo(48.0));
    },
  );

  testWidgets(
    'MobileStudioTracks expands and collapses stem channels',
    (tester) async {
      await _mount(tester, size: const Size(360, 640));

      // lead_vocal is initially expanded
      expect(find.byKey(const ValueKey('volume-slider-lead_vocal')), findsOneWidget);
      expect(find.byKey(const ValueKey('pan-slider-lead_vocal')), findsOneWidget);

      // backing_vocals is initially collapsed
      expect(find.byKey(const ValueKey('volume-slider-backing_vocals')), findsNothing);

      // Scroll to and tap backing_vocals header to expand it
      final backingHeader = find.byKey(const ValueKey('header-backing_vocals'));
      await tester.ensureVisible(backingHeader);
      await tester.pumpAndSettle();
      await tester.tap(backingHeader);
      await tester.pumpAndSettle();

      expect(find.byKey(const ValueKey('volume-slider-backing_vocals')), findsOneWidget);
      expect(find.byKey(const ValueKey('pan-slider-backing_vocals')), findsOneWidget);

      // Scroll to and tap lead_vocal header to collapse it
      final leadHeader = find.byKey(const ValueKey('header-lead_vocal'));
      await tester.ensureVisible(leadHeader);
      await tester.pumpAndSettle();
      await tester.tap(leadHeader);
      await tester.pumpAndSettle();

      expect(find.byKey(const ValueKey('volume-slider-lead_vocal')), findsNothing);

      // All 5 TrackLevelMeters in headers remain in widget tree
      expect(find.byType(TrackLevelMeter), findsNWidgets(5));
    },
  );

  testWidgets(
    'Two-finger pinch scale gesture updates timeline zoom factor',
    (tester) async {
      final mounted = await _mount(tester, size: const Size(360, 640));

      expect(mounted.container.read(timelineZoomProvider), 1.0);

      final waveFinder = find.byKey(const ValueKey('wave-lead_vocal'));
      expect(waveFinder, findsOneWidget);
      final center = tester.getCenter(waveFinder);

      // Start 2-pointer pinch gesture
      final pointer1 = await tester.startGesture(center - const Offset(20, 0), pointer: 1);
      final pointer2 = await tester.startGesture(center + const Offset(20, 0), pointer: 2);
      await tester.pump();

      // Spread fingers apart horizontally
      await pointer1.moveBy(const Offset(-40, 0));
      await pointer2.moveBy(const Offset(40, 0));
      await tester.pump();

      expect(mounted.container.read(timelineZoomProvider), greaterThan(1.0));

      await pointer1.up();
      await pointer2.up();
      await tester.pump();
    },
  );

  testWidgets(
    'One-finger horizontal pan smoothly shifts waveform scroll offset',
    (tester) async {
      final mounted = await _mount(tester, size: const Size(360, 640));

      // Zoom in to 4.0 so the waveform is 4x wider than viewport and scrollable
      mounted.container.read(timelineZoomProvider.notifier).state = 4.0;
      await tester.pumpAndSettle();

      final scrollFinder = find.byKey(const ValueKey('wave-scroll-lead_vocal'));
      final scrollable = tester.widget<SingleChildScrollView>(scrollFinder);
      expect(scrollable.controller!.offset, 0.0);

      final gesture = await tester.startGesture(const Offset(250, 369));
      await tester.pump();
      await gesture.moveBy(const Offset(-40, 0));
      await tester.pump();
      await gesture.moveBy(const Offset(-60, 0));
      await tester.pump();
      await gesture.up();
      await tester.pumpAndSettle();

      expect(scrollable.controller!.offset, greaterThan(0.0));
    },
  );

  testWidgets(
    'Single tap on waveform seeks audio playback position',
    (tester) async {
      final mounted = await _mount(tester, size: const Size(360, 640));

      final center = tester.getCenter(find.byKey(const ValueKey('wave-lead_vocal')));
      await tester.tapAt(center);
      await tester.pump();

      expect(mounted.gateway.calls.any((call) => call.startsWith('seek:')), isTrue);
    },
  );

  testWidgets(
    'Transport buttons trigger audio engine commands',
    (tester) async {
      final mounted = await _mount(tester, size: const Size(360, 640));

      // Play/Pause
      await tester.tap(find.byKey(const ValueKey('transport-play-pause')));
      await tester.pump();
      expect(mounted.gateway.calls, contains('play'));

      // Stop
      await tester.tap(find.byKey(const ValueKey('transport-stop')));
      await tester.pump();
      expect(mounted.gateway.calls, contains('stop'));

      // Seek to 0
      await tester.tap(find.byKey(const ValueKey('transport-seek-0')));
      await tester.pump();
      expect(mounted.gateway.calls.any((c) => c.startsWith('seek:0')), isTrue);
    },
  );

  for (final landscapeSize in const [Size(844, 390), Size(915, 412)]) {
    testWidgets(
      'Widescreen landscape layout renders with sidebar and zero overflow on ${landscapeSize.width.toInt()}x${landscapeSize.height.toInt()} dp across all views',
      (tester) async {
        final mounted = await _mount(tester, size: landscapeSize);
        expect(tester.takeException(), isNull);
        // Bottom NavigationBar should not be rendered in widescreen landscape
        expect(find.byType(NavigationBar), findsNothing);
        expect(find.byKey(const ValueKey('transport-scrub-slider')), findsOneWidget);
        expect(find.byKey(const ValueKey('transport-play-pause')), findsOneWidget);

        for (final view in StudioView.values) {
          mounted.container.read(studioViewProvider.notifier).state = view;
          await tester.pumpAndSettle();
          expect(
            tester.takeException(),
            isNull,
            reason: 'View $view must render with zero overflow on $landscapeSize',
          );
        }
      },
    );
  }
}
