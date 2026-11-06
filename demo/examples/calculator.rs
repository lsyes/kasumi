//! `calculator` — a keypad application with a small pure model.
//!
//! ```sh
//! cargo run -p kasumi-demo --example calculator
//! ```
//!
//! The calculator keeps a `State<Calc>` holding the display, the pending
//! operand and the operator. Every key press is handled by reading a `Response`
//! and applying a pure transition to the model — the framework only ever sees
//! one `view` function.
//!
//! It also demonstrates keyboard input: digits, `+ - * /`, Enter, Backspace,
//! Escape and `%` are mapped in `view` with `ui.key_pressed`, so the on-screen
//! keypad and the physical keyboard use the same command path.

use kasumi::App;
use kasumi::core::Key;
use kasumi::prelude::*;
use kasumi_backend::run;

#[path = "shared/mod.rs"]
mod shared;

use shared::{code, heading, micro, note, options};

#[derive(Clone, Copy, PartialEq, Eq)]
enum Op {
    Add,
    Sub,
    Mul,
    Div,
}

impl Op {
    fn symbol(self) -> &'static str {
        match self {
            Op::Add => "+",
            Op::Sub => "−",
            Op::Mul => "×",
            Op::Div => "÷",
        }
    }

    fn apply(self, left: f64, right: f64) -> Option<f64> {
        match self {
            Op::Add => Some(left + right),
            Op::Sub => Some(left - right),
            Op::Mul => Some(left * right),
            Op::Div => {
                if right.abs() < f64::EPSILON {
                    None
                } else {
                    Some(left / right)
                }
            }
        }
    }
}

#[derive(Clone)]
struct Calc {
    display: String,
    pending: Option<f64>,
    operator: Option<Op>,
    /// True when the next digit should start a fresh number.
    fresh: bool,
    error: Option<String>,
}

impl Default for Calc {
    fn default() -> Self {
        Self {
            display: "0".to_owned(),
            pending: None,
            operator: None,
            fresh: true,
            error: None,
        }
    }
}

impl Calc {
    fn clear(&mut self) {
        *self = Self::default();
    }

    fn digit(&mut self, d: char) {
        if self.error.is_some() {
            self.clear();
        }
        if self.fresh {
            self.display = d.to_string();
            self.fresh = false;
        } else if self.display.len() < 15 {
            self.display.push(d);
        }
    }

    fn dot(&mut self) {
        if self.error.is_some() {
            self.clear();
        }
        if self.fresh {
            self.display = "0.".to_owned();
            self.fresh = false;
        } else if !self.display.contains('.') {
            self.display.push('.');
        }
    }

    fn value(&self) -> f64 {
        self.display.parse().unwrap_or(0.0)
    }

    fn set_value(&mut self, value: f64) {
        if !value.is_finite() {
            self.error = Some("not a number".to_owned());
            self.display = "0".to_owned();
        } else if value.abs() >= 1e15 {
            self.error = Some("overflow".to_owned());
            self.display = "0".to_owned();
        } else {
            // Trim the usual binary-float noise without losing small values.
            self.display = format!("{value:.10}")
                .trim_end_matches('0')
                .trim_end_matches('.')
                .to_owned();
            if self.display.is_empty() || self.display == "-" {
                self.display = "0".to_owned();
            }
        }
        self.fresh = true;
        self.error = None;
    }

    fn operate(&mut self, next: Op) {
        if self.error.is_some() {
            self.clear();
            return;
        }
        let current = self.value();
        if let (Some(left), Some(op)) = (self.pending, self.operator) {
            if !self.fresh {
                match op.apply(left, current) {
                    Some(value) => self.set_value(value),
                    None => {
                        self.error = Some("cannot divide by zero".to_owned());
                        self.pending = None;
                        self.operator = None;
                        return;
                    }
                }
                self.pending = Some(self.value());
            } else {
                self.pending = Some(left);
            }
        } else {
            self.pending = Some(current);
        }
        self.operator = Some(next);
        self.fresh = true;
    }

    fn equals(&mut self) {
        let current = self.value();
        if let (Some(left), Some(op)) = (self.pending.take(), self.operator.take()) {
            match op.apply(left, current) {
                Some(value) => self.set_value(value),
                None => self.error = Some("cannot divide by zero".to_owned()),
            }
        }
        self.fresh = true;
    }

    fn percent(&mut self) {
        self.set_value(self.value() / 100.0);
    }

    fn negate(&mut self) {
        let value = -self.value();
        self.set_value(value);
    }

    fn backspace(&mut self) {
        if self.error.is_some() {
            self.clear();
            return;
        }
        let mut chars: Vec<char> = self.display.chars().collect();
        chars.pop();
        if chars.is_empty() || chars == ['-'] {
            self.display = "0".to_owned();
            self.fresh = true;
        } else {
            self.display = chars.into_iter().collect();
        }
    }
}

/// One keypad key, styled by its role.
fn key(ui: &mut Ui, label: &str, kind: KeyKind) -> Response {
    let theme = ui.theme().clone();
    ui.row(|ui| {
        ui.height(52.0)
            .align_items(Align::Center)
            .justify(Justify::Center)
            .radius(theme.shape.medium)
            .background(match kind {
                KeyKind::Digit => theme.colors.surface_container_high,
                KeyKind::Operator => theme.colors.secondary_container,
                KeyKind::Accent => theme.colors.primary_container,
                KeyKind::Equals => theme.colors.primary,
            })
            .border(0.0, Color::TRANSPARENT);
        ui.clickable();
        let color = match kind {
            KeyKind::Digit => theme.colors.on_surface,
            KeyKind::Operator => theme.colors.on_secondary_container,
            KeyKind::Accent => theme.colors.on_primary_container,
            KeyKind::Equals => theme.colors.on_primary,
        };
        ui.add(
            Label::new(label)
                .font_size(18.0)
                .color(color)
                .weight(FontWeight::MEDIUM),
        );
    })
}

#[derive(Clone, Copy)]
enum KeyKind {
    Digit,
    Operator,
    Accent,
    Equals,
}

struct Calculator;

impl App for Calculator {
    fn view(&mut self, ui: &mut Ui) {
        let theme = ui.theme().clone();
        let calc = ui.state(Calc::default);
        let history = ui.state(Vec::<String>::new);

        // Physical keyboard. `Key::Character` gives the physical character,
        // independent of the keyboard layout or IME text.
        for d in '0'..='9' {
            if ui.key_pressed(&Key::Character(d.to_string())) {
                calc.update(|state| state.digit(d));
            }
        }
        for (character, op) in [
            ("+", Op::Add),
            ("-", Op::Sub),
            ("*", Op::Mul),
            ("/", Op::Div),
        ] {
            if ui.key_pressed(&Key::Character(character.to_owned())) {
                calc.update(|state| state.operate(op));
            }
        }
        if ui.key_pressed(&Key::Character(".".to_owned())) {
            calc.update(|state| state.dot());
        }
        if ui.key_pressed(&Key::Character("%".to_owned())) {
            calc.update(|state| state.percent());
        }
        if ui.key_pressed(&Key::Enter) || ui.key_pressed(&Key::Character("=".to_owned())) {
            let snapshot = calc.get();
            calc.update(|state| state.equals());
            history.update(|lines| {
                lines.insert(0, format!("{} → {}", snapshot.display, calc.get().display));
                lines.truncate(4);
            });
        }
        if ui.key_pressed(&Key::Backspace) {
            calc.update(|state| state.backspace());
        }
        if ui.key_pressed(&Key::Escape) {
            calc.update(|state| state.clear());
        }
        if ui.key_pressed(&Key::Delete) {
            calc.update(|state| state.clear());
        }

        let state = calc.get();

        ui.size_full().background(theme.colors.background);
        ui.column(|ui| {
            ui.size_full()
                .padding(24.0)
                .gap(14.0)
                .align_items(Align::Center);

            ui.row(|ui| {
                ui.width_full().align_items(Align::End);
                ui.column(|ui| {
                    ui.gap(2.0);
                    ui.add(
                        Label::new("CALCULATOR")
                            .font_size(10.0)
                            .color(theme.colors.primary)
                            .family(FontFamily::Monospace),
                    );
                    ui.add(
                        Label::new("one view function, one State<Calc>")
                            .font_size(11.0)
                            .color(theme.colors.on_surface_variant),
                    );
                });
                ui.spacer();
                micro(ui, "digits · + − × ÷ · Enter · Backspace · Esc");
            });

            // Display
            ui.column(|ui| {
                ui.width_full()
                    .min_width(320.0)
                    .max_width(440.0)
                    .padding_edges(Edges {
                        left: 18.0,
                        right: 18.0,
                        top: 14.0,
                        bottom: 14.0,
                    })
                    .gap(4.0)
                    .align_items(Align::End)
                    .background(theme.colors.surface_container_low)
                    .border(1.0, theme.colors.outline_variant)
                    .radius(theme.shape.medium);
                ui.row(|ui| {
                    ui.width_full().justify(Justify::End);
                    micro(
                        ui,
                        match (state.pending, state.operator) {
                            (Some(left), Some(op)) => {
                                format!("{left} {}", op.symbol())
                            }
                            _ => " ".to_owned(),
                        },
                    );
                });
                ui.add(
                    Label::new(&state.display)
                        .font_size(38.0)
                        .color(if state.error.is_some() {
                            theme.colors.error
                        } else {
                            theme.colors.on_surface
                        })
                        .weight(FontWeight::SEMIBOLD)
                        .align(TextAlign::End)
                        .family(FontFamily::Monospace)
                        .wrap(TextWrap::None),
                );
                if let Some(error) = &state.error {
                    ui.add(Label::new(error).font_size(11.0).color(theme.colors.error));
                }
            });

            // Keypad: a 4-column grid. The framework has no special calculator
            // widget — a grid of clickable rows is enough.
            ui.column(|ui| {
                ui.width_full().max_width(440.0);
                let grid: Vec<(&str, KeyKind, CalcCommand)> = vec![
                    ("C", KeyKind::Accent, CalcCommand::Clear),
                    ("±", KeyKind::Accent, CalcCommand::Negate),
                    ("%", KeyKind::Accent, CalcCommand::Percent),
                    ("÷", KeyKind::Operator, CalcCommand::Op(Op::Div)),
                    ("7", KeyKind::Digit, CalcCommand::Digit('7')),
                    ("8", KeyKind::Digit, CalcCommand::Digit('8')),
                    ("9", KeyKind::Digit, CalcCommand::Digit('9')),
                    ("×", KeyKind::Operator, CalcCommand::Op(Op::Mul)),
                    ("4", KeyKind::Digit, CalcCommand::Digit('4')),
                    ("5", KeyKind::Digit, CalcCommand::Digit('5')),
                    ("6", KeyKind::Digit, CalcCommand::Digit('6')),
                    ("−", KeyKind::Operator, CalcCommand::Op(Op::Sub)),
                    ("1", KeyKind::Digit, CalcCommand::Digit('1')),
                    ("2", KeyKind::Digit, CalcCommand::Digit('2')),
                    ("3", KeyKind::Digit, CalcCommand::Digit('3')),
                    ("+", KeyKind::Operator, CalcCommand::Op(Op::Add)),
                    ("⌫", KeyKind::Accent, CalcCommand::Backspace),
                    ("0", KeyKind::Digit, CalcCommand::Digit('0')),
                    (".", KeyKind::Digit, CalcCommand::Dot),
                    ("=", KeyKind::Equals, CalcCommand::Equals),
                ];
                ui.grid(4, |ui| {
                    ui.width_full().gap(8.0);
                    for (label, kind, command) in grid {
                        let response = key(ui, label, kind);
                        if response.clicked() {
                            let before = calc.get();
                            calc.update(|state| command.apply(state));
                            if matches!(command, CalcCommand::Equals) {
                                history.update(|lines| {
                                    lines.insert(
                                        0,
                                        format!("{} → {}", before.display, calc.get().display),
                                    );
                                    lines.truncate(4);
                                });
                            }
                        }
                    }
                });
            });

            ui.column(|ui| {
                ui.width_full().max_width(440.0).gap(3.0);
                heading(ui, "History", "State<Vec<String>>");
                let lines = history.get();
                if lines.is_empty() {
                    note(ui, "Press = to record a calculation.");
                }
                for line in lines {
                    code(ui, line);
                }
            });

            ui.spacer();
            note(
                ui,
                "The display, pending operand and operator live in one `State<Calc>`. \
                 Every key — on screen or physical — applies the same pure command.",
            );
        });
    }
}

#[derive(Clone, Copy)]
enum CalcCommand {
    Digit(char),
    Dot,
    Clear,
    Negate,
    Percent,
    Backspace,
    Op(Op),
    Equals,
}

impl CalcCommand {
    fn apply(self, calc: &mut Calc) {
        match self {
            CalcCommand::Digit(d) => calc.digit(d),
            CalcCommand::Dot => calc.dot(),
            CalcCommand::Clear => calc.clear(),
            CalcCommand::Negate => calc.negate(),
            CalcCommand::Percent => calc.percent(),
            CalcCommand::Backspace => calc.backspace(),
            CalcCommand::Op(op) => calc.operate(op),
            CalcCommand::Equals => calc.equals(),
        }
    }
}

fn main() -> anyhow::Result<()> {
    run(Calculator, options("Kasumi — calculator", 560.0, 720.0))
}
