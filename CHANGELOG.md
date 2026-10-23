# Changelog

All notable changes to Rodent are documented here. The format follows
[Keep a Changelog](https://keepachangelog.com/en/1.1.0/), and versions follow
[Semantic Versioning](https://semver.org/).

## [Unreleased]

## [0.2.0] - 2026-10-23

### Added
- Factory preset bank: Classic, Edge, Crunch, Lead, Fuzz, Doom and Bright, stepped with the
  ‹ › selector next to the LED. Presets are level-matched to within 1 dB of Classic. The
  selector shows CUSTOM once the settings no longer match a preset.

## [0.1.1] - 2026-10-22

### Changed
- Licensed under GPL-3.0-or-later. Release zips now include the `LICENSE` file.

## [0.1.0] - 2026-10-22

### Added
- RAT circuit model: LM308 gain stage with bandwidth and slew-rate limits, diode clipper and
  passive filter, with 4x oversampling on the non-linear stages.
- Volume, Filter and Distortion knobs with the original pot tapers.
- Diode switch: SI (stock 1N914), LED (Turbo RAT) and GE (You Dirty Rat).
- Bypass on the footswitch, with a click-free crossfade and host bypass integration.
- Hand-drawn pedal GUI. The knobs support dragging, the mouse wheel, Shift for fine control and
  double-click to reset. The window is resizable and its size is saved with the project.

[Unreleased]: https://github.com/accraze/rodent/compare/v0.2.0...HEAD
[0.2.0]: https://github.com/accraze/rodent/compare/v0.1.1...v0.2.0
[0.1.1]: https://github.com/accraze/rodent/compare/v0.1.0...v0.1.1
[0.1.0]: https://github.com/accraze/rodent/releases/tag/v0.1.0
