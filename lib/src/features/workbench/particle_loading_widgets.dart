import 'dart:io';
import 'dart:math' as math;
import 'package:flutter/material.dart';
import '../../../core/theme/app_theme.dart';

bool isFlutterTestEnvironment() =>
    Platform.environment.containsKey('FLUTTER_TEST') ||
    WidgetsBinding.instance.runtimeType.toString().contains('Test');

bool _isFlutterTestEnvironment() => isFlutterTestEnvironment();

Color _stemParticleColor(String kind) => switch (kind) {
  'lead_vocal' => AppColors.coral,
  'backing_vocals' => AppColors.lavender,
  'drums' => AppColors.amber,
  'bass' => AppColors.blue,
  _ => AppColors.green,
};

/// Barra de progreso basada en partículas reactivas al porcentaje (0% -> 100%).
/// Mantiene un [LinearProgressIndicator] subyacente para accesibilidad y pruebas,
/// superponiendo una corriente suave y natural de partículas cuya densidad,
/// velocidad controlada y cresta luminosa avanzan progresivamente con [progress]
/// sin oscilar hacia atrás.
class ParticleProgressBar extends StatefulWidget {
  const ParticleProgressBar({
    super.key,
    this.progress,
    this.color = AppColors.seaGlass,
    this.backgroundColor,
    this.height = 6.0,
    this.showPercentageSpark = true,
    this.semanticsLabel,
  });

  final double? progress;
  final Color color;
  final Color? backgroundColor;
  final double height;
  final bool showPercentageSpark;
  final String? semanticsLabel;

  @override
  State<ParticleProgressBar> createState() => _ParticleProgressBarState();
}

class _ParticleProgressBarState extends State<ParticleProgressBar>
    with SingleTickerProviderStateMixin {
  late final AnimationController _controller;
  late final Stopwatch _indeterminateWatch;

  @override
  void initState() {
    super.initState();
    _indeterminateWatch = Stopwatch()..start();
    // Velocidad controlada y orgánica (~4.2 segundos por ciclo de partículas)
    _controller = AnimationController(
      vsync: this,
      duration: const Duration(milliseconds: 4200),
    );
    if (_isFlutterTestEnvironment()) {
      _controller.value = 0.5;
    } else {
      _controller.repeat();
    }
  }

  @override
  void didChangeDependencies() {
    super.didChangeDependencies();
    final disable = MediaQuery.maybeDisableAnimationsOf(context) ?? false;
    if (_isFlutterTestEnvironment() || disable) {
      if (_controller.isAnimating) _controller.stop();
      _controller.value = 0.5;
    } else if (!_controller.isAnimating) {
      _controller.repeat();
    }
  }

  @override
  void dispose() {
    _indeterminateWatch.stop();
    _controller.dispose();
    super.dispose();
  }

  double _resolveTargetProgress() {
    if (widget.progress != null) {
      return widget.progress!.clamp(0.0, 1.0);
    }
    if (_isFlutterTestEnvironment()) {
      return 0.45;
    }
    // Avance estrictamente progresivo y asintótico (nunca retrocede)
    final elapsedSec = _indeterminateWatch.elapsedMilliseconds / 1000.0;
    return (0.12 + 0.80 * (1.0 - math.exp(-elapsedSec / 1.8))).clamp(0.12, 0.92);
  }

  @override
  Widget build(BuildContext context) {
    final target = _resolveTargetProgress();
    final effectiveHeight = math.max(widget.height, 6.0);
    final isTest = _isFlutterTestEnvironment();

    Widget buildBar(double currentProgress) {
      return SizedBox(
        height: effectiveHeight + 4,
        child: Stack(
          alignment: Alignment.centerLeft,
          children: [
            ClipRRect(
              borderRadius: BorderRadius.circular(effectiveHeight),
              child: LinearProgressIndicator(
                value: currentProgress,
                color: widget.color.withValues(alpha: 0.38),
                backgroundColor:
                    widget.backgroundColor ??
                    widget.color.withValues(alpha: 0.08),
                minHeight: effectiveHeight,
                semanticsLabel: widget.semanticsLabel,
              ),
            ),
            Positioned.fill(
              child: IgnorePointer(
                child: RepaintBoundary(
                  child: AnimatedBuilder(
                    animation: _controller,
                    builder: (context, _) {
                      final liveTarget =
                          widget.progress != null
                              ? currentProgress
                              : _resolveTargetProgress();
                      return CustomPaint(
                        painter: _ParticleBarPainter(
                          progress: liveTarget,
                          phase: _controller.value,
                          color: widget.color,
                          showSpark: widget.showPercentageSpark,
                        ),
                      );
                    },
                  ),
                ),
              ),
            ),
          ],
        ),
      );
    }

    if (isTest) {
      return buildBar(target);
    }

    return TweenAnimationBuilder<double>(
      tween: Tween<double>(begin: 0.0, end: target),
      duration: const Duration(milliseconds: 520),
      curve: Curves.easeOutCubic,
      builder: (context, animatedProgress, _) => buildBar(animatedProgress),
    );
  }
}

class _ParticleBarPainter extends CustomPainter {
  const _ParticleBarPainter({
    required this.progress,
    required this.phase,
    required this.color,
    required this.showSpark,
  });

  final double progress;
  final double phase;
  final Color color;
  final bool showSpark;

  @override
  void paint(Canvas canvas, Size size) {
    if (size.width <= 0 || size.height <= 0) return;
    final cy = size.height / 2;
    final p = progress.clamp(0.0, 1.0);
    if (p <= 0.005) return;
    final fillWidth = (size.width * p).clamp(4.0, size.width);

    // Core glowing beam proportional to percentage
    final beamRect = RRect.fromRectAndRadius(
      Rect.fromLTWH(0, 2, fillWidth, math.max(2.0, size.height - 4)),
      Radius.circular(size.height),
    );
    canvas.drawRRect(
      beamRect,
      Paint()
        ..shader = LinearGradient(
          colors: [
            color.withValues(alpha: 0.42),
            color.withValues(alpha: 0.92),
          ],
        ).createShader(Rect.fromLTWH(0, 0, fillWidth, size.height)),
    );

    // Particle density scales smoothly with growing percentage (0% -> 100%)
    // Speed is controlled (1 integer cycle per controller period for seamless wrap)
    final count = 10 + (p * 32).round();
    final particlePaint = Paint()..style = PaintingStyle.fill;

    for (var i = 0; i < count; i++) {
      final seed = (i * 0.61803398875) % 1.0;
      final travel = (seed + phase) % 1.0;
      final x = travel * fillWidth;
      final yOffset =
          math.sin((seed * 12.0) + (phase * math.pi * 2)) *
          (size.height * 0.30);
      final radius = 1.0 + (seed * 1.3) + (travel * 0.6);
      final edgeFade = math.sin(travel * math.pi);
      final alpha = (0.25 + 0.65 * edgeFade).clamp(0.0, 1.0);
      particlePaint.color =
          Color.lerp(color, Colors.white, travel * 0.48)!.withValues(
            alpha: alpha,
          );
      canvas.drawCircle(Offset(x, cy + yOffset), radius, particlePaint);
    }

    // Spark crest at the leading percentage edge
    if (showSpark && p > 0.015) {
      final headX = fillWidth.clamp(3.0, size.width - 3.0);
      canvas.drawCircle(
        Offset(headX, cy),
        size.height * 0.54,
        Paint()..color = color.withValues(alpha: 0.25),
      );
      canvas.drawCircle(
        Offset(headX, cy),
        size.height * 0.26,
        Paint()..color = Colors.white.withValues(alpha: 0.90),
      );
      for (var s = 0; s < 5; s++) {
        final angle = (s / 5.0) * math.pi * 2 + (phase * math.pi * 2);
        final dist = 2.0 + ((phase + s * 0.2) % 1.0) * (size.height * 0.65);
        final sx = (headX + math.cos(angle) * dist).clamp(0.0, size.width);
        final sy = (cy + math.sin(angle) * (dist * 0.50)).clamp(
          0.0,
          size.height,
        );
        canvas.drawCircle(
          Offset(sx, sy),
          1.1,
          Paint()..color = Colors.white.withValues(alpha: 0.72),
        );
      }
    }
  }

  @override
  bool shouldRepaint(_ParticleBarPainter old) =>
      old.progress != progress || old.phase != phase || old.color != color;
}

/// Animación de partículas para la subida de archivos locales y decodificación
/// de forma de onda. Las partículas emergen con cadencia natural y cristalizan la
/// silueta de onda progresivamente de izquierda a derecha conforme el porcentaje
/// crece (0% -> 100%), además de incluir una barra progresiva monotónica.
class ParticleWaveIntakeLoader extends StatefulWidget {
  const ParticleWaveIntakeLoader({
    super.key,
    required this.progress,
    this.label = 'Analizando audio local y generando forma de onda…',
    this.sublabel,
    this.color = AppColors.seaGlass,
    this.compact = false,
  });

  final double progress;
  final String label;
  final String? sublabel;
  final Color color;
  final bool compact;

  @override
  State<ParticleWaveIntakeLoader> createState() =>
      _ParticleWaveIntakeLoaderState();
}

class _ParticleWaveIntakeLoaderState extends State<ParticleWaveIntakeLoader>
    with SingleTickerProviderStateMixin {
  late final AnimationController _controller;
  double _highestProgress = 0.0;

  @override
  void initState() {
    super.initState();
    _highestProgress = widget.progress.clamp(0.0, 1.0);
    // Cadencia suave y elegante (~4.8 segundos por ciclo de partículas)
    _controller = AnimationController(
      vsync: this,
      duration: const Duration(milliseconds: 4800),
    );
    if (_isFlutterTestEnvironment()) {
      _controller.value = 0.4;
    } else {
      _controller.repeat();
    }
  }

  @override
  void didUpdateWidget(covariant ParticleWaveIntakeLoader oldWidget) {
    super.didUpdateWidget(oldWidget);
    final incoming = widget.progress.clamp(0.0, 1.0);
    if (incoming < 0.05 && _highestProgress > 0.90) {
      _highestProgress = incoming;
    } else if (incoming > _highestProgress) {
      _highestProgress = incoming;
    }
  }

  @override
  void didChangeDependencies() {
    super.didChangeDependencies();
    final disable = MediaQuery.maybeDisableAnimationsOf(context) ?? false;
    if (_isFlutterTestEnvironment() || disable) {
      if (_controller.isAnimating) _controller.stop();
      _controller.value = 0.4;
    } else if (!_controller.isAnimating) {
      _controller.repeat();
    }
  }

  @override
  void dispose() {
    _controller.dispose();
    super.dispose();
  }

  @override
  Widget build(BuildContext context) {
    final targetP =
        _isFlutterTestEnvironment()
            ? widget.progress.clamp(0.0, 1.0)
            : math.max(_highestProgress, widget.progress.clamp(0.0, 1.0));
    final displayPct = (widget.progress.clamp(0.0, 1.0) * 100).round();

    Widget buildLoader(double smoothP) {
      return RepaintBoundary(
        child: LayoutBuilder(
          builder: (context, box) {
            final isVeryShort = box.maxHeight < 76 || widget.compact;
            return Stack(
              alignment: Alignment.center,
              children: [
                Positioned.fill(
                  child: AnimatedBuilder(
                    animation: _controller,
                    builder:
                        (context, _) => CustomPaint(
                          painter: _WaveIntakeParticlePainter(
                            progress: smoothP,
                            phase: _controller.value,
                            color: widget.color,
                          ),
                        ),
                  ),
                ),
                Padding(
                  padding: const EdgeInsets.symmetric(
                    horizontal: 14,
                    vertical: 4,
                  ),
                  child:
                      isVeryShort
                          ? Row(
                            mainAxisAlignment: MainAxisAlignment.center,
                            children: [
                              Container(
                                padding: const EdgeInsets.symmetric(
                                  horizontal: 8,
                                  vertical: 3,
                                ),
                                decoration: BoxDecoration(
                                  color: const Color(
                                    0xFF0A1218,
                                  ).withValues(alpha: 0.88),
                                  borderRadius: BorderRadius.circular(12),
                                  border: Border.all(
                                    color: widget.color.withValues(alpha: 0.45),
                                  ),
                                ),
                                child: Text(
                                  '$displayPct %',
                                  style: TextStyle(
                                    fontSize: 10,
                                    fontWeight: FontWeight.w700,
                                    color: widget.color,
                                  ),
                                ),
                              ),
                              const SizedBox(width: 10),
                              Flexible(
                                child: Text(
                                  widget.label,
                                  maxLines: 1,
                                  overflow: TextOverflow.ellipsis,
                                  style: const TextStyle(
                                    fontSize: 10,
                                    color: AppColors.text,
                                    fontWeight: FontWeight.w500,
                                  ),
                                ),
                              ),
                            ],
                          )
                          : Column(
                            mainAxisSize: MainAxisSize.min,
                            children: [
                              Container(
                                padding: const EdgeInsets.symmetric(
                                  horizontal: 12,
                                  vertical: 5,
                                ),
                                decoration: BoxDecoration(
                                  color: const Color(
                                    0xFF0A1218,
                                  ).withValues(alpha: 0.88),
                                  borderRadius: BorderRadius.circular(16),
                                  border: Border.all(
                                    color: widget.color.withValues(alpha: 0.5),
                                  ),
                                ),
                                child: Row(
                                  mainAxisSize: MainAxisSize.min,
                                  children: [
                                    Icon(
                                      Icons.auto_awesome,
                                      size: 13,
                                      color: widget.color,
                                    ),
                                    const SizedBox(width: 6),
                                    Text(
                                      '$displayPct %',
                                      style: TextStyle(
                                        fontSize: 12,
                                        fontWeight: FontWeight.w800,
                                        color: widget.color,
                                      ),
                                    ),
                                  ],
                                ),
                              ),
                              const SizedBox(height: 6),
                              Text(
                                widget.label,
                                maxLines: 1,
                                overflow: TextOverflow.ellipsis,
                                textAlign: TextAlign.center,
                                style: const TextStyle(
                                  fontSize: 10.5,
                                  fontWeight: FontWeight.w600,
                                  color: AppColors.text,
                                ),
                              ),
                              if (widget.sublabel != null) ...[
                                const SizedBox(height: 2),
                                Text(
                                  widget.sublabel!,
                                  maxLines: 1,
                                  overflow: TextOverflow.ellipsis,
                                  textAlign: TextAlign.center,
                                  style: const TextStyle(
                                    fontSize: 9,
                                    color: AppColors.muted,
                                  ),
                                ),
                              ],
                              const SizedBox(height: 8),
                              ConstrainedBox(
                                constraints: const BoxConstraints(
                                  maxWidth: 280,
                                ),
                                child: ParticleProgressBar(
                                  progress: smoothP.clamp(0.04, 1.0),
                                  color: widget.color,
                                  height: 5,
                                ),
                              ),
                            ],
                          ),
                ),
              ],
            );
          },
        ),
      );
    }

    if (_isFlutterTestEnvironment()) {
      return buildLoader(targetP);
    }

    return TweenAnimationBuilder<double>(
      tween: Tween<double>(begin: 0.04, end: targetP.clamp(0.04, 1.0)),
      duration: const Duration(milliseconds: 480),
      curve: Curves.easeOutCubic,
      builder: (context, smoothP, _) => buildLoader(smoothP),
    );
  }
}

class _WaveIntakeParticlePainter extends CustomPainter {
  const _WaveIntakeParticlePainter({
    required this.progress,
    required this.phase,
    required this.color,
  });

  final double progress;
  final double phase;
  final Color color;

  @override
  void paint(Canvas canvas, Size size) {
    if (size.width <= 0 || size.height <= 0) return;
    final cy = size.height / 2;
    final effectiveP = progress <= 0.01 ? 0.08 : progress.clamp(0.05, 1.0);
    final activeWidth = size.width * effectiveP;

    // 1. Coalescing harmonic waveform bars from 0 to activeWidth (progressive left-to-right)
    const bars = 48;
    final barSpacing = size.width / bars;
    for (var i = 0; i < bars; i++) {
      final x = (i + 0.5) * barSpacing;
      // Soft progressive reveal around the leading edge instead of abrupt jump
      final revealFactor = ((activeWidth - x) / (barSpacing * 2.5)).clamp(
        0.0,
        1.0,
      );
      final waveEnv =
          (math.sin(i * 0.38 + phase * math.pi * 2) * 0.38 + 0.62) *
          (0.32 + 0.68 * math.sin((i / bars) * math.pi));
      final barHalfHeight =
          size.height * 0.33 * waveEnv * (0.18 + 0.82 * revealFactor);
      final paint =
          Paint()
            ..color =
                Color.lerp(
                  AppColors.muted.withValues(alpha: 0.10),
                  color.withValues(alpha: 0.40),
                  revealFactor,
                )!
            ..strokeWidth = 2.2
            ..strokeCap = StrokeCap.round;
      canvas.drawLine(
        Offset(x, cy - barHalfHeight),
        Offset(x, cy + barHalfHeight),
        paint,
      );
    }

    // 2. Gentle streaming particles converging toward the active wave crest
    final particleCount = 18 + (effectiveP * 38).round();
    final dotPaint = Paint()..style = PaintingStyle.fill;
    for (var i = 0; i < particleCount; i++) {
      final seed = (i * 0.754877666) % 1.0;
      // Exactly 1 cycle per controller period for smooth, slow, continuous drift
      final t = (seed + phase) % 1.0;
      final x = (t * activeWidth).clamp(0.0, size.width);
      final amplitude =
          (1.0 - t * 0.55) *
          (size.height * 0.36) *
          math.cos(seed * 19.0 + phase * math.pi * 2);
      final y = (cy + amplitude).clamp(2.0, size.height - 2.0);
      final r = 1.1 + (t * 1.4);
      final fade = math.sin(t * math.pi);
      dotPaint.color =
          Color.lerp(color, Colors.white, t * 0.5)!.withValues(
            alpha: (0.20 + 0.65 * fade).clamp(0.0, 1.0),
          );
      canvas.drawCircle(Offset(x, y), r, dotPaint);
    }
  }

  @override
  bool shouldRepaint(_WaveIntakeParticlePainter old) =>
      old.progress != progress || old.phase != phase || old.color != color;
}

/// Reactor central de partículas para la pantalla de separación de stems.
/// Recibe partículas de la mezcla original (izquierda), las acelera con cadencia
/// natural en un prisma orbital con el porcentaje actual (0% -> 100%) y las divide
/// progresivamente en 2 o 5 haces de partículas coloreadas hacia los stems de salida.
class ParticleStemSplitterHero extends StatefulWidget {
  const ParticleStemSplitterHero({
    super.key,
    required this.progress,
    required this.activeStems,
    this.stageLabel = '',
  });

  final double progress;
  final List<String> activeStems;
  final String stageLabel;

  @override
  State<ParticleStemSplitterHero> createState() =>
      _ParticleStemSplitterHeroState();
}

class _ParticleStemSplitterHeroState extends State<ParticleStemSplitterHero>
    with SingleTickerProviderStateMixin {
  late final AnimationController _controller;

  @override
  void initState() {
    super.initState();
    // Velocidad controlada y serena (~4.5 segundos por ciclo)
    _controller = AnimationController(
      vsync: this,
      duration: const Duration(milliseconds: 4500),
    );
    if (_isFlutterTestEnvironment()) {
      _controller.value = 0.35;
    } else {
      _controller.repeat();
    }
  }

  @override
  void didChangeDependencies() {
    super.didChangeDependencies();
    final disable = MediaQuery.maybeDisableAnimationsOf(context) ?? false;
    if (_isFlutterTestEnvironment() || disable) {
      if (_controller.isAnimating) _controller.stop();
      _controller.value = 0.35;
    } else if (!_controller.isAnimating) {
      _controller.repeat();
    }
  }

  @override
  void dispose() {
    _controller.dispose();
    super.dispose();
  }

  @override
  Widget build(BuildContext context) {
    final p = widget.progress.clamp(0.0, 1.0);
    return SizedBox(
      width: 124,
      child: RepaintBoundary(
        child: AnimatedBuilder(
          animation: _controller,
          builder:
              (context, _) => CustomPaint(
                painter: _StemSplitterParticlePainter(
                  progress: p,
                  phase: _controller.value,
                  activeStems: widget.activeStems,
                ),
                child: Center(
                  child: Column(
                    mainAxisSize: MainAxisSize.min,
                    children: [
                      Text(
                        '${(p * 100).round()}%',
                        style: const TextStyle(
                          fontSize: 12,
                          fontWeight: FontWeight.w800,
                          color: Colors.white,
                        ),
                      ),
                      Text(
                        '${widget.activeStems.length} STEMS',
                        style: const TextStyle(
                          fontSize: 7.5,
                          letterSpacing: 0.7,
                          fontWeight: FontWeight.w700,
                          color: AppColors.seaGlass,
                        ),
                      ),
                    ],
                  ),
                ),
              ),
        ),
      ),
    );
  }
}

class _StemSplitterParticlePainter extends CustomPainter {
  const _StemSplitterParticlePainter({
    required this.progress,
    required this.phase,
    required this.activeStems,
  });

  final double progress;
  final double phase;
  final List<String> activeStems;

  @override
  void paint(Canvas canvas, Size size) {
    if (size.width <= 0 || size.height <= 0) return;
    final center = Offset(size.width / 2, size.height / 2);
    final p = progress.clamp(0.0, 1.0);
    final dotPaint = Paint()..style = PaintingStyle.fill;

    // 1. Incoming mix particles (left edge -> center prism) with smooth speed
    final incomingCount = 10 + (p * 18).round();
    for (var i = 0; i < incomingCount; i++) {
      final seed = (i * 0.6180339) % 1.0;
      final t = (seed + phase) % 1.0;
      final x = t * center.dx;
      final spread = (1.0 - t) * (size.height * 0.26) * math.sin(seed * 19.0);
      final y = center.dy + spread;
      dotPaint.color = AppColors.seaGlass.withValues(alpha: 0.22 + 0.62 * t);
      canvas.drawCircle(Offset(x, y), 1.2 + t * 1.0, dotPaint);
    }

    // 2. Outgoing stem particle beams (center prism -> right edge per stem)
    final stemCount = math.max(1, activeStems.length);
    for (var s = 0; s < stemCount; s++) {
      final stemColor = _stemParticleColor(activeStems[s]);
      // Staggered intensity per stem so stems activate progressively as separation advances
      final stemThreshold = (s / stemCount) * 0.72;
      final stemActivity = ((p - stemThreshold) / (1.0 - stemThreshold)).clamp(
        0.18,
        1.0,
      );
      final perStemParticles = 6 + (stemActivity * 11).round();
      final targetY =
          stemCount == 1
              ? center.dy
              : 28.0 + (s / (stemCount - 1)) * (size.height - 56.0);

      // Subtle guide beam
      final path =
          Path()
            ..moveTo(center.dx, center.dy)
            ..quadraticBezierTo(
              center.dx + (size.width - center.dx) * 0.45,
              center.dy,
              size.width,
              targetY,
            );
      canvas.drawPath(
        path,
        Paint()
          ..color = stemColor.withValues(alpha: 0.10 + 0.22 * stemActivity)
          ..style = PaintingStyle.stroke
          ..strokeWidth = 1.4,
      );

      for (var j = 0; j < perStemParticles; j++) {
        final seed = ((j + s * 7) * 0.4721359) % 1.0;
        final t = (seed + phase) % 1.0;
        final x = center.dx + t * (size.width - center.dx);
        final curveT = t * t;
        final y =
            center.dy +
            (targetY - center.dy) * curveT +
            math.sin(seed * 13 + phase * math.pi * 2) * 1.8;
        dotPaint.color =
            Color.lerp(stemColor, Colors.white, t * 0.35)!.withValues(
              alpha:
                  ((0.25 + 0.65 * (1.0 - (t - 0.5).abs())) * stemActivity)
                      .clamp(0.0, 1.0),
            );
        canvas.drawCircle(Offset(x, y), 1.3 + (1.0 - t) * 0.9, dotPaint);
      }
    }

    // 3. Central orbital prism ring proportional to progress (0 -> 360 deg)
    const ringRadius = 26.0;
    canvas.drawCircle(
      center,
      ringRadius,
      Paint()..color = const Color(0xFF0A1218).withValues(alpha: 0.92),
    );
    canvas.drawCircle(
      center,
      ringRadius,
      Paint()
        ..color = AppColors.seaGlass.withValues(alpha: 0.18)
        ..style = PaintingStyle.stroke
        ..strokeWidth = 2.5,
    );
    canvas.drawArc(
      Rect.fromCircle(center: center, radius: ringRadius),
      -math.pi / 2,
      math.pi * 2 * p.clamp(0.04, 1.0),
      false,
      Paint()
        ..color = AppColors.seaGlass
        ..style = PaintingStyle.stroke
        ..strokeCap = StrokeCap.round
        ..strokeWidth = 3.0,
    );

    // Orbiting spark particles around the central ring at controlled speed
    final orbitDots = 6 + (p * 10).round();
    for (var k = 0; k < orbitDots; k++) {
      final angle = (k / orbitDots) * math.pi * 2 + (phase * math.pi * 2);
      final ox = center.dx + math.cos(angle) * (ringRadius + 4.0);
      final oy = center.dy + math.sin(angle) * (ringRadius + 4.0);
      canvas.drawCircle(
        Offset(ox, oy),
        1.4,
        Paint()..color = AppColors.seaGlass.withValues(alpha: 0.80),
      );
    }
  }

  @override
  bool shouldRepaint(_StemSplitterParticlePainter old) =>
      old.progress != progress ||
      old.phase != phase ||
      old.activeStems.length != activeStems.length;
}

/// Visualizador de partículas para el proceso de Exportación de Stems (`ExportDialog`).
class ParticleExportVisualizer extends StatefulWidget {
  const ParticleExportVisualizer({
    super.key,
    required this.progress,
    required this.selectedStems,
    this.completed = false,
  });

  final double progress;
  final List<String> selectedStems;
  final bool completed;

  @override
  State<ParticleExportVisualizer> createState() =>
      _ParticleExportVisualizerState();
}

class _ParticleExportVisualizerState extends State<ParticleExportVisualizer>
    with SingleTickerProviderStateMixin {
  late final AnimationController _controller;

  @override
  void initState() {
    super.initState();
    // Velocidad controlada (~4.0 segundos por ciclo)
    _controller = AnimationController(
      vsync: this,
      duration: const Duration(milliseconds: 4000),
    );
    if (_isFlutterTestEnvironment()) {
      _controller.value = 0.5;
    } else {
      _controller.repeat();
    }
  }

  @override
  void didChangeDependencies() {
    super.didChangeDependencies();
    final disable = MediaQuery.maybeDisableAnimationsOf(context) ?? false;
    if (_isFlutterTestEnvironment() || disable) {
      if (_controller.isAnimating) _controller.stop();
      _controller.value = 0.5;
    } else if (!_controller.isAnimating) {
      _controller.repeat();
    }
  }

  @override
  void dispose() {
    _controller.dispose();
    super.dispose();
  }

  @override
  Widget build(BuildContext context) {
    final p = widget.completed ? 1.0 : widget.progress.clamp(0.0, 1.0);
    return Column(
      crossAxisAlignment: CrossAxisAlignment.stretch,
      children: [
        SizedBox(
          height: 44,
          child: RepaintBoundary(
            child: AnimatedBuilder(
              animation: _controller,
              builder:
                  (context, _) => CustomPaint(
                    painter: _ExportParticlePainter(
                      progress: p,
                      phase: _controller.value,
                      stems: widget.selectedStems,
                    ),
                  ),
            ),
          ),
        ),
        const SizedBox(height: 6),
        ParticleProgressBar(progress: p, height: 6),
      ],
    );
  }
}

class _ExportParticlePainter extends CustomPainter {
  const _ExportParticlePainter({
    required this.progress,
    required this.phase,
    required this.stems,
  });

  final double progress;
  final double phase;
  final List<String> stems;

  @override
  void paint(Canvas canvas, Size size) {
    if (size.width <= 0 || size.height <= 0) return;
    final cy = size.height / 2;
    final activeStems = stems.isEmpty ? const ['lead_vocal'] : stems;
    final p = progress.clamp(0.04, 1.0);
    final count = 14 + (p * 32).round();
    final dotPaint = Paint()..style = PaintingStyle.fill;

    for (var i = 0; i < count; i++) {
      final stemColor = _stemParticleColor(activeStems[i % activeStems.length]);
      final seed = (i * 0.6180339) % 1.0;
      final t = (seed + phase) % 1.0;
      final x = t * (size.width * p);
      final y =
          cy +
          math.sin(seed * 14.0 + phase * math.pi * 2) *
              (size.height * 0.30 * (1.0 - t * 0.45));
      final fade = math.sin(t * math.pi);
      dotPaint.color =
          Color.lerp(stemColor, Colors.white, t * 0.4)!.withValues(
            alpha: (0.25 + 0.65 * fade).clamp(0.0, 1.0),
          );
      canvas.drawCircle(Offset(x, y), 1.3 + t * 1.2, dotPaint);
    }
  }

  @override
  bool shouldRepaint(_ExportParticlePainter old) =>
      old.progress != progress || old.phase != phase;
}

/// Cargador orbital de partículas para etapas activas, galería de proyectos y dock.
class ParticleBurstLoader extends StatefulWidget {
  const ParticleBurstLoader({
    super.key,
    this.size = 32.0,
    this.progress,
    this.color = AppColors.seaGlass,
    this.label,
  });

  final double size;
  final double? progress;
  final Color color;
  final String? label;

  @override
  State<ParticleBurstLoader> createState() => _ParticleBurstLoaderState();
}

class _ParticleBurstLoaderState extends State<ParticleBurstLoader>
    with SingleTickerProviderStateMixin {
  late final AnimationController _controller;

  @override
  void initState() {
    super.initState();
    // Rotación orbital serena (~3.6 segundos por revolución)
    _controller = AnimationController(
      vsync: this,
      duration: const Duration(milliseconds: 3600),
    );
    if (_isFlutterTestEnvironment()) {
      _controller.value = 0.25;
    } else {
      _controller.repeat();
    }
  }

  @override
  void didChangeDependencies() {
    super.didChangeDependencies();
    final disable = MediaQuery.maybeDisableAnimationsOf(context) ?? false;
    if (_isFlutterTestEnvironment() || disable) {
      if (_controller.isAnimating) _controller.stop();
      _controller.value = 0.25;
    } else if (!_controller.isAnimating) {
      _controller.repeat();
    }
  }

  @override
  void dispose() {
    _controller.dispose();
    super.dispose();
  }

  @override
  Widget build(BuildContext context) {
    final ring = SizedBox(
      width: widget.size,
      height: widget.size,
      child: RepaintBoundary(
        child: AnimatedBuilder(
          animation: _controller,
          builder:
              (context, _) => CustomPaint(
                painter: _BurstOrbitPainter(
                  progress: widget.progress,
                  phase: _controller.value,
                  color: widget.color,
                ),
              ),
        ),
      ),
    );
    if (widget.label == null) return ring;
    return Column(
      mainAxisSize: MainAxisSize.min,
      children: [
        ring,
        const SizedBox(height: 8),
        Text(
          widget.label!,
          style: const TextStyle(fontSize: 10, color: AppColors.muted),
        ),
      ],
    );
  }
}

class _BurstOrbitPainter extends CustomPainter {
  const _BurstOrbitPainter({
    required this.progress,
    required this.phase,
    required this.color,
  });

  final double? progress;
  final double phase;
  final Color color;

  @override
  void paint(Canvas canvas, Size size) {
    final center = Offset(size.width / 2, size.height / 2);
    final radius = math.max(4.0, (math.min(size.width, size.height) / 2) - 3.0);
    final p = (progress ?? 0.65).clamp(0.1, 1.0);
    final dots = 8 + (p * 6).round();
    final paint = Paint()..style = PaintingStyle.fill;

    for (var i = 0; i < dots; i++) {
      final frac = i / dots;
      final angle = frac * math.pi * 2 + (phase * math.pi * 2);
      final active = progress == null || frac <= p;
      final pulse = 0.5 + 0.5 * math.sin(phase * math.pi * 2 - frac * math.pi * 2);
      paint.color =
          active
              ? Color.lerp(color, Colors.white, pulse * 0.45)!.withValues(
                alpha: (0.35 + 0.60 * pulse).clamp(0.0, 1.0),
              )
              : color.withValues(alpha: 0.14);
      final dotRadius = active ? (1.1 + pulse * 1.1) : 1.0;
      canvas.drawCircle(
        Offset(
          center.dx + math.cos(angle) * radius,
          center.dy + math.sin(angle) * radius,
        ),
        dotRadius,
        paint,
      );
    }
  }

  @override
  bool shouldRepaint(_BurstOrbitPainter old) =>
      old.progress != progress || old.phase != phase || old.color != color;
}
