# Rodent

ProCo RAT style distortion plugin (CLAP/VST3) in Rust using nih-plug. Single crate, all DSP in `src/lib.rs`.
The user runs it in Bitwig Studio on Linux (CLAP).

## Commands

```sh
cargo xtask bundle rodent --release && cp target/bundled/Rodent.clap ~/.clap/   # build + install
cargo test --release --lib -- --nocapture                                       # DSP sanity test
cargo clippy --release --lib
```

- The system has an old distro Rust (1.75) in `/usr/bin` that comes before `~/.cargo/bin` on PATH and
  can't build nih-plug. Prefix commands with `PATH="$HOME/.cargo/bin:$PATH"`.
- The GUI build needs X11/GL dev packages (listed in README); a missing `x11-xcb` pkg-config error means they're not installed.
- Bitwig needs the plugin re-added or a restart to pick up a new build.
- Releases: follow `docs/RELEASING.md` (CI publishes on annotated `v*` tags; never move an existing tag).
- The "trailing semicolon in macro" warnings come from nih-plug's export macros, not our code.

## DSP structure

- Circuit values are named constants at the top of `lib.rs`; keep the model tied to real component values.
- `Coeffs::new(dist, filter, fs)` maps knob values to coefficients. It's shared by `process()` and the
  test, so change the mapping there only.
- `Channel::process` runs the non-linear stages (gain stage, rail clip, slew limit, diode clip) at
  `OVERSAMPLING`x, then applies the tone filter at the base rate.
- GUI lives in `src/editor.rs`: a hand-painted pedal (egui `Painter`) with a custom `knob()` widget.
  Drag or mouse wheel to turn (2%/notch), Shift for fine, double-click resets. `pedal()` draws in a fixed
  `WIDTH`x`HEIGHT` design space on its own egui layer, which `create()` scales (input included) to fit the
  resizable window. Text is rasterized at 1x and stretched, so it softens when enlarged. The window size is
  persisted per project in `editor_state`.
- `Diode` enum sets the clipper's forward voltage (Si 0.6 V stock, LED 1.7 V Turbo RAT, Ge 0.3 V); output level
  changes with it like the real mods. Bypass crossfades to dry but keeps processing so filter state stays warm.
- The audio thread must not allocate (`assert_process_allocs` is enabled).
- `CLAP_ID` and `VST3_CLASS_ID` must never change, or saved Bitwig projects lose the plugin.
