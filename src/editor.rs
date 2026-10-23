//! Pedal-style editor: a black RAT-like enclosure with three knobs, painted by hand.

use crate::{Diode, RodentParams};
use nih_plug::prelude::*;
use nih_plug_egui::create_egui_editor;
use nih_plug_egui::egui::{
    self, Align2, Color32, FontId, Painter, Pos2, Rect, Sense, Stroke, StrokeKind, Ui, Vec2,
};
use std::f32::consts::{PI, TAU};
use std::sync::Arc;

pub const WIDTH: u32 = 360;
pub const HEIGHT: u32 = 440;

const KNOB_RADIUS: f32 = 30.0;
/// Knobs turn +-135 degrees from straight up
const SWEEP: f32 = 0.75 * PI;
const WHITE: Color32 = Color32::from_rgb(236, 234, 228);
const ENCLOSURE: Color32 = Color32::from_rgb(20, 20, 20);

pub fn create(params: Arc<RodentParams>) -> Option<Box<dyn Editor>> {
    create_egui_editor(
        params.editor_state.clone(),
        (),
        |_, _| {},
        move |ctx, setter, _| {
            egui::CentralPanel::default()
                .frame(egui::Frame::NONE.fill(Color32::from_gray(48)))
                .show(ctx, |ui| {
                    let body = ui.max_rect().shrink(12.0);
                    let p = ui.painter();
                    p.rect_filled(body, 14.0, ENCLOSURE);
                    p.rect_stroke(
                        body,
                        14.0,
                        Stroke::new(2.0_f32, Color32::from_gray(64)),
                        StrokeKind::Inside,
                    );
                    for corner in [
                        body.left_top(),
                        body.right_top(),
                        body.left_bottom(),
                        body.right_bottom(),
                    ] {
                        screw(p, corner + (body.center() - corner).normalized() * 22.0);
                    }

                    // RAT2 knob order
                    for (i, param) in [&params.volume, &params.filter, &params.distortion]
                        .into_iter()
                        .enumerate()
                    {
                        let x = body.left() + body.width() * (i as f32 + 0.5) / 3.0;
                        knob(ui, setter, param, Pos2::new(x, body.top() + 92.0));
                    }

                    let led = Pos2::new(body.center().x, body.top() + 178.0);
                    if params.bypass.value() {
                        p.circle_filled(led, 4.5, Color32::from_rgb(70, 12, 10));
                    } else {
                        p.circle_filled(
                            led,
                            10.0,
                            Color32::from_rgba_unmultiplied(255, 30, 20, 40),
                        );
                        p.circle_filled(led, 4.5, Color32::from_rgb(255, 50, 35));
                        p.circle_filled(
                            led + Vec2::new(-1.5, -1.5),
                            1.5,
                            Color32::from_rgb(255, 190, 180),
                        );
                    }

                    let switch_x = body.right() - body.width() / 6.0;
                    diode_switch(ui, setter, &params.diode, Pos2::new(switch_x, led.y));

                    let logo = Pos2::new(body.center().x, body.top() + 245.0);
                    // The default font has no bold weight, so fake one by overdrawing
                    for offset in [
                        Vec2::ZERO,
                        Vec2::new(1.0, 0.0),
                        Vec2::new(0.0, 1.0),
                        Vec2::splat(1.0),
                    ] {
                        p.text(
                            logo + offset,
                            Align2::CENTER_CENTER,
                            "RODENT",
                            FontId::proportional(58.0),
                            WHITE,
                        );
                    }
                    p.text(
                        logo + Vec2::new(0.0, 40.0),
                        Align2::CENTER_CENTER,
                        "D I S T O R T I O N",
                        FontId::proportional(12.0),
                        WHITE,
                    );

                    let switch = Pos2::new(body.center().x, body.bottom() - 62.0);
                    footswitch(ui, setter, &params.bypass, switch);
                });
        },
    )
}

/// Unit vector for a knob angle, measured clockwise from straight up.
fn dir(angle: f32) -> Vec2 {
    Vec2::new(angle.sin(), -angle.cos())
}

fn knob(ui: &Ui, setter: &ParamSetter, param: &FloatParam, center: Pos2) {
    let rect = Rect::from_center_size(center, Vec2::splat(2.0 * KNOB_RADIUS + 12.0));
    let response = ui.interact(rect, ui.id().with(param.name()), Sense::click_and_drag());

    // Accumulate drags in egui memory since the param value can lag behind the host round trip
    if response.drag_started() {
        setter.begin_set_parameter(param);
        ui.data_mut(|d| d.insert_temp(response.id, param.unmodulated_normalized_value()));
    }
    if response.dragged() {
        let speed = if ui.input(|i| i.modifiers.shift) {
            0.0005
        } else {
            0.005
        };
        let delta = response.drag_delta();
        let start = ui
            .data(|d| d.get_temp(response.id))
            .unwrap_or(param.unmodulated_normalized_value());
        let value = (start + (delta.x - delta.y) * speed).clamp(0.0, 1.0);
        ui.data_mut(|d| d.insert_temp(response.id, value));
        setter.set_parameter_normalized(param, value);
    }
    if response.drag_stopped() {
        setter.end_set_parameter(param);
    }
    if response.double_clicked() {
        setter.begin_set_parameter(param);
        setter.set_parameter(param, param.default_plain_value());
        setter.end_set_parameter(param);
    }

    if response.hovered() {
        let (notches, shift) = ui.input(|i| {
            let notches: f32 = i
                .events
                .iter()
                .map(|e| match e {
                    // Smooth-scrolling touchpads report points; treat ~50 points as one notch
                    egui::Event::MouseWheel {
                        unit: egui::MouseWheelUnit::Point,
                        delta,
                        ..
                    } => delta.y / 50.0,
                    egui::Event::MouseWheel { delta, .. } => delta.y,
                    _ => 0.0,
                })
                .sum();
            (notches, i.modifiers.shift)
        });
        if notches != 0.0 {
            let step = if shift { 0.002 } else { 0.02 };
            let value = (param.unmodulated_normalized_value() + notches * step).clamp(0.0, 1.0);
            setter.begin_set_parameter(param);
            setter.set_parameter_normalized(param, value);
            setter.end_set_parameter(param);
        }
    }

    let value = param.unmodulated_normalized_value();
    let angle = -SWEEP + 2.0 * SWEEP * value;
    let p = ui.painter();

    for i in 0..=10 {
        let a = -SWEEP + 2.0 * SWEEP * i as f32 / 10.0;
        let r = if i % 5 == 0 { 2.0 } else { 1.2 };
        p.circle_filled(center + dir(a) * (KNOB_RADIUS + 9.0), r, WHITE);
    }

    // Knurled skirt that turns with the knob
    p.circle_filled(
        center + Vec2::new(0.0, 3.0),
        KNOB_RADIUS,
        Color32::from_black_alpha(160),
    );
    p.circle_filled(center, KNOB_RADIUS, Color32::from_gray(8));
    for i in 0..40 {
        let a = angle + i as f32 * TAU / 40.0;
        p.line_segment(
            [
                center + dir(a) * (KNOB_RADIUS - 6.0),
                center + dir(a) * (KNOB_RADIUS - 0.5),
            ],
            Stroke::new(1.5_f32, Color32::from_gray(42)),
        );
    }
    p.circle_filled(center, KNOB_RADIUS - 7.0, Color32::from_gray(26));
    p.circle_stroke(
        center,
        KNOB_RADIUS - 7.0,
        Stroke::new(1.0_f32, Color32::from_gray(58)),
    );
    p.line_segment(
        [
            center + dir(angle) * 7.0,
            center + dir(angle) * (KNOB_RADIUS - 9.0),
        ],
        Stroke::new(3.0_f32, WHITE),
    );

    p.text(
        center - Vec2::new(0.0, KNOB_RADIUS + 18.0),
        Align2::CENTER_BOTTOM,
        param.name().to_uppercase(),
        FontId::proportional(13.0),
        WHITE,
    );
    if response.hovered() || response.dragged() {
        p.text(
            center + Vec2::new(0.0, KNOB_RADIUS + 16.0),
            Align2::CENTER_TOP,
            param.normalized_value_to_string(value, true),
            FontId::proportional(12.0),
            Color32::from_gray(170),
        );
    }
}

fn diode_switch(ui: &Ui, setter: &ParamSetter, param: &EnumParam<Diode>, center: Pos2) {
    const LABELS: [&str; 3] = ["SI", "LED", "GE"];
    const SPACING: f32 = 26.0;
    let slot = Rect::from_center_size(center, Vec2::new(SPACING * LABELS.len() as f32, 14.0));

    let response = ui.interact(slot.expand(8.0), ui.id().with("diode"), Sense::click());
    if let (true, Some(pos)) = (response.clicked(), response.interact_pointer_pos()) {
        let index = ((pos.x - slot.left()) / SPACING).clamp(0.0, 2.0) as usize;
        setter.begin_set_parameter(param);
        setter.set_parameter(param, Diode::from_index(index));
        setter.end_set_parameter(param);
    }

    let p = ui.painter();
    p.rect_filled(slot, 7.0, Color32::from_gray(4));
    p.rect_stroke(
        slot,
        7.0,
        Stroke::new(1.0_f32, Color32::from_gray(60)),
        StrokeKind::Outside,
    );
    let x = |i: usize| slot.left() + SPACING * (i as f32 + 0.5);
    for (i, label) in LABELS.iter().enumerate() {
        let pos = Pos2::new(x(i), slot.top() - 5.0);
        p.text(
            pos,
            Align2::CENTER_BOTTOM,
            label,
            FontId::proportional(10.0),
            WHITE,
        );
    }
    let lever = Pos2::new(x(param.value().to_index()), center.y);
    p.circle_filled(lever, 6.0, Color32::from_gray(185));
    p.circle_filled(lever + Vec2::new(-1.5, -1.5), 2.0, Color32::from_gray(240));
    p.text(
        Pos2::new(center.x, slot.bottom() + 5.0),
        Align2::CENTER_TOP,
        "DIODES",
        FontId::proportional(9.0),
        Color32::from_gray(150),
    );
}

fn screw(p: &Painter, center: Pos2) {
    p.circle_filled(center, 5.5, Color32::from_gray(90));
    p.circle_stroke(center, 5.5, Stroke::new(1.0_f32, Color32::from_gray(40)));
    p.line_segment(
        [center - dir(0.6) * 4.0, center + dir(0.6) * 4.0],
        Stroke::new(1.5_f32, Color32::from_gray(35)),
    );
}

fn footswitch(ui: &Ui, setter: &ParamSetter, bypass: &BoolParam, center: Pos2) {
    let rect = Rect::from_center_size(center, Vec2::splat(54.0));
    let response = ui.interact(rect, ui.id().with("footswitch"), Sense::click());
    if response.clicked() {
        setter.begin_set_parameter(bypass);
        setter.set_parameter(bypass, !bypass.value());
        setter.end_set_parameter(bypass);
    }

    // Hex nut, then the chrome plunger, which sinks a little while held
    let p = ui.painter();
    let nut: Vec<Pos2> = (0..6)
        .map(|i| center + dir(i as f32 * TAU / 6.0) * 27.0)
        .collect();
    p.add(egui::Shape::convex_polygon(
        nut,
        Color32::from_gray(110),
        Stroke::new(1.0_f32, Color32::from_gray(60)),
    ));
    p.circle_filled(center, 19.0, Color32::from_gray(150));
    p.circle_stroke(center, 19.0, Stroke::new(1.5_f32, Color32::from_gray(80)));
    let cap = if response.is_pointer_button_down_on() {
        11.5
    } else {
        13.0
    };
    p.circle_filled(center, cap, Color32::from_gray(195));
    p.circle_filled(center + Vec2::new(-4.0, -4.0), 4.0, Color32::from_gray(235));
}
