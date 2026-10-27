# Auditoría y continuación de la migración Python → Rust/Flutter

Fecha: 27 de octubre de 2026. Rama única: `feature/complete-python-migration`.
Referencia: proyecto `Separador`, sus implementaciones y pruebas; contexto del
chat **migracion fases** y documentos de fases 1–5. Base nueva: `origin/develop`
en `f5b359d`. Referencia Python: `feature/atomusicator-performance` en
`b1179ae7702a482327a228a738d03371f54ff7f0`. No se modificó su código.

## Conclusión de la comparación inicial

Las cinco fases habían creado componentes y contratos, pero no una migración
funcional completa. El puente solo exponía versión y nombres de stems; Flutter
seleccionaba rutas y mantenía deshabilitados separación, reproducción y exportación.
El STFT genérico no sirve directamente para MDX: los modelos vocales usan FFT de
7680/5120 puntos y un hop de 1024, mientras que ese componente exige potencias
de dos y fija el hop en la mitad de la FFT. Faltaba el adaptador neuronal.

La caché antigua también estaba mal localizada: Python usa
`%LOCALAPPDATA%/Separador Vocal/Separador Vocal`, y Rust solo buscaba un nivel
`Separador Vocal`. Una prueba nueva reprodujo el fallo antes de corregirlo.

## Capacidades y resultado de esta rama

| Capacidad | Python de referencia | Situación inicial Rust/Flutter | Resultado implementado |
|---|---|---|---|
| Importar WAV/MP3/FLAC/M4A | Diálogo, arrastre, validación de audio | Solo selección de rutas/extensiones | Selección de fuente y arrastre; FFmpeg decodifica, valida y convierte a estéreo 44.1 kHz al separar; fuente conservada |
| Cinco stems | Cuatro perfiles MDX | Orquestador sin adaptador de inferencia | Inferencia ONNX real, tensores MDX, ventanas solapadas, residuales exactos y validación de reconstrucción |
| Voz principal/coros | KARA con destino primario `Instrumental` | Sin mapeo de salida del modelo | Invierte la predicción KARA para obtener voz principal; el residual conserva coros |
| Batería/bajo | Bajo consume el residual sin batería | Puerto genérico de ritmo | Conserva esa dependencia: batería → residual → bajo |
| Modelos | Caché y SHA-256, perfiles revisados | Manifiesto y gestor sin conexión a la app | Reutiliza caché antigua/nueva con SHA-256, descarga consentida, timeout, tamaño máximo y cancelación; rechaza perfiles no revisados |
| Caché de etapas | Checkpoints por entradas/runtime | Caché implementada pero desconectada | Conectada a MDX; clave incluye contenido, hash del modelo, FFT/hop, dimensiones, compensación, inversión, overlap y versión nativa |
| Progreso/cancelación | Workers/proceso de cómputo | Tokens/puertos sin API | Trabajos identificados, un trabajo activo, progreso por etapas, consulta desde Flutter, cancelación y error visible |
| Sesiones | JSON y WAV publicados en conjunto | Escritura transaccional sin lector de producto | Publicación, reapertura de formato nativo y de los cinco stems de Python, verificación de alineación y rechazo de rutas que escapen de la sesión |
| Reproducción | Mezcla sincronizada | CPAL sin API ni transporte completo | Play, pausa, stop, búsqueda común, fin de reproducción; cargado y controlado desde Flutter |
| Mezcla | Ganancia, mute/solo, panorama, master | Controles internos sin conexión | Controles conectados; master antes de clipping y medidor master basado en las muestras mezcladas |
| Formas de onda/zoom | Ondas y desplazamiento común | Cuadrícula decorativa | Picos reales desde Rust, playhead y búsqueda sobre la onda, regla temporal y zoom común 100–400 % |
| Pitch | ±12, Rubber Band, tempo constante, formantes | Ajuste de velocidad que altera duración | Render offline Rubber Band con tempo 1.0 y formantes opcionales; usa originales, publica lote completo y mantiene posición/mezcla; 0 restaura originales |
| Exportación | WAV/FLAC/MP3, selección y opciones | Solo puerto WAV | Lote transaccional de stems seleccionados, 44.1/48 kHz, mono/estéreo, WAV/FLAC 16/24 bits, MP3 320 kbps, loudnorm opcional y pitch activo |
| Proyectos | Carpeta configurable y QSettings | Política de rutas | Selector de carpeta escribible y preferencia JSON persistida en AppData; adopción no destructiva de carpetas antiguas |
| Puente | No aplica | Metadatos solamente | API generada con FRB **2.8.0**, trabajos, sesiones, transporte y mezcla; audio PCM permanece en Rust |
| Distribución | FFmpeg/FFprobe incluidos | Script sin herramientas de conversión | Preparador PowerShell con hashes de archivo/ZIP y licencias, paquete Flutter + DLL Rust + ONNX Runtime + FFmpeg/Rubber Band |

## Contratos numéricos y límites de equivalencia

Los parámetros se contrastaron con `mdx_model_data.json` de la caché que usa el
Python instalado y con las formas nativas de los cuatro modelos verificados:

| Perfil | FFT | Hop | Frecuencias | Frames de tensor | Compensación | Predicción invertida |
|---|---:|---:|---:|---:|---:|---|
| Main | 7680 | 1024 | 3072 | 256 | 1.043 | No |
| KARA 2 | 5120 | 1024 | 2048 | 256 | 1.065 | Sí |
| Kuielab drums | 4096 | 1024 | 2048 | 512 | 1.035 | No |
| Kuielab bass | 16384 | 1024 | 2048 | 512 | 1.035 | No |

El adaptador usa Hann periódica, centrado con reflexión, componentes reales e
imaginarias por canal, descarte de los tres primeros bins, padding y overlap
0.35. Una referencia independiente de `torch.stft` verifica la disposición y
los valores del tensor. Alterar deliberadamente su componente imaginaria hace
fallar esa prueba; se restauró el código y se verificó la versión correcta.

No se promete igualdad bit a bit con Python: se mantiene la decisión del plan
nativo de preservar la suma exacta de los cinco stems, aplicar VAD a bloques de
ocho segundos y redistribuir bleed espectral. El flujo Python normaliza algunas
salidas de forma independiente. El test de reconstrucción detecta pérdidas de
energía/alineación, pero **no demuestra calidad perceptual de aislamiento**.

La exportación toma cada archivo del stem con su pitch activo; ganancia de
escucha, mute/solo, panorama y master no cambian los archivos exportados, igual
que en la aplicación Python.

## Pendientes concretos, por prioridad

1. **Memoria y streaming para canciones largas.** El flujo conectado carga los
   cinco stems en memoria para garantizar búsqueda y sincronización. Existe un
   lector por ring buffers, pero requiere búsqueda coordinada, prebuffer y
   conversión de frecuencia de calidad fuera del callback antes de conectarlo al producto.
   Los underruns comunes, el último frame, el avance diferido y el fin común
   ya tienen correcciones verificadas. No se declara
   migrada la reproducción de memoria acotada ni la inferencia por archivo completo
   con consumo de memoria constante.
2. **Comparación musical y rendimiento.** Medir calidad de voz/coros, transitorios,
   SDR/null tests y tiempos/RAM sobre el mismo conjunto de audio licenciado usado
   por Python, con duración larga y límites de bloques. Se añadieron un corpus
   instrumental CC BY 4.0 y una canción local de 225.5 s, procesada completa con
   dos estrategias. La canción no tiene referencias vocales originales. El loop
   muestra separación de batería insuficiente. No se atribuyen mejoras de calidad, velocidad o
   latencia de hardware sin esas mediciones.
3. **GPU y política de recursos.** El escritorio usa CPU explícita, máximo seis
   hilos intra-op, un hilo inter-op, ejecución secuencial y spinning desactivado.
   El runtime y pool se conservan entre trabajos; la admisión cuenta sesiones
   todavía en uso, no solo entradas del LRU. Faltan presupuesto por bytes,
   medición de la política adaptativa y ramas paralelas. DirectML/CUDA siguen
   sin calificar; las pruebas no certifican aceleración GPU efectiva.
   La política CPU es parte de la paridad; GPU es una mejora calificada por
   proveedor, no un requisito para cerrar una migración CPU completa.
4. **Recuperación parcial y reanudación.** Los checkpoints exitosos se reutilizan,
   pero un trabajo fallido conserva la sesión anterior y no publica stems parciales.
   Python puede conservar resultados parciales de una rama fallida. Falta una
   representación explícita y una UI de recuperación para esos casos.
5. **Hardware y estados del flujo.** Hay medidores individuales en vivo, caché
   verificable de lotes de pitch y propagación tipada del fallo de salida hasta
   Flutter. Faltan desconexión/reconexión física y estados tipados de trabajos
   con su recuperación. Cancelar una inferencia
   espera a que termine la llamada ONNX en curso; no se interrumpe su kernel.
6. **Instalador completo.** La carpeta release compila con herramientas incluidas.
   Compilar/verificar el instalador requiere Inno Setup y una instalación limpia
   de Windows; no se considera comprobado por haber compilado solo la app.

La carpeta personalizada de QSettings ya se adopta mediante el registro Windows,
con prioridad del ajuste nativo y sin Qt/Python. También se corrigió la validación
repetida de rutas locales canónicas `\\?\C:\...`. Los lotes de pitch se reutilizan
por contenido original, especificación y huella del ejecutable FFmpeg; se verifican
hashes de salida y se descartan lotes incompletos. Guardar toda la edición de
mezcla/posición o restaurar automáticamente el último pitch siguen siendo mejoras
opcionales, no requisitos demostrados de paridad.

`native-v1` conserva los parámetros actuales del producto.
`context-candidate-v1` permite evaluar contexto vocal completo sin VAD/gate,
exclusivamente en el banco de pruebas. No se promovió como mejora de calidad.

El plan de implementación se conserva localmente en una carpeta excluida de Git.
Organiza la paridad obligatoria, la validación de calidad/rendimiento y las mejoras
opcionales. Se continuará en esta feature con commits independientes. No se
abrió PR, no se fusionaron ramas y no se añadieron workflows de Actions.

## Verificación reproducible

Resultados locales de esta ejecución:

| Verificación | Resultado |
|---|---|
| Python de referencia, auditoría anterior | 74 pruebas aprobadas entonces; no se volvió a ejecutar Python en esta implementación |
| Rust workspace con `--include-ignored --test-threads=1` | 79 pruebas aprobadas; se ejecutaron también modelos reales, admisión con sesiones vivas y FFmpeg |
| `cargo fmt --check` y Clippy con `-D warnings` | Aprobados |
| `flutter analyze` | Sin observaciones |
| `flutter test` con DLL/FFmpeg release configurados | 8 pruebas aprobadas, incluidos medidores, recuperación de dispositivo y smoke FFI; sin omisiones |
| Integración local de audio proporcionado | Dos separaciones completas, cinco WAV alineados por versión, reapertura y seek mediante la DLL empaquetada; prueba adicional local aprobada |
| Pantalla Flutter | Sesión vacía y cargada; 940×650/1440×900, mezcla, transporte, zoom y error largo sin desbordamiento |
| Preparador de audio y análisis sintáctico PowerShell | Aprobados; binarios cotejados con el manifiesto |
| `build_windows.ps1 -SkipInstaller` | Flutter release y Rust release compilados; DLL y herramientas copiadas al paquete |
| DLL empaquetada frente al build Rust | SHA-256 idéntico |
| Instalador Inno Setup / instalación limpia | No ejecutados en esta revisión |

Pruebas rápidas sin modelos ni dispositivo:

```powershell
cargo fmt --manifest-path rust_core/Cargo.toml --all -- --check
cargo clippy --manifest-path rust_core/Cargo.toml --workspace --all-targets -- -D warnings
cargo test --manifest-path rust_core/Cargo.toml --workspace
flutter analyze
flutter test
```

Pruebas reales, con los cuatro modelos ya presentes en la caché:

```powershell
./packaging/prepare_audio_tools.ps1
$env:ATOMUSICATOR_FFMPEG = (Resolve-Path build/ffmpeg-runtime/bin/ffmpeg.exe).Path
cargo test --manifest-path rust_core/Cargo.toml --workspace -- --include-ignored --test-threads=1
./packaging/build_windows.ps1 -SkipInstaller
$release = (Resolve-Path build/windows/x64/runner/Release).Path
$env:PATH = "$release;$env:PATH"
$env:ATOMUSICATOR_NATIVE_LIBRARY = Join-Path $release 'atomusicator_bootstrap_ffi.dll'
flutter test test/native_bridge_smoke_test.dart
```

El smoke FFI usa sesiones/audio sintéticos en una carpeta temporal y cruza los
bindings generados hasta la DLL compilada; comprueba reapertura, controles,
búsqueda, pitch y exportación. No reproduce grabaciones del usuario. Los modelos
y los artefactos de build no se incorporan a Git.
