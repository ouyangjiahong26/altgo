# altgo

![altgo](assets/banner.png)

[![CI](https://github.com/ouyangjiahong26/altgo/actions/workflows/ci.yml/badge.svg)](https://github.com/ouyangjiahong26/altgo/actions/workflows/ci.yml)
[![Documentation](https://img.shields.io/badge/docs-online-2f6feb)](https://ouyangjiahong26.github.io/altgo/)
[![Release](https://img.shields.io/github/v/release/ouyangjiahong26/altgo)](https://github.com/ouyangjiahong26/altgo/releases)
[![License](https://img.shields.io/badge/license-MIT-green.svg)](LICENSE)

English | [简体中文](README.zh-CN.md)

altgo is a desktop speech-to-text tool. Hold the trigger key (right Alt by default), speak, and release. altgo records, transcribes and optionally polishes your speech, writes the result to the system clipboard and shows it on a single line in an overlay at the bottom of the screen.

Transcription uses one of two backends. The local backend embeds sherpa-onnx and offers SenseVoice (multilingual automatic detection) or FireRedASR2 CTC (better on mixed Chinese and English speech), and works offline. The online backend calls a cloud ASR service, MiMo by default, needs no model download and requires an API key.

altgo covers four combinations of Linux and Windows on x86_64 and arm64: Linux supports Ubuntu 22.04+ on x86_64 and aarch64, and Windows supports 10+ on x86_64 and arm64. macOS is not supported.

- [Documentation](https://ouyangjiahong26.github.io/altgo/)
- [Releases](https://github.com/ouyangjiahong26/altgo/releases)
- [Issues](https://github.com/ouyangjiahong26/altgo/issues)

## Features

### Recording and transcription

- Hold right Alt to speak and release to transcribe. The trigger key can be changed in Settings.
- Continuous mode: double-press the trigger key to start continuous recording, then single-press to stop. It suits longer speech.
- Local transcription: sherpa-onnx is embedded and models stay loaded in memory. Download and manage models in Settings, and choose SenseVoice, SenseVoice Cantonese-enhanced or FireRedASR2 CTC.
- Online transcription: cloud ASR, MiMo by default. No model download, and an API key is required.

![Overlay while recording: a red dot and a live level trace](assets/overlay-phase-recording.svg)

### Polishing and output

- LLM polishing (optional): four levels, support for the OpenAI-compatible protocol and the Anthropic Messages API, built-in presets for many providers and a fully offline local model.
- The result is written to the clipboard and shown on a single line in an overlay at the bottom of the screen.
- While transcribing, the overlay shows a spinner and a progress line.

![Overlay while transcribing: a spinner and a progress line](assets/overlay-phase-transcribing.svg)

![Overlay when done: a check mark and a one-line result, with a close button on hover](assets/overlay-phase-result-en.svg)

### History and desktop integration

- Local transcription history: view, copy, copy the raw text, delete, clear and polish again.
- Automatic text injection at the cursor (Windows only, off by default).
- Automatic update checks: silent at startup, with a manual check in Settings and release notes in a separate window.
- Tray icon: show the main window or quit the app.
- Adjustable appearance: light and dark themes that follow the system or a manual choice, plus three font sizes and three window sizes.
- Text only is stored. Audio is never saved.

## Installation

### Linux

Add the current user to the `input` group before installing, otherwise altgo cannot read keyboard devices. Log out and back in afterwards:

```bash
sudo usermod -aG input "$USER"
```

Download the `.deb`, `.rpm` or `.AppImage` for your architecture from [Releases](https://github.com/ouyangjiahong26/altgo/releases), then install or run it:

```bash
sudo apt install ./altgo_*.deb
# or
sudo dnf install ./altgo-*.rpm
# or
chmod +x altgo_*.AppImage && ./altgo_*.AppImage
```

The `.deb` and `.rpm` packages declare dependencies for desktop integration, audio, clipboard and `evtest`. The AppImage does not resolve dependencies, so install missing libraries based on the error at launch. On Wayland sessions, make sure `evtest` is installed and the current user can read `/dev/input/event*`.

After logging back in, start altgo and finish the transcription setup in Settings.

### Windows

Download the `*-setup.exe` for your device architecture (NSIS installer) from [Releases](https://github.com/ouyangjiahong26/altgo/releases), double-click to install, start altgo from the Start menu and finish the transcription setup in Settings. Choose the package that matches x64 or arm64.

## Quick start

After starting the app, complete these steps in Settings:

1. Choose a transcription backend. The local backend needs a model download first, and online transcription needs an API key.
2. Set the polishing level and service if needed.
3. Confirm the trigger key, right Alt by default.
4. Click Save.

Hold right Alt to start recording. On release altgo transcribes, polishes (optional), writes to the clipboard and shows the result in the overlay. History is stored in `~/.config/altgo/history.json` by default and contains text only.

## Documentation

- [Documentation site](https://ouyangjiahong26.github.io/altgo/): quick start, configuration, usage, architecture and FAQ
- [`CONTRIBUTING.md`](CONTRIBUTING.md): development environment, build, tests, CI and releases
- [`docs/architecture.md`](docs/architecture.md) and [`AGENTS.md`](AGENTS.md): system architecture and core modules
- [`docs/README.md`](docs/README.md): repository documentation index
- [`CHANGELOG.md`](CHANGELOG.md): version history

Repository and maintainer documentation is written in Chinese. The user documentation site is available in English and Simplified Chinese.

## Code signing policy

Windows installers (`*-setup.exe`) use free code signing provided by SignPath Foundation. After onboarding is complete, every release will be signed with the same identity.

> Free code signing provided by SignPath.io, certificate by SignPath Foundation

- Committers and reviewers: [ouyangjiahong26](https://github.com/ouyangjiahong26)
- Approvers: [ouyangjiahong26](https://github.com/ouyangjiahong26)
- Privacy policy: [Privacy Policy](https://ouyangjiahong26.github.io/altgo/docs/privacy)
- Onboarding status, requirements and release process: [`docs/code-signing.md`](docs/code-signing.md)

After signing is enabled, Windows SmartScreen and UAC prompts will show SignPath Foundation as the publisher. Reputation builds up over clean installs, so the first downloads of a new release may still show a warning.

## License

[MIT License](LICENSE)
