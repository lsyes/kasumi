use std::hint::black_box;
use std::sync::Arc;
use std::time::Instant;

use kasumi::prelude::*;
use kasumi::{App, Runtime};
use kasumi_core::{Color, DrawList, Id, Length, Point, Size, Style, TextContent, TextStyle};
use kasumi_layout::LayoutEngine;
use kasumi_paint::{InteractionState, TextEngine, paint};
use kasumi_tree::{NodeId, NodeKind, Tree};

const VIEWPORT: Size = Size::new(1280.0, 800.0);
const ROUNDS: usize = 7;

struct ListApp {
    rows: usize,
}

impl App for ListApp {
    fn view(&mut self, ui: &mut Ui) {
        ui.column(|ui| {
            ui.size_full().padding(16.0).gap(4.0);
            ui.add(Label::new("Static benchmark").font_size(22.0));
            for row in 0..self.rows {
                ui.keyed(row, |ui| {
                    ui.row(|ui| {
                        ui.height(32.0).gap(8.0);
                        ui.label("Fixed benchmark row");
                        ui.add(Button::new("Open").min_width(72.0));
                    });
                });
            }
        });
    }
}

struct StateApp {
    rows: usize,
    sum: u64,
}

impl App for StateApp {
    fn view(&mut self, ui: &mut Ui) {
        ui.column(|ui| {
            ui.size_full().padding(16.0).gap(4.0);
            for row in 0..self.rows {
                ui.keyed(row, |ui| {
                    let value = ui.state(move || row as u64);
                    self.sum = self.sum.wrapping_add(value.get());
                    ui.label("State benchmark row");
                });
            }
        });
    }
}

struct BenchTree {
    tree: Tree,
    label: NodeId,
}

fn benchmark_tree(rows: usize) -> BenchTree {
    let mut tree = Tree::new();
    tree.begin_frame(VIEWPORT);

    let mut first_label = None;
    for row in 0..rows {
        let row_id = tree.ensure(Id::from_raw(10_000 + row as u64), NodeKind::Container);
        let style = Style {
            height: Length::Px(32.0),
            gap: 8.0,
            ..Style::row()
        };
        tree.set_style(row_id, style);
        tree.declare_child(tree.root(), row_id);

        let label_id = tree.ensure(Id::from_raw(20_000 + row as u64), NodeKind::Text);
        tree.declare_child(row_id, label_id);
        tree.set_style(
            label_id,
            Style {
                width: Length::Px(360.0),
                ..Style::default()
            },
        );
        tree.set_text(
            label_id,
            TextContent::new("Fixed benchmark row", TextStyle::default()),
        );
        first_label.get_or_insert(label_id);

        let button_id = tree.ensure(Id::from_raw(30_000 + row as u64), NodeKind::Button);
        tree.declare_child(row_id, button_id);
        tree.set_style(
            button_id,
            Style {
                width: Length::Px(72.0),
                height: Length::Px(28.0),
                background: Color::rgb8(42, 104, 180),
                ..Style::default()
            },
        );
        tree.set_text(button_id, TextContent::new("Open", TextStyle::default()));
    }
    tree.end_frame();

    BenchTree {
        tree,
        label: first_label.expect("benchmark tree needs at least one row"),
    }
}

fn bench<F>(name: &str, iterations: usize, mut f: F)
where
    F: FnMut(),
{
    for _ in 0..2 {
        f();
    }

    let mut samples = Vec::with_capacity(ROUNDS);
    for _ in 0..ROUNDS {
        let started = Instant::now();
        for _ in 0..iterations {
            f();
        }
        samples.push(started.elapsed() / iterations as u32);
    }
    samples.sort_unstable();
    let median = samples[samples.len() / 2];
    let time = if median >= std::time::Duration::from_millis(1) {
        format!("{:.3} ms", median.as_secs_f64() * 1_000.0)
    } else if median >= std::time::Duration::from_micros(1) {
        format!("{:.3} us", median.as_secs_f64() * 1_000_000.0)
    } else {
        format!("{} ns", median.as_nanos())
    };
    println!("{name:<42} {time:>10}/frame   ({iterations} frames/round)");
}

fn main() {
    println!("Kasumi CPU frame benchmarks (median of {ROUNDS} rounds)");
    println!();

    let text_engine = Arc::new(TextEngine::new());

    for rows in [100, 500] {
        let mut runtime = Runtime::new(Arc::clone(&text_engine), VIEWPORT, Theme::default());
        let mut app = ListApp { rows };
        let mut input = kasumi::input_state(VIEWPORT, 0.0, Point::new(-1.0, -1.0));
        input.pointer_inside = false;
        bench(
            &format!("runtime/static_{}_rows", rows),
            if rows == 100 { 40 } else { 12 },
            || {
                runtime.frame(&mut app, input.clone());
            },
        );
    }

    {
        let mut runtime = Runtime::new(Arc::clone(&text_engine), VIEWPORT, Theme::default());
        let mut app = StateApp { rows: 250, sum: 0 };
        let mut input = kasumi::input_state(VIEWPORT, 0.0, Point::new(-1.0, -1.0));
        input.pointer_inside = false;
        bench("runtime/state_250_entries", 24, || {
            runtime.frame(&mut app, input.clone());
        });
        black_box(app.sum);
    }

    let tree_rows = 500;
    {
        let mut fixture = benchmark_tree(tree_rows);
        let mut layout = LayoutEngine::new();
        layout.compute(&mut fixture.tree, VIEWPORT, text_engine.as_ref());
        let name = format!("layout/clean_{}_nodes", fixture.tree.node_count());
        bench(&name, 100, || {
            fixture.tree.begin_frame(VIEWPORT);
            black_box(layout.compute(&mut fixture.tree, VIEWPORT, text_engine.as_ref()));
        });
    }

    {
        let mut fixture = benchmark_tree(tree_rows);
        let mut layout = LayoutEngine::new();
        layout.compute(&mut fixture.tree, VIEWPORT, text_engine.as_ref());
        let mut width = 360.0;
        let name = format!("layout/one_dirty_label_{}_nodes", fixture.tree.node_count());
        bench(&name, 40, || {
            fixture.tree.begin_frame(VIEWPORT);
            width = if width == 360.0 { 361.0 } else { 360.0 };
            let mut style = fixture
                .tree
                .get(fixture.label)
                .expect("label")
                .style
                .clone();
            style.width = Length::Px(width);
            fixture.tree.set_style(fixture.label, style);
            black_box(layout.compute(&mut fixture.tree, VIEWPORT, text_engine.as_ref()));
        });
    }

    {
        let mut fixture = benchmark_tree(tree_rows);
        let mut layout = LayoutEngine::new();
        layout.compute(&mut fixture.tree, VIEWPORT, text_engine.as_ref());
        let mut draw = DrawList::default();
        let name = format!("paint/cached_text_{}_nodes", fixture.tree.node_count());
        bench(&name, 80, || {
            draw.clear();
            black_box(paint(
                &fixture.tree,
                Arc::clone(&text_engine),
                InteractionState::default(),
                VIEWPORT,
                &mut draw,
            ));
        });
    }
}
