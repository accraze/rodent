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
- The "trailing semicolon in macro" warnings come from nih-plug's export macros, not our code.

## DSP structure

- Circuit values are named constants at the top of `lib.rs`; keep the model tied to real component values.
- `Coeffs::new(dist, filter, fs)` maps knob values to coefficients. It's shared by `process()` and the
  test, so change the mapping there only.
- `Channel::process` runs the non-linear stages (gain stage, rail clip, slew limit, diode clip) at
  `OVERSAMPLING`x, then applies the tone filter at the base rate.
- GUI lives in `src/editor.rs`: a hand-painted pedal (egui `Painter`) with a custom `knob()` widget.
  Drag to turn, Shift for fine, double-click resets. Layout is absolute within a fixed `WIDTH`x`HEIGHT` window.
  Changing the window size needs a new `#[persist]` key on `editor_state`, or old projects reopen at the old size.
- The audio thread must not allocate (`assert_process_allocs` is enabled).
- `CLAP_ID` and `VST3_CLASS_ID` must never change, or saved Bitwig projects lose the plugin.
