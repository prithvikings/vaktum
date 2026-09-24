# Vaktum

Local-first Windows desktop voice dictation.

## Local Whisper transcription

Vaktum uses embedded whisper.cpp through `whisper-rs 0.16.0`.

Place the English `base.en` GGML model at:

```
%LOCALAPPDATA%\\Vaktum\\models\\ggml-base.en.bin
```

The model is intentionally not bundled with the application or committed to this repository.

To use a different model path during development, set:

```
VAKTUM_WHISPER_MODEL=C:\\path\\to\\ggml-model.bin
```

No model downloading or cloud transcription is implemented.
