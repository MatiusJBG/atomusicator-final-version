# Auditoría y continuación de la migración Python → Rust/Flutter

## Actualización visual del estudio — 27/10/2026

La referencia Figma proporcionada por el usuario se consultó mediante su sesión
autenticada: mezclador DAW, exportación y progreso de pitch. Flutter adopta sus
superficies oscuras, acciones turquesa y colores por stem. Los controles permanecen
fijos junto al timeline durante el zoom; transporte, pitch y master comparten el
panel inferior. Exportación permite elegir formato, destino, pistas e inclusión
de pistas silenciadas. Las formas de onda muestran los picos combinados reales
del puente; no se presentan como dos canales independientes.

Verificación de esta actualización: análisis Flutter sin observaciones, 12 pruebas
Flutter aprobadas con la DLL/FFmpeg configurados y render local adicional de una
sesión real mediante la DLL empaquetada. Se comprobaron 940×650 y 1440×900,
controles fijos al desplazar/zoom, pitch con formantes y selección efectiva de
exportación. Release Windows recompilado; instalador y las puertas restantes de
la migración conservaban su estado pendiente en esa actualización visual; el cierre técnico posterior se registra abajo. Los renders y referencias visuales
permanecen locales. No se modificó el DSP ni se añadieron dependencias Python.

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
| Pitch | ±12, Rubber Band, tempo constante, formantes | Ajuste de velocidad que altera duración | Desde 0.7.0: Signalsmith integrado, cinco stems alineados, controles en vivo sin Apply ni render, tempo constante, formantes opcionales, originales inmutables y ajustes locales. Se conserva la API offline anterior para compatibilidad. Evidencia y límites en [interactive-pitch.md](interactive-pitch.md) |
| Exportación | WAV/FLAC/MP3, selección y opciones | Solo puerto WAV | Lote transaccional de stems seleccionados, 44.1/48 kHz, mono/estéreo, WAV 16/24 bits o float32, FLAC 16/24 bits, MP3 320 kbps, loudnorm opcional y pitch activo |
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

## Cierre implementado: trabajos, audio largo y recuperación

El escritorio ejecuta la ruta serial por bloques. No vuelve a materializar la
canción ni los cinco stems completos para separar o reproducir. Se conserva el
adaptador en memoria como referencia de regresión; no es la ruta del estudio.
El producto y su paquete siguen sin Python.

| Responsabilidad | Implementación y garantía comprobada |
|---|---|
| Perfil DSP | `domain/quality.rs`: parámetros y modelos congelados, normalización, inversión, ventanas, VAD y gate versionados; se conserva `native-v1` |
| Trabajos | Máquina de estados tipada; `JobRegistry` separa ejecución/cancelación de Studio; límites legales de confirmación y fallos tipados |
| Diario | Revisiones inmutables con comprobación de versión; lease entre procesos desde la primera revisión; historial de reintentos y fuentes por SHA-256 |
| Recuperación | Reutiliza etapas completas verificadas por contenido y dependencias; invalida descendientes incompatibles; descartar solo limpia staging con propietario y ruta validados |
| Publicación | Cinco WAV y manifiesto completos antes del único rename de carpeta; cancelar antes no publica, cancelar después conserva la confirmación |
| Sesiones | Lector independiente valida cinco stems únicos, alineación, estado completo, rutas canónicas y checksums nuevos; admite JSON histórico sin checksums |
| Salida dañada | Recuperar verifica antes de abrir; si el resultado publicado está dañado y existe el original, crea otra sesión desde etapas válidas; conserva la carpeta dañada |
| PCM temporal | Spools float de 64 bits, pico/hash incremental, lectores por bloques, comprobación de finitud y longitud; no depende de RIFF para intermedios |
| MDX | Acumulación rolling de una ventana; mantiene rejilla, normalización global por llamada, orden float, Hann, padding y cola del perfil congelado |
| Residuales | Operaciones y reconstrucción por bloques, tolerancia del dominio `1e-5`; manifiestos de caché incluyen padres, modelos, runtime y hilos |
| Ondas | Índice min/max multinivel en disco: menos de 1 MiB por stem, buffers acotados y lectura por rango/nivel; zoom Flutter pide 1000–4000 buckets |
| Reproducción | Un productor y reloj para cinco pistas, cola SPSC acotada, prebuffer, generaciones por seek; WAV y resampling fuera del callback |
| Resampling | Sinc de 64 taps y 1024 fases compartidas; pruebas 44.1/48 kHz, mono, alineación, banda pasante y rechazo fuera de banda |
| Pitch | Lotes verificables con lease mientras sesión/exportación los usa; caché central de 2 GiB para retención inactiva; 0 restaura originales |
| Recursos | Reservas RAII de CPU/memoria, observación de memoria disponible y admisión de disco; retención de spools de 4 GiB sin eliminar lectores activos |
| Flutter | Estados tipados, recuperación/reintento/descarte, protección frente a polling/seek tardíos y lecturas de ondas de sesiones/pitch anteriores |

Las cuotas de caché son de retención, no límites sobre artefactos activos. Los
presupuestos de RAM son estimaciones conservadoras de admisión, no un límite de
RSS impuesto por Windows ni contabilidad exacta del allocator de ONNX Runtime.
La admisión CPU permite una ruta compacta cuando no quedan los 3 GiB libres
estimados para retener dos sesiones: libera modelos ONNX inactivos, limita su
caché a una sesión y vuelve a consultar la memoria del sistema. Exige al menos
2 GiB disponibles después de reservar 512 MiB para el sistema; no cambia modelos,
precisión, FFT ni hilos de inferencia. La caché reducida permanece así durante
la vida del pool. No elimina sesiones prestadas ni permite trabajos simultáneos.
La regresión con los cuatro modelos reales compara esta ruta con el baseline de
dos sesiones y exige error máximo por muestra `<=1e-6` en los cinco stems.
El espacio libre se verifica antes de trabajar y el proceso FFmpeg conserva un
margen de seguridad. Los cachés históricos en proyectos anteriores se preservan;
los nuevos renders de pitch se guardan en la caché nativa central.

Las salidas float conservan headroom aunque superen 1.0. La escucha limita la
salida final y el medidor informa el pico anterior al clipping. Exportar a WAV float32 sin
normalización conserva headroom y niveles; la conversión a PCM entero puede saturar. El diálogo
informa cuando los stems seleccionados superan 0 dBFS, ofrece float32 para conservarlos y explica que loudnorm,
si se selecciona, actúa por archivo y puede cambiar balances relativos. No se
normalizan stems silenciosamente.

Los WAV finales siguen limitados por RIFF a aproximadamente 4 GiB por archivo;
no se declara soporte RF64/BW64. El formato temporal sí usa tamaños y posiciones
u64. La cancelación espera la llamada ONNX en curso, pero impide publicar antes
del punto de confirmación. Las revisiones comprobadas incluyen terminación real de proceso durante
decodificación y antes/después de publicar; no equivalen a una certificación frente a pérdida eléctrica del disco.

## Diferencias históricas conservadas

| Aspecto | Referencia Python inspeccionada | Decisión nativa |
|---|---|---|
| Preprocesamiento y normalización | `native_mdx.py` conserva los de audio-separator | Pico global de entrada por llamada y residuales exactos; no se promete igualdad bit a bit con Python |
| KARA | La salida `Vocals` se presenta como voz principal, `Instrumental` como coros | Predicción primaria KARA invertida explícitamente para voz principal; mismos cuatro modelos revisados |
| Contexto vocal | El orquestador inspeccionado entrega la rama vocal a su separador | Perfil congelado con VAD de 8 s y gate; el candidato de contexto completo sigue experimental |
| Batería y bajo | Bajo consume el residual de batería | Se conserva la dependencia y se comprueba reconstrucción |
| Ramas | Admite dos workers para voz e instrumentos | Serial; no se habilita paralelismo de ramas sin calificación de recursos y rendimiento |
| Fallos parciales | Puede devolver `PartialSeparationError` con stems disponibles | Conserva sesión previa y etapas verificadas; ofrece recuperación; solo muestra sesión final con cinco pistas completas |

El candidato no se promovió. La igualdad de suma no demuestra asignación musical
correcta y la regresión frente al motor nativo congelado no sustituye la
comparación perceptual con Python ni referencias vocales independientes.

## Evidencia CPU de esta ejecución

Una separación completa del FLAC local de 225.5 s mediante la API de escritorio:
9,944,550 frames, cinco pistas, 418.34 s de proceso y pico RSS muestreado de
1,460,228,096 bytes (1.36 GiB). Las cinco salidas coinciden muestra por muestra
con el baseline nativo anterior. Error máximo de reconstrucción: `2.98e-7`.
La medición corresponde al proceso nativo; excluye shell Flutter y codecs hijos.
Se ejecutó sin otras compilaciones ni inferencias controladas por este trabajo.
No se compara su tiempo con ejecuciones anteriores contaminadas por otras cargas.

Matriz serial de un fragmento local de 24 s, dos pasadas por configuración,
caché de etapas nueva en cada pasada y mismo perfil/modelos:

| Hilos intra-op | Pasada 1, s | Pasada 2, s |
|---:|---:|---:|
| 1 | 182.79 | 183.85 |
| 2 | 113.25 | 112.46 |
| 4 | 70.33 | 71.01 |
| 6 | 61.98 | 63.38 |

Las ocho salidas coinciden muestra por muestra entre configuraciones y cumplen
reconstrucción `<=1e-5`. Se mantiene máximo seis hilos, inter-op uno, ejecución
secuencial y sin spinning. Es la mejor configuración serial medida aquí; no un
óptimo universal ni una justificación para abrir dos ramas simultáneas.

El kernel real Main FFT/overlap-add recorrió 5, 30 y 60 minutos con fuente
virtual no silenciosa y sink contador, sin modelos ONNX. Emitió exactamente
13,230,000 / 79,380,000 / 158,760,000 frames; el mayor bloque escrito mantuvo
169,728 frames en las tres duraciones. Pico RSS: 41,979,904 / 41,099,264 /
40,857,600 bytes, respectivamente; memoria privada en torno a 35 MiB. La prueba
recorre todas sus ventanas y no materializa PCM completo. Su alcance excluye
inferencia neuronal, archivos finales y GUI; no sustituye una campaña de una
hora con modelos reales. Los JSON locales conservan mediciones y alcance.

## Distribución Windows

Inno Setup 6.7.3 produjo un instalador real. Se incluyeron runtimes MSVC locales,
DLL Rust/ONNX y FFmpeg/Rubber Band, junto con manifiesto SHA-256 de archivos.
La distribución calificada es CPU: se excluyen DLL de proveedores GPU no
calificados. El build falla si faltan las tres DLL MSVC esenciales.

La prueba de instalador usa AppId aleatorio, ruta con espacios/Unicode y modo
usuario; no altera instalaciones existentes. Verifica instalación, arranque,
actualización de versión declarada 0.5.9→0.6.0 con el mismo payload actual y
desinstalación, conservando fixtures de proyectos y AppData. Esto no certifica
actualización desde todos los binarios históricos de producción.

La carpeta release arranca con PATH limitado a Windows, verifica hashes y
filtro Rubber Band. La prueba CPAL real, con master a cero, comprueba reloj,
pausa, búsquedas repetidas y EOF de la sesión larga. No acredita escucha audible
ni desconexión/reconexión física. La política de runtimes locales corresponde a
la [distribución Windows de Flutter](https://docs.flutter.dev/platform-integration/windows/building).

## Validación y puertas todavía abiertas

| Verificación | Resultado registrado |
|---|---|
| Rust workspace, incluidos modelos y FFmpeg | 114 pruebas aprobadas; se ejecutaron también las opcionales de modelos/FFmpeg y el proceso hijo real |
| Formato Rust y Clippy `-D warnings` | Aprobados; sin warnings |
| Flutter analyze / tests con DLL real | 18 pruebas y análisis aprobados con DLL/FFmpeg empaquetados; render local adicional de sesión real aprobado |
| Regresiones inducidas | Ondas sin mínimos correctos, limpieza sin lease y reanudación de publicación corrupta hacen fallar sus pruebas; código restaurado y pruebas aprobadas |
| Pitch | 0, ±1, ±7 y ±12, con/sin formantes: frecuencia y duración; originales, caché y cinco pistas comprobados por pruebas de integración |
| Exportación | 48 combinaciones de formato/profundidad/canales/frecuencia/normalización inspeccionadas con FFprobe; cuantización PCM y headroom float32 >1 comprobados |
| Playback | Alineación, underrun, EOF, seek/generaciones y fallo de productor; ruta de driver real comprobada sin salida audible |
| Paquete / instalador | Build e instalador reales, 33 archivos verificados, arranque PATH reducido e instalación/actualización/desinstalación aisladas aprobados |

Permanecen abiertas las siguientes puertas del cierre completo:

- Corpus diverso con al menos doce fragmentos independientes, referencias reales
  de voz/coros y escucha comparada. El FLAC disponible no contiene esas referencias;
  repetirlo no amplía la diversidad musical. El loop instrumental disponible tuvo
  separación de batería insuficiente y no permite atribuir mejoras vocales.
- Campaña completa de aplicación/modelos en canciones de 30/60 minutos, con
  inferencia y reproducción simultáneas, cancelación/reanudación y registro de
  underruns. La prueba del kernel largo y la canción completa de 225.5 s tienen
  alcances distintos; no se presentan como esa campaña completa.
- Desconexión y reconexión física del dispositivo y escucha prolongada.
- Instalación y flujo musical en Windows limpio. Sandbox está deshabilitado en
  este equipo; un PATH reducido y una instalación aislada en el host no sustituyen
  esa máquina. Tampoco se certifica pérdida eléctrica, disco realmente lleno ni
  todas las combinaciones de permisos por las pruebas de publicación simuladas.

GPU sigue opcional y sin calificar. Las puertas anteriores no se marcan completas
por contar commits o pruebas. Los planes, audios, pesos y evidencia privada están
excluidos de Git; esta auditoría registra contratos y resultados verificables.

## Reproducción de comprobaciones

```powershell
$env:ATOMUSICATOR_FFMPEG = (Resolve-Path build/ffmpeg-runtime/bin/ffmpeg.exe).Path
cargo fmt --manifest-path rust_core/Cargo.toml --all -- --check
cargo clippy --manifest-path rust_core/Cargo.toml --workspace --all-targets -- -D warnings
cargo test --manifest-path rust_core/Cargo.toml --workspace -- --include-ignored --test-threads=1
./packaging/build_windows.ps1
$release = (Resolve-Path build/windows/x64/runner/Release).Path
$env:ATOMUSICATOR_NATIVE_LIBRARY = Join-Path $release 'atomusicator_bootstrap_ffi.dll'
flutter analyze
flutter test
./packaging/validate_windows_release.ps1 -ReleaseDirectory $release -Launch
./packaging/test_windows_installer.ps1 -ReleaseDirectory $release -InnoCompiler build/toolchain/inno/ISCC.exe
```

Los modelos locales deben coincidir con el manifiesto. Regenerar bindings con
FRB 2.8.0. Las pruebas opcionales nunca descargan modelos. Los runners
`desktop_bench`, `desktop_playback_check`, `cpu_thread_bench` y
`mdx_memory_bench` documentan sus argumentos y alcance; sus entradas/salidas
permanecen locales. El proyecto no necesita el repositorio Python para ejecutarse.
