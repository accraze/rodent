# Rodent

A ProCo RAT style distortion plugin (CLAP / VST3) built with [nih-plug](https://github.com/robbert-vdh/nih-plug).

Each stage of the original circuit is modelled: the LM308 gain stage (including its
bandwidth and slew-rate limits), the 1N914 diode clipper, and the passive filter.
The non-linear part runs at 4x oversampling.

| Knob       | Range                                 |
|------------|---------------------------------------|
| Distortion | 0–100% (audio taper, up to ~66 dB)    |
| Filter     | 0–100% (clockwise = darker, ~32 kHz → 475 Hz) |
| Volume     | −30 to +12 dB                         |

## Build

Needs a recent Rust from [rustup](https://rustup.rs) (distro packages are usually too old).
The GUI (egui) also needs X11/GL headers:

```sh
sudo apt install libx11-xcb-dev libxcb1-dev libxcb-dri2-0-dev libxcb-icccm4-dev libxcursor-dev libxkbcommon-dev libxcb-shape0-dev libxcb-xfixes0-dev libgl-dev
```

```sh
cargo xtask bundle rodent --release
cargo test --lib
```

Bundles land in `target/bundled/`.

## Install (Linux)

```sh
cp target/bundled/Rodent.clap ~/.clap/
cp -r target/bundled/Rodent.vst3 ~/.vst3/   # optional
```

Then rescan plug-ins in your DAW.
