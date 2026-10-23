//! A ProCo RAT style distortion.
//!
//! Signal chain, modelled stage by stage on the original circuit:
//! 1. LM308 non-inverting gain stage: gain = 1 + Rdist / Zground, where the ground leg is
//!    47R + 2.2uF in parallel with 560R + 4.7uF (two high-pass "legs" at ~1.5 kHz and ~60 Hz).
//!    Bandwidth is limited by the 100pF feedback cap and the op-amp's gain-bandwidth product,
//!    and the output is slew-rate limited and clips at the rails.
//! 2. 1N914 diode pair hard clipping to ground.
//! 3. "Filter" control: one-pole low-pass, 1.5k + 100k pot into 3.3nF.
//! 4. Volume.
//!
//! The non-linear part runs at 4x oversampling to keep aliasing down.

use nih_plug::prelude::*;
use nih_plug_egui::EguiState;
use std::f32::consts::PI;
use std::sync::Arc;

mod editor;

const OVERSAMPLING: usize = 4;

// Circuit values
const R_DIST_MAX: f32 = 100e3;
const R_LEG_HI: f32 = 47.0;
const C_LEG_HI: f32 = 2.2e-6;
const R_LEG_LO: f32 = 560.0;
const C_LEG_LO: f32 = 4.7e-6;
const C_FEEDBACK: f32 = 100e-12;
const OPAMP_GBW: f32 = 1e6; // LM308 with 30pF compensation
const OPAMP_SLEW: f32 = 0.3e6; // V/s
const RAIL: f32 = 4.5;
const R_FILTER_MIN: f32 = 1.5e3;
const R_FILTER_POT: f32 = 100e3;
const C_FILTER: f32 = 3.3e-9;

/// Bypass crossfade time in seconds, long enough to avoid clicks
const BYPASS_FADE: f32 = 0.01;

/// Clipping diode pair, after the stock RAT and its two most famous variants.
#[derive(Enum, PartialEq, Clone, Copy)]
enum Diode {
    /// Stock 1N914
    Silicon,
    /// Turbo RAT: louder and more open
    #[name = "LED"]
    Led,
    /// You Dirty Rat (1N34A): quieter and more compressed
    Germanium,
}

impl Diode {
    fn forward_voltage(self) -> f32 {
        match self {
            Diode::Silicon => 0.6,
            Diode::Led => 1.7,
            Diode::Germanium => 0.3,
        }
    }
}

/// Topology-preserving one-pole filter, gives both low-pass and high-pass outputs.
#[derive(Default, Clone, Copy)]
struct OnePole {
    s: f32,
}

impl OnePole {
    /// `g` is `tan(pi * fc / fs)`. Returns the low-pass output.
    fn lp(&mut self, x: f32, g: f32) -> f32 {
        let v = (x - self.s) * g / (1.0 + g);
        let y = v + self.s;
        self.s = y + v;
        y
    }

    fn hp(&mut self, x: f32, g: f32) -> f32 {
        x - self.lp(x, g)
    }
}

fn prewarp(fc: f32, fs: f32) -> f32 {
    (PI * fc.min(fs * 0.45) / fs).tan()
}

#[derive(Default, Clone, Copy)]
struct Biquad {
    b0: f32,
    b1: f32,
    b2: f32,
    a1: f32,
    a2: f32,
    z1: f32,
    z2: f32,
}

impl Biquad {
    fn lowpass(fc: f32, fs: f32, q: f32) -> Self {
        let w0 = 2.0 * PI * fc / fs;
        let (sin, cos) = w0.sin_cos();
        let alpha = sin / (2.0 * q);
        let a0 = 1.0 + alpha;
        Self {
            b0: (1.0 - cos) / 2.0 / a0,
            b1: (1.0 - cos) / a0,
            b2: (1.0 - cos) / 2.0 / a0,
            a1: -2.0 * cos / a0,
            a2: (1.0 - alpha) / a0,
            z1: 0.0,
            z2: 0.0,
        }
    }

    fn process(&mut self, x: f32) -> f32 {
        let y = self.b0 * x + self.z1;
        self.z1 = self.b1 * x - self.a1 * y + self.z2;
        self.z2 = self.b2 * x - self.a2 * y;
        y
    }
}

/// 8th order Butterworth low-pass used as the anti-imaging/anti-aliasing filter.
#[derive(Default, Clone, Copy)]
struct AntiAlias([Biquad; 4]);

impl AntiAlias {
    fn new(fs_base: f32) -> Self {
        let fs = fs_base * OVERSAMPLING as f32;
        let fc = (fs_base * 0.45).min(20e3);
        let qs = [0.5098, 0.6013, 0.9000, 2.5629];
        Self(qs.map(|q| Biquad::lowpass(fc, fs, q)))
    }

    fn process(&mut self, x: f32) -> f32 {
        self.0.iter_mut().fold(x, |x, bq| bq.process(x))
    }
}

#[derive(Default, Clone, Copy)]
struct Channel {
    up: AntiAlias,
    down: AntiAlias,
    leg_hi: OnePole,
    leg_lo: OnePole,
    opamp_bw: OnePole,
    opamp_out: f32,
    tone: OnePole,
}

/// Coefficients shared by both channels, recomputed per sample from the smoothed parameters.
struct Coeffs {
    r_dist: f32,
    g_leg_hi: f32,
    g_leg_lo: f32,
    g_opamp: f32,
    max_step: f32,
    g_tone: f32,
    diode_v: f32,
}

impl Coeffs {
    fn new(dist: f32, filter: f32, diode: Diode, fs: f32) -> Self {
        let fs_os = fs * OVERSAMPLING as f32;
        // Audio taper (log) pot for distortion, linear pot for the filter like the original
        let r_dist = R_DIST_MAX * (10f32.powf(2.0 * dist) - 1.0) / 99.0;
        let r_ground = 1.0 / (1.0 / R_LEG_HI + 1.0 / R_LEG_LO);
        let noise_gain = 1.0 + r_dist / r_ground;
        let opamp_fc =
            (1.0 / (2.0 * PI * r_dist.max(1.0) * C_FEEDBACK)).min(OPAMP_GBW / noise_gain);
        let tone_fc = 1.0 / (2.0 * PI * (R_FILTER_MIN + R_FILTER_POT * filter) * C_FILTER);

        Self {
            r_dist,
            g_leg_hi: prewarp(1.0 / (2.0 * PI * R_LEG_HI * C_LEG_HI), fs_os),
            g_leg_lo: prewarp(1.0 / (2.0 * PI * R_LEG_LO * C_LEG_LO), fs_os),
            g_opamp: prewarp(opamp_fc, fs_os),
            max_step: OPAMP_SLEW / fs_os,
            g_tone: prewarp(tone_fc, fs),
            diode_v: diode.forward_voltage(),
        }
    }
}

impl Channel {
    fn process(&mut self, x: f32, c: &Coeffs) -> f32 {
        let mut out = 0.0;
        for i in 0..OVERSAMPLING {
            // Zero-stuffing upsampler, gain compensated
            let u = self
                .up
                .process(if i == 0 { x * OVERSAMPLING as f32 } else { 0.0 });

            // Current into the two ground legs times the feedback resistor
            let i_ground =
                self.leg_hi.hp(u, c.g_leg_hi) / R_LEG_HI + self.leg_lo.hp(u, c.g_leg_lo) / R_LEG_LO;
            let gained = u + self.opamp_bw.lp(c.r_dist * i_ground, c.g_opamp);

            // Rail clipping and slew limiting
            let target = RAIL * (gained / RAIL).tanh();
            self.opamp_out += (target - self.opamp_out).clamp(-c.max_step, c.max_step);

            let clipped = c.diode_v * (self.opamp_out / c.diode_v).tanh();
            out = self.down.process(clipped);
        }

        self.tone.lp(out, c.g_tone)
    }
}

struct Rodent {
    params: Arc<RodentParams>,
    sample_rate: f32,
    channels: [Channel; 2],
    /// 0 = bypassed, 1 = fully processed
    wet: f32,
}

#[derive(Params)]
struct RodentParams {
    // Renamed from "editor-state" so projects saved with the old 320x200 slider UI open at the new size
    #[persist = "editor-size"]
    editor_state: Arc<EguiState>,

    #[id = "distortion"]
    distortion: FloatParam,
    #[id = "filter"]
    filter: FloatParam,
    #[id = "volume"]
    volume: FloatParam,
    #[id = "diode"]
    diode: EnumParam<Diode>,
    #[id = "bypass"]
    bypass: BoolParam,
}

fn knob(name: &str, default: f32) -> FloatParam {
    FloatParam::new(name, default, FloatRange::Linear { min: 0.0, max: 1.0 })
        .with_smoother(SmoothingStyle::Linear(20.0))
        .with_unit("%")
        .with_value_to_string(formatters::v2s_f32_percentage(0))
        .with_string_to_value(formatters::s2v_f32_percentage())
}

impl Default for RodentParams {
    fn default() -> Self {
        Self {
            editor_state: EguiState::from_size(editor::WIDTH, editor::HEIGHT),
            distortion: knob("Distortion", 0.5),
            filter: knob("Filter", 0.3),
            volume: FloatParam::new(
                "Volume",
                util::db_to_gain(0.0),
                FloatRange::Skewed {
                    min: util::db_to_gain(-30.0),
                    max: util::db_to_gain(12.0),
                    factor: FloatRange::gain_skew_factor(-30.0, 12.0),
                },
            )
            .with_smoother(SmoothingStyle::Logarithmic(50.0))
            .with_unit(" dB")
            .with_value_to_string(formatters::v2s_f32_gain_to_db(1))
            .with_string_to_value(formatters::s2v_f32_gain_to_db()),
            diode: EnumParam::new("Diodes", Diode::Silicon),
            bypass: BoolParam::new("Bypass", false).make_bypass(),
        }
    }
}

impl Default for Rodent {
    fn default() -> Self {
        Self {
            params: Arc::new(RodentParams::default()),
            sample_rate: 48000.0,
            channels: [Channel::default(); 2],
            wet: 1.0,
        }
    }
}

impl Plugin for Rodent {
    const NAME: &'static str = "Rodent";
    const VENDOR: &'static str = "accraze";
    const URL: &'static str = "";
    const EMAIL: &'static str = "";
    const VERSION: &'static str = env!("CARGO_PKG_VERSION");

    const AUDIO_IO_LAYOUTS: &'static [AudioIOLayout] = &[
        AudioIOLayout {
            main_input_channels: NonZeroU32::new(2),
            main_output_channels: NonZeroU32::new(2),
            ..AudioIOLayout::const_default()
        },
        AudioIOLayout {
            main_input_channels: NonZeroU32::new(1),
            main_output_channels: NonZeroU32::new(1),
            ..AudioIOLayout::const_default()
        },
    ];

    const SAMPLE_ACCURATE_AUTOMATION: bool = true;

    type SysExMessage = ();
    type BackgroundTask = ();

    fn params(&self) -> Arc<dyn Params> {
        self.params.clone()
    }

    fn editor(&mut self, _async_executor: AsyncExecutor<Self>) -> Option<Box<dyn Editor>> {
        editor::create(self.params.clone())
    }

    fn initialize(
        &mut self,
        _audio_io_layout: &AudioIOLayout,
        buffer_config: &BufferConfig,
        _context: &mut impl InitContext<Self>,
    ) -> bool {
        self.sample_rate = buffer_config.sample_rate;
        self.reset();
        true
    }

    fn reset(&mut self) {
        let aa = AntiAlias::new(self.sample_rate);
        self.channels = [Channel {
            up: aa,
            down: aa,
            ..Channel::default()
        }; 2];
        self.wet = if self.params.bypass.value() { 0.0 } else { 1.0 };
    }

    fn process(
        &mut self,
        buffer: &mut Buffer,
        _aux: &mut AuxiliaryBuffers,
        _context: &mut impl ProcessContext<Self>,
    ) -> ProcessStatus {
        let fade_step = 1.0 / (BYPASS_FADE * self.sample_rate);

        for mut frame in buffer.iter_samples() {
            let coeffs = Coeffs::new(
                self.params.distortion.smoothed.next(),
                self.params.filter.smoothed.next(),
                self.params.diode.value(),
                self.sample_rate,
            );
            let volume = self.params.volume.smoothed.next();
            let target = if self.params.bypass.value() { 0.0 } else { 1.0 };
            self.wet += (target - self.wet).clamp(-fade_step, fade_step);

            // Keep processing while bypassed so filter states stay warm for a click-free return
            for (sample, channel) in frame.iter_mut().zip(self.channels.iter_mut()) {
                let dry = *sample;
                let wet = channel.process(dry, &coeffs) * volume;
                *sample = dry + (wet - dry) * self.wet;
            }
        }

        ProcessStatus::Normal
    }
}

impl ClapPlugin for Rodent {
    const CLAP_ID: &'static str = "com.accraze.rodent";
    const CLAP_DESCRIPTION: Option<&'static str> = Some("ProCo RAT style distortion");
    const CLAP_MANUAL_URL: Option<&'static str> = None;
    const CLAP_SUPPORT_URL: Option<&'static str> = None;
    const CLAP_FEATURES: &'static [ClapFeature] = &[
        ClapFeature::AudioEffect,
        ClapFeature::Stereo,
        ClapFeature::Mono,
        ClapFeature::Distortion,
    ];
}

impl Vst3Plugin for Rodent {
    const VST3_CLASS_ID: [u8; 16] = *b"RodentDistortion";
    const VST3_SUBCATEGORIES: &'static [Vst3SubCategory] =
        &[Vst3SubCategory::Fx, Vst3SubCategory::Distortion];
}

nih_export_clap!(Rodent);
nih_export_vst3!(Rodent);

#[cfg(test)]
mod tests {
    use super::*;

    /// Runs one second of a sine through a channel and returns the output once the filters settle.
    fn render(dist: f32, filter: f32, diode: Diode, fs: f32, freq: f32, amp: f32) -> Vec<f32> {
        let c = Coeffs::new(dist, filter, diode, fs);
        let aa = AntiAlias::new(fs);
        let mut ch = Channel {
            up: aa,
            down: aa,
            ..Channel::default()
        };
        let n = fs as usize;
        (0..n)
            .map(|i| ch.process(amp * (2.0 * PI * freq * i as f32 / fs).sin(), &c))
            .skip(n / 10)
            .collect()
    }

    fn peak(x: &[f32]) -> f32 {
        x.iter().fold(0.0, |m, y| m.max(y.abs()))
    }

    fn rms(x: &[f32]) -> f32 {
        (x.iter().map(|y| y * y).sum::<f32>() / x.len() as f32).sqrt()
    }

    #[test]
    fn output_is_bounded_and_not_silent() {
        for fs in [44100.0, 48000.0, 96000.0, 192000.0] {
            for diode in [Diode::Silicon, Diode::Led, Diode::Germanium] {
                for (dist, filter) in [(0.0, 0.0), (0.5, 0.3), (1.0, 0.0), (1.0, 1.0)] {
                    let out = render(dist, filter, diode, fs, 220.0, 0.3);
                    assert!(out.iter().all(|y| y.is_finite()));
                    let (peak, vf) = (peak(&out), diode.forward_voltage());
                    // Anti-alias filter ringing lets peaks overshoot the diode clip a bit
                    assert!(
                        peak > 0.05 && peak < 1.5 * vf,
                        "fs {fs} vf {vf} dist {dist} filter {filter}: peak {peak}"
                    );
                }
            }
        }
    }

    #[test]
    fn silence_in_silence_out() {
        let out = render(1.0, 0.0, Diode::Silicon, 48000.0, 0.0, 0.0);
        assert!(peak(&out) < 1e-6);
    }

    #[test]
    fn distortion_squares_up_the_wave() {
        // Crest factor (peak / RMS) is 1.41 for a sine and falls towards 1.0 as it clips into a square
        let crest = |dist| {
            let out = render(dist, 0.5, Diode::Silicon, 48000.0, 220.0, 0.3);
            peak(&out) / rms(&out)
        };
        let (clean, dirty) = (crest(0.0), crest(1.0));
        assert!(
            clean > 1.3 && dirty < clean - 0.2,
            "clean {clean}, dirty {dirty}"
        );
    }

    #[test]
    fn filter_knob_darkens() {
        let level = |filter| rms(&render(0.0, filter, Diode::Silicon, 48000.0, 4000.0, 0.1));
        let (open, dark) = (level(0.0), level(1.0));
        assert!(dark < 0.3 * open, "open {open}, dark {dark}");
    }
}
