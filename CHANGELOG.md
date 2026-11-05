# Changelog

## 0.7.0

Current stable Windows x64 release. The installer runs inference on CPU.
Model weights are downloaded after consent and are not bundled.

- Live pitch uses a longer Signalsmith window scaled to the sample rate and an
  8 kHz tonality limit, so a key change keeps duration and avoids shifting the
  whole spectrum.
- Formant compensation is not a control. Monitoring and export use the same
  pitch setting. Older session files that still contain `preserve_formants`
  load their semitones and ignore that field.
- The workbench can change compute profile, execution preference and cache
  settings, and it receives native job updates without a status timer.
- Separation stays a bounded five-stem pipeline. Original stems are not
  rewritten by pitch or export.

Limits that remain outside this release: GPU runtimes are not shipped, physical
USB reconnection is not certified, and a clean-machine installer lifecycle was
not repeated for this tag.
