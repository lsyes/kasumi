//! `animation` — tweens, springs, presence transitions and transforms.
//!
//! ```sh
//! cargo run -p kasumi-demo --example animation
//! ```
//!
//! Three layers, from cheapest to most general:
//!
//! 1. `ui.animate(target, duration)` animates one scalar channel attached to the
//!    current container. The first call snaps from zero; a changed target
//!    restarts from the value visible now, so re-declaring every frame is safe.
//! 2. `ui.animate_with(key, target, MotionSpec)` names the channel, so one
//!    container can drive several independent animations, and chooses a spring
//!    or a tween with an explicit easing curve.
//! 3. `ui.transition(key, visible, Transition, ..)` keeps an exiting subtree
//!    alive until its exit motion completes. That is how content disappears
//!    smoothly without a manual "is it still visible?" flag.
//!
//! Everything is retained on the node and requested by the runtime: an
//! animation in flight asks for the next frame itself, and an idle window with
//! no animation sleeps.

use std::time::Duration;

use kasumi::App;
use kasumi::prelude::*;
use kasumi_backend::run;

#[path = "shared/mod.rs"]
mod shared;

use shared::{code, heading, micro, note, options};

/// One easing curve with a label, so the comparison table stays declarative.
///
/// `Easing` has four fixed curves plus `CubicBezier`, which is what the M3
/// motion tokens are built from. The last two rows below use the tokens the
/// theme exposes rather than hand-written control points.
fn easings(theme: &Theme) -> Vec<(&'static str, Easing)> {
    vec![
        ("Linear", Easing::Linear),
        ("EaseOutCubic", Easing::EaseOutCubic),
        ("EaseInOutCubic", Easing::EaseInOutCubic),
        ("Smoothstep", Easing::Smoothstep),
        ("M3 standard", Easing::M3_STANDARD),
        ("M3 decelerate", Easing::M3_DECELERATE),
        ("M3 accelerate", Easing::M3_ACCELERATE),
        ("M3 emphasized", Easing::M3_EMPHASIZED),
        ("theme standard", theme.motion.easing.standard),
        ("theme emphasized", theme.motion.easing.emphasized),
    ]
}

/// The four presence presets offered by the chips.
fn transitions() -> [(&'static str, Transition); 4] {
    [
        ("Fade", Transition::fade()),
        ("Slide", Transition::slide(0.0, 24.0)),
        ("Scale", Transition::scale(0.9)),
        ("Popover", Transition::popover()),
    ]
}

/// A single animated track: a progress value rendered as a filled bar.
fn track(ui: &mut Ui, label: &str, value: f32, color: Color, height: f32) {
    let theme = ui.theme().clone();
    ui.column(|ui| {
        ui.width_full().gap(3.0);
        ui.row(|ui| {
            ui.width_full().align_items(Align::Center);
            micro(ui, label);
            ui.spacer();
            code(ui, format!("{value:.3}"));
        });
        ui.column(|ui| {
            ui.width_full()
                .height(height)
                .radius(height * 0.5)
                .background(theme.colors.surface_container_high);
            ui.row(|ui| {
                ui.width(Length::Percent((value.clamp(0.0, 1.0)) * 100.0))
                    .height_full()
                    .radius(height * 0.5)
                    .background(color);
            });
        });
    });
}

struct Animation;

impl App for Animation {
    fn view(&mut self, ui: &mut Ui) {
        let theme = ui.theme().clone();
        let target = ui.state(|| 0.25f32);
        let playing = ui.state(|| true);
        let show_banner = ui.state(|| true);
        let presence = ui.state(|| 0usize);

        // A continuously advancing clock. `request_repaint` is enough here: the
        // runtime schedules one follow-up frame and lets the loop settle if the
        // animation is the only thing asking for frames.
        if playing.get() {
            ui.request_repaint();
        }
        let phase = ((ui.time() * 0.6).sin() * 0.5 + 0.5) as f32;
        let driven = if playing.get() { phase } else { target.get() };

        shared::page(
            ui,
            "Animation",
            "animate · animate_with · Transition · transforms",
            "animation",
            |ui| {
                ui.row(|ui| {
                    ui.width_full().gap(16.0).align_items(Align::Stretch);

                    // ------------------------------------------------------
                    // Tween vs spring
                    // ------------------------------------------------------
                    ui.column(|ui| {
                        ui.grow(1.0).basis(0.0);
                        ui.card(
                            Card::elevated()
                                .padding(theme.spacing.lg)
                                .gap(theme.spacing.md),
                            |ui| {
                                heading(ui, "Tween and spring", "animate_with");
                                note(
                                    ui,
                                    "Both channels below track the same target. The tween has a \
                                     fixed duration and an easing curve; the spring has a \
                                     response time and a damping ratio, so it can overshoot.",
                                );
                                let tween = ui.animate_with(
                                    "tween",
                                    driven,
                                    MotionSpec::tween(
                                        Duration::from_millis(320),
                                        Easing::EaseOutCubic,
                                    ),
                                );
                                let spring = ui.animate_with(
                                    "spring",
                                    driven,
                                    MotionSpec::spring(Spring::new(0.38, 0.62)),
                                );
                                track(
                                    ui,
                                    "tween · 320 ms · ease-out cubic",
                                    tween,
                                    theme.colors.primary,
                                    10.0,
                                );
                                track(
                                    ui,
                                    "spring · response 0.38 · damping 0.62",
                                    spring,
                                    theme.colors.tertiary,
                                    10.0,
                                );
                                track(ui, "target", driven, theme.colors.outline, 4.0);
                                ui.divider();
                                ui.row(|ui| {
                                    ui.width_full().gap(8.0).align_items(Align::Center);
                                    if ui.add(Button::new("Toggle motion").tonal()).clicked() {
                                        playing.update(|value| *value = !*value);
                                    }
                                    ui.add(
                                        Switch::new(&playing)
                                            .label("Playing")
                                            .keyboard_arrows_move_thumb(false),
                                    );
                                    ui.spacer();
                                    micro(ui, format!("frame {}", ui.frame()));
                                });
                            },
                        );
                    });

                    // ------------------------------------------------------
                    // Easing curves
                    // ------------------------------------------------------
                    ui.column(|ui| {
                        ui.grow(1.0).basis(0.0);
                        ui.card(
                            Card::elevated()
                                .padding(theme.spacing.lg)
                                .gap(theme.spacing.md),
                            |ui| {
                                heading(ui, "Easing curves", "Easing::sample");
                                note(
                                    ui,
                                    "Each row samples its curve at the same normalized time \
                                     `t` in 0..1, looping every 2.2 s while Playing is on. \
                                     `Easing::sample(t)` is the primitive behind a tween; \
                                     `animate_with` applies the same curve to a retained channel.",
                                );
                                // A 2.2 s loop keeps the curves visibly different without
                                // needing to recreate any retained animation channels.
                                let t = ((ui.time() % 2.2) / 2.2) as f32;
                                for (name, easing) in easings(&theme) {
                                    let value = easing.sample(t).clamp(0.0, 1.0);
                                    ui.row(|ui| {
                                        ui.width_full().gap(8.0).align_items(Align::Center);
                                        ui.column(|ui| {
                                            ui.width(120.0).flex_shrink(0.0);
                                            micro(ui, name);
                                        });
                                        ui.column(|ui| {
                                            ui.grow(1.0)
                                                .height(8.0)
                                                .radius(4.0)
                                                .background(theme.colors.surface_container_high);
                                            ui.row(|ui| {
                                                ui.width(Length::Percent(value * 100.0))
                                                    .height_full()
                                                    .radius(4.0)
                                                    .background(theme.colors.secondary);
                                            });
                                        });
                                    });
                                }
                            },
                        );
                    });
                });

                // -----------------------------------------------------------
                // Presence transition
                // -----------------------------------------------------------
                ui.card(
                    Card::elevated()
                        .padding(theme.spacing.lg)
                        .gap(theme.spacing.md),
                    |ui| {
                        heading(ui, "Presence transitions", "ui.transition");
                        note(
                            ui,
                            "When `show_banner` flips to false the subtree is not removed \
                             immediately: it finishes its exit motion, stops accepting input, \
                             and only then releases its nodes. Hide and re-show it quickly to \
                             watch the exit reverse into the enter.",
                        );
                        ui.row(|ui| {
                            ui.width_full().gap(10.0).align_items(Align::Center);
                            ui.add(
                                Switch::new(&show_banner)
                                    .label("Visible")
                                    .supporting("keyed by \"banner\""),
                            );
                            ui.spacer();
                            // The same table drives both the chips and the motion
                            // spec, so the two cannot drift apart.
                            for (index, (label, _)) in transitions().iter().enumerate() {
                                let active = presence.get() == index;
                                let mut button =
                                    Button::new(*label).font_size(10.5).padding_xy(10.0, 5.0);
                                button = if active {
                                    button.tonal()
                                } else {
                                    button.subtle()
                                };
                                if ui.add(button).clicked() {
                                    presence.set(index);
                                }
                            }
                        });

                        let transition = transitions()
                            .get(presence.get())
                            .map(|(_, spec)| *spec)
                            .unwrap_or_default();

                        ui.transition("banner", show_banner.get(), transition, |ui| {
                            ui.row(|ui| {
                                ui.width_full()
                                    .padding(14.0)
                                    .gap(12.0)
                                    .align_items(Align::Center)
                                    .radius(theme.shape.medium)
                                    .background(theme.colors.primary_container);
                                ui.add(
                                    Label::new("▣")
                                        .font_size(20.0)
                                        .color(theme.colors.on_primary_container),
                                );
                                ui.column(|ui| {
                                    ui.gap(2.0);
                                    ui.add(
                                        Label::new("Presence is retained")
                                            .font_size(13.0)
                                            .color(theme.colors.on_primary_container)
                                            .semibold(),
                                    );
                                    ui.add(
                                        Label::new(
                                            "The nodes stay in the tree until the exit \
                                                 motion finishes.",
                                        )
                                        .font_size(11.0)
                                        .color(theme.colors.on_primary_container),
                                    );
                                });
                            });
                        });
                    },
                );

                // -----------------------------------------------------------
                // Transforms
                // -----------------------------------------------------------
                ui.row(|ui| {
                    ui.width_full().gap(16.0).align_items(Align::Stretch);

                    ui.column(|ui| {
                        ui.grow(1.0).basis(0.0);
                        ui.card(
                            Card::outlined()
                                .padding(theme.spacing.lg)
                                .gap(theme.spacing.md),
                            |ui| {
                                heading(ui, "Transforms", "opacity · scale · translate");
                                note(
                                    ui,
                                    "Visual transforms are paint-time only: they do not affect \
                                     layout and they do not move hit-testing. An element scaled \
                                     up still reports the response of its untransformed rect.",
                                );
                                let amount = ui.animate_with(
                                    "transform",
                                    if playing.get() { 1.0 } else { 0.0 },
                                    MotionSpec::spring(Spring::new(0.5, 0.55)),
                                );
                                ui.row(|ui| {
                                    ui.width_full().gap(14.0).align_items(Align::Center);
                                    ui.column(|ui| {
                                        ui.width(56.0)
                                            .height(56.0)
                                            .radius(theme.shape.medium)
                                            .background(theme.colors.primary);
                                        ui.opacity(0.35 + amount * 0.65);
                                        ui.scale(0.8 + amount * 0.4);
                                        ui.transform_origin(0.5, 0.5);
                                    });
                                    ui.column(|ui| {
                                        ui.width(56.0)
                                            .height(56.0)
                                            .radius(theme.shape.medium)
                                            .background(theme.colors.tertiary);
                                        ui.translate(amount * 24.0, 0.0);
                                    });
                                    ui.column(|ui| {
                                        ui.grow(1.0);
                                        code(
                                            ui,
                                            format!(
                                                "opacity {:.2} · scale {:.2} · translate {:.1}",
                                                0.35 + amount * 0.65,
                                                0.8 + amount * 0.4,
                                                amount * 24.0
                                            ),
                                        );
                                    });
                                });
                            },
                        );
                    });

                    ui.column(|ui| {
                        ui.grow(1.0).basis(0.0);
                        ui.card(
                            Card::outlined()
                                .padding(theme.spacing.lg)
                                .gap(theme.spacing.md),
                            |ui| {
                                heading(ui, "Driving it yourself", "Ui::visual");
                                note(
                                    ui,
                                    "When the three conveniences are not enough, `ui.visual()` \
                                     hands you the retained `AnimationState` of the current node. \
                                     These are the same fields the transitions write.",
                                );
                                for line in [
                                    "visual.opacity.snap(0.5)      — instant",
                                    "visual.scale.set_target(..)    — animated",
                                    "visual.translate[0].snap(..)   — x offset",
                                    "visual.origin = Some((0.5,0.5))",
                                ] {
                                    code(ui, format!("{line}"));
                                }
                                ui.divider();
                                micro(
                                    ui,
                                    "Animations ask for frames themselves; an idle window \
                                     with no animation sleeps.",
                                );
                            },
                        );
                    });
                });
            },
        );
    }
}

fn main() -> anyhow::Result<()> {
    run(
        shared::QuitOnEscape { app: Animation },
        options("Kasumi — animation", 1120.0, 880.0),
    )
}
