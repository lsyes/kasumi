//! `async` — background loaders with loading, ready, error and refresh states.
//!
//! ```sh
//! cargo run -p kasumi-demo --example async
//! ```
//!
//! `ui.resource(..)` runs a loader on a shared background executor and returns
//! a `Resource<T>` handle. The value is cached for the declaring position, so
//! reaching the same call site again returns the same data instead of starting
//! another request.
//!
//! Three things make this practical:
//!
//! - **Stale-while-revalidate.** `reload()` keeps the previous value readable
//!   while the next request is in flight, so the UI never flashes back to a
//!   spinner.
//! - **No polling.** A finished worker wakes the winit event loop, and frames
//!   are produced only when something actually changed.
//! - **Cancellation by generation.** `cancel()` discards results from the
//!   in-flight request; it cannot interrupt a blocking call already on the
//!   worker thread, but the stale result never reaches the UI.
//!
//! Every fourth request fails on purpose so the error path is visible.

use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Duration;

use kasumi::App;
use kasumi::prelude::*;
use kasumi_backend::run;

#[path = "shared/mod.rs"]
mod shared;

use shared::{code, heading, micro, note, options};

#[derive(Clone)]
struct Snapshot {
    request: usize,
    users: usize,
    latency_ms: u64,
    tail: String,
}

static REQUESTS: AtomicUsize = AtomicUsize::new(0);
static FUTURES: AtomicUsize = AtomicUsize::new(0);

/// A deliberately slow, deliberately flaky loader.
fn load_snapshot() -> Result<Snapshot, String> {
    let request = REQUESTS.fetch_add(1, Ordering::Relaxed) + 1;
    std::thread::sleep(Duration::from_millis(650));
    if request % 4 == 0 {
        return Err(format!("simulated 503 on request {request}"));
    }
    Ok(Snapshot {
        request,
        users: 2_400 + request * 37,
        latency_ms: 28 + (request as u64 * 7) % 41,
        tail: format!("worker thread · generation {request}"),
    })
}

fn status_chip(ui: &mut Ui, label: &str, color: Color, text: Color) {
    let radius = ui.theme().shape.full;
    ui.row(|ui| {
        ui.padding_xy(9.0, 3.0)
            .align_items(Align::Center)
            .radius(radius)
            .background(color);
        ui.add(Label::new(label).font_size(10.0).color(text).semibold());
    });
}

struct Async;

impl App for Async {
    fn view(&mut self, ui: &mut Ui) {
        let theme = ui.theme().clone();
        let counter = ui.state(|| 0u32);

        shared::page(
            ui,
            "Async resources",
            "Resource<T> · loading / ready / error · reload and cancel",
            "async",
            |ui| {
                let snapshot = ui.resource_keyed("gallery::snapshot", load_snapshot);
                let status = snapshot.status();

                ui.card(
                    Card::elevated()
                        .padding(theme.spacing.lg)
                        .gap(theme.spacing.md),
                    |ui| {
                        heading(ui, "Blocking loader", "ui.resource_keyed");
                        note(
                            ui,
                            "The loader sleeps for 650 ms on a worker thread and fails on every \
                             fourth request. Reload keeps the old value visible while the next \
                             request is in flight; Cancel discards the result in flight.",
                        );

                        ui.row(|ui| {
                            ui.width_full().gap(10.0).align_items(Align::Center);
                            let (label, chip, text) = match status {
                                ResourceStatus::Idle => (
                                    "idle",
                                    theme.colors.surface_container_high,
                                    theme.colors.on_surface_variant,
                                ),
                                ResourceStatus::Loading if snapshot.has_value() => (
                                    "refreshing",
                                    theme.colors.tertiary_container,
                                    theme.colors.on_tertiary_container,
                                ),
                                ResourceStatus::Loading => (
                                    "loading",
                                    theme.colors.secondary_container,
                                    theme.colors.on_secondary_container,
                                ),
                                ResourceStatus::Ready => (
                                    "ready",
                                    theme.colors.success_container,
                                    theme.colors.on_success_container,
                                ),
                                ResourceStatus::Failed => (
                                    "failed",
                                    theme.colors.error_container,
                                    theme.colors.on_error_container,
                                ),
                            };
                            status_chip(ui, label, chip, text);
                            ui.spacer();
                            micro(ui, format!("{status:?}"));
                        });

                        match (status, snapshot.get()) {
                            (ResourceStatus::Loading, Some(stale)) => {
                                ui.add(
                                    Label::new(format!(
                                        "Refreshing… still showing request #{} ({} users)",
                                        stale.request, stale.users
                                    ))
                                    .font_size(20.0)
                                    .color(theme.colors.tertiary)
                                    .semibold(),
                                );
                            }
                            (ResourceStatus::Loading, None) => {
                                ui.add(
                                    Label::new("Loading on a background worker…")
                                        .font_size(20.0)
                                        .color(theme.colors.secondary)
                                        .semibold(),
                                );
                            }
                            (ResourceStatus::Failed, value) => {
                                ui.add(
                                    Label::new(format!(
                                        "Failed: {}",
                                        snapshot
                                            .error()
                                            .map(|error| error.message().to_owned())
                                            .unwrap_or_default()
                                    ))
                                    .font_size(15.0)
                                    .color(theme.colors.error),
                                );
                                if let Some(stale) = value {
                                    ui.add(
                                        Label::new(format!(
                                            "Last good value: request #{} · {} users · {} ms",
                                            stale.request, stale.users, stale.latency_ms
                                        ))
                                        .font_size(11.0)
                                        .color(theme.colors.outline)
                                        .family(FontFamily::Monospace),
                                    );
                                }
                            }
                            (_, Some(snapshot)) => {
                                ui.add(
                                    Label::new(format!("{} users online", snapshot.users))
                                        .font_size(26.0)
                                        .color(theme.colors.on_surface)
                                        .semibold(),
                                );
                                code(
                                    ui,
                                    format!(
                                        "request #{} · {} ms · {}",
                                        snapshot.request, snapshot.latency_ms, snapshot.tail
                                    ),
                                );
                            }
                            _ => {
                                ui.add(
                                    Label::new("No request has run yet.")
                                        .font_size(13.0)
                                        .color(theme.colors.outline),
                                );
                            }
                        }

                        ui.divider();

                        ui.row(|ui| {
                            ui.width_full().gap(8.0).align_items(Align::Center);
                            if ui.add(Button::new("Reload").primary()).clicked() {
                                snapshot.reload();
                            }
                            if ui.add(Button::new("Cancel").subtle()).clicked() {
                                snapshot.cancel();
                            }
                            if ui.add(Button::new("Inject value").subtle()).clicked() {
                                let request = REQUESTS.fetch_add(1, Ordering::Relaxed) + 1;
                                snapshot.set(Snapshot {
                                    request,
                                    users: 9_999,
                                    latency_ms: 0,
                                    tail: "injected from the UI thread".to_owned(),
                                });
                            }
                            ui.spacer();
                            micro(
                                ui,
                                format!("{} loader run(s)", REQUESTS.load(Ordering::Relaxed)),
                            );
                        });
                    },
                );

                ui.row(|ui| {
                    ui.width_full().gap(16.0).align_items(Align::Stretch);

                    // ------------------------------------------------------
                    // Future-backed resource
                    // ------------------------------------------------------
                    ui.column(|ui| {
                        ui.grow(1.0).basis(0.0);
                        ui.card(
                            Card::outlined()
                                .padding(theme.spacing.lg)
                                .gap(theme.spacing.md),
                            |ui| {
                                heading(ui, "Future resource", "ui.resource_future_keyed");
                                note(
                                    ui,
                                    "The factory returns a future, so `reload()` builds a fresh \
                                     one. The executor polls and parks; bring your own reactor \
                                     inside the future if the work needs one.",
                                );
                                let future =
                                    ui.resource_future_keyed("gallery::future", || async {
                                        let request = FUTURES.fetch_add(1, Ordering::Relaxed) + 1;
                                        std::thread::sleep(Duration::from_millis(420));
                                        Ok::<_, String>(format!(
                                            "future #{request} resolved · {}",
                                            if request == 1 {
                                                "first run"
                                            } else {
                                                "rebuilt by reload"
                                            }
                                        ))
                                    });
                                match (future.status(), future.get()) {
                                    (ResourceStatus::Ready, Some(value)) => {
                                        ui.add(
                                            Label::new(value)
                                                .font_size(13.0)
                                                .color(theme.colors.primary)
                                                .family(FontFamily::Monospace),
                                        );
                                    }
                                    (ResourceStatus::Loading, _) => {
                                        ui.add(
                                            Label::new("Polling the future…")
                                                .font_size(13.0)
                                                .color(theme.colors.outline),
                                        );
                                    }
                                    (ResourceStatus::Failed, _) => {
                                        ui.add(
                                            Label::new("The future rejected")
                                                .font_size(13.0)
                                                .color(theme.colors.error),
                                        );
                                    }
                                    _ => {
                                        ui.add(
                                            Label::new("No result yet")
                                                .font_size(13.0)
                                                .color(theme.colors.outline),
                                        );
                                    }
                                }
                                if ui
                                    .add(Button::new("Run the future again").subtle().full_width())
                                    .clicked()
                                {
                                    future.reload();
                                }
                            },
                        );
                    });

                    // ------------------------------------------------------
                    // Lifecycle notes
                    // ------------------------------------------------------
                    ui.column(|ui| {
                        ui.grow(1.0).basis(0.0);
                        ui.card(
                            Card::filled()
                                .padding(theme.spacing.lg)
                                .gap(theme.spacing.md),
                            |ui| {
                                heading(ui, "Cache lifecycle", "ResourceStore");
                                for line in [
                                    "Handles live on the runtime and survive frames.",
                                    "Unused slots expire after the resource TTL (600 frames).",
                                    "reload() keeps the previous value while status is Loading.",
                                    "Completion wakes the event loop through an EventLoopProxy.",
                                    "cancel() is generation-based; it never interrupts a worker.",
                                    "set(value) injects a value without running the loader.",
                                    "T and the loader must be Send + Sync + 'static.",
                                ] {
                                    ui.add(
                                        Label::new(format!("· {line}"))
                                            .font_size(11.5)
                                            .color(theme.colors.on_surface_variant),
                                    );
                                }
                            },
                        );
                    });
                });

                // ----------------------------------------------------------
                // Event-driven rendering proof
                // ----------------------------------------------------------
                ui.card(
                    Card::outlined()
                        .padding(theme.spacing.lg)
                        .gap(theme.spacing.md),
                    |ui| {
                        heading(ui, "Idle windows sleep", "request_repaint");
                        note(
                            ui,
                            "Without a running animation or a resource notification this window \
                             produces no frames at all. The counter below only advances while a \
                             timer is armed, which is what makes an idle desktop app cheap.",
                        );
                        ui.row(|ui| {
                            ui.width_full().gap(10.0).align_items(Align::Center);
                            ui.add(
                                Label::new(counter.get().to_string())
                                    .font_size(30.0)
                                    .color(theme.colors.on_surface)
                                    .semibold()
                                    .family(FontFamily::Monospace),
                            );
                            ui.spacer();
                            if ui.add(Button::new("Tick once").tonal()).clicked() {
                                counter.update(|value| *value += 1);
                            }
                            micro(ui, format!("frame {}", ui.frame()));
                        });
                    },
                );
            },
        );
    }
}

fn main() -> anyhow::Result<()> {
    run(
        shared::QuitOnEscape { app: Async },
        options("Kasumi — async resources", 1120.0, 880.0),
    )
}
