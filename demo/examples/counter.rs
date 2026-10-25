use kasumi::App;
use kasumi::prelude::*;
use kasumi_backend::{WindowOptions, run};

struct Counter;

impl App for Counter {
    fn view(&mut self, ui: &mut Ui) {
        let count = ui.state(|| 0i32);

        ui.column(|ui| {
            ui.size_full().padding(24.0).gap(12.0);
            ui.add(Label::new("Counter").font_size(22.0).semibold());

            ui.row(|ui| {
                ui.gap(8.0);
                if ui.button("-").clicked() {
                    count.update(|value| *value -= 1);
                }
                ui.add(
                    Label::new(count.get().to_string())
                        .font_size(28.0)
                        .family(FontFamily::Monospace),
                );
                if ui.add(Button::new("+").primary()).clicked() {
                    count.update(|value| *value += 1);
                }
            });
        });
    }
}

fn main() {
    run(Counter, WindowOptions::default()).expect("kasumi event loop");
}
