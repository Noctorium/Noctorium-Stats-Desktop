//! Signing in, or making an account: the same account Noctorium itself signs in to, whose plays are what
//! the dashboard counts.

use super::theme::{self, ERROR, FAINT, SUBTEXT, TEXT, WARNING};
use super::{Action, App, Mode, SignInForm};
use eframe::egui::{self, Align, Key, Layout, Margin, RichText, Ui};

/// What the service would refuse anyway, said before asking it.
pub fn check(form: &SignInForm) -> Result<(), String> {
    let email = form.email.trim();
    if email.is_empty() {
        return Err("Enter your email address.".into());
    }
    let looks_like_one = email.split_once('@').is_some_and(|(name, domain)| {
        !name.is_empty()
            && domain.contains('.')
            && !domain.starts_with('.')
            && !domain.ends_with('.')
    });
    if !looks_like_one || email.contains(char::is_whitespace) {
        return Err("That does not look like an email address.".into());
    }
    if form.password.is_empty() {
        return Err("Enter your password.".into());
    }
    // The service's own floor, so a short one is caught here rather than after a round trip.
    if form.mode == Mode::Create && form.password.chars().count() < 10 {
        return Err("Use at least 10 characters for the password.".into());
    }
    Ok(())
}

pub fn show(ui: &mut Ui, app: &mut App, actions: &mut Vec<Action>) {
    let full = ui.max_rect();
    theme::glow(
        ui.painter(),
        egui::pos2(full.center().x, full.top() + full.height() * 0.22),
        egui::vec2(full.width().min(900.0) * 0.7, full.height() * 0.45),
        0.22,
    );

    let mark = app.mark().id();
    let service = app.service.clone();
    let newer = app.newer.clone();
    let Some(form) = app.form_mut() else { return };

    // Centred in the window when it fits, from the top when it does not: the height it took last frame
    // decides, which settles on the first frame or two.
    let height_id = ui.id().with("sign-in-height");
    let last_height: f32 = ui.data(|d| d.get_temp(height_id)).unwrap_or(620.0);

    egui::ScrollArea::vertical()
        .auto_shrink(false)
        .show(ui, |ui| {
            let available = ui.available_height();
            let top = ui.cursor().top();
            ui.add_space(((available - last_height) / 2.0).max(28.0));
            let width = 404.0f32.min(ui.available_width() - 32.0);

            ui.vertical_centered(|ui| {
                ui.add(egui::Image::new((mark, egui::vec2(76.0, 76.0))));
                ui.add_space(14.0);
                ui.label(
                    RichText::new("Noctorium Stats")
                        .font(theme::bold(28.0))
                        .color(TEXT),
                );
                ui.add_space(2.0);
                ui.label(
                    RichText::new("Your listening, counted.")
                        .font(theme::regular(15.0))
                        .color(SUBTEXT),
                );
                ui.add_space(24.0);
            });

            ui.horizontal(|ui| {
                ui.add_space(((ui.available_width() - width) / 2.0).max(0.0));
                ui.vertical(|ui| {
                    ui.set_width(width);
                    theme::card()
                        .inner_margin(Margin::same(24))
                        .shadow(egui::epaint::Shadow {
                            offset: [0, 10],
                            blur: 40,
                            spread: 0,
                            color: egui::Color32::from_black_alpha(180),
                        })
                        .show(ui, |ui| {
                            ui.set_width(width - 50.0);
                            form_body(ui, form, actions);
                        });
                });
            });

            ui.add_space(18.0);
            ui.vertical_centered(|ui| {
                ui.label(
                    RichText::new("Use the same account you sign in to inside Noctorium.")
                        .font(theme::regular(13.0))
                        .color(FAINT),
                );
                ui.add_space(2.0);
                theme::link(ui, "Open the Noctorium website", &service);
                ui.add_space(26.0);
                about(ui, &service, newer.as_ref());
            });
            ui.add_space(20.0);

            let used = ui.cursor().top() - top - ((available - last_height) / 2.0).max(28.0);
            ui.data_mut(|d| d.insert_temp(height_id, used));
        });
}

fn form_body(ui: &mut Ui, form: &mut SignInForm, actions: &mut Vec<Action>) {
    let inner = ui.available_width();
    let chosen = if form.mode == Mode::SignIn { 0 } else { 1 };
    if let Some(index) = theme::segmented(
        ui,
        &["Sign in", "Create account"],
        chosen,
        (inner - 8.0) / 2.0,
    ) {
        actions.push(Action::SwitchMode(if index == 0 {
            Mode::SignIn
        } else {
            Mode::Create
        }));
    }
    ui.add_space(18.0);

    if let Some(notice) = &form.notice {
        egui::Frame::new()
            .fill(WARNING.gamma_multiply(0.12))
            .stroke(theme::stroke(1.0, WARNING.gamma_multiply(0.4)))
            .corner_radius(10)
            .inner_margin(Margin::symmetric(12, 10))
            .show(ui, |ui| {
                ui.set_width(ui.available_width());
                ui.label(RichText::new(notice).font(theme::regular(13.0)).color(TEXT));
            });
        ui.add_space(14.0);
    }

    let mut submit = false;
    let enabled = !form.busy;

    let email = field(
        ui,
        "Email",
        None,
        &mut form.email,
        "you@example.com",
        false,
        enabled,
    );
    submit |= entered(ui, &email);
    let (password_hint, password_note) = match form.mode {
        Mode::SignIn => ("Your password", None),
        Mode::Create => ("At least 10 characters", None),
    };
    let password = field(
        ui,
        "Password",
        password_note,
        &mut form.password,
        password_hint,
        true,
        enabled,
    );
    submit |= entered(ui, &password);
    if form.mode == Mode::Create {
        let name = field(
            ui,
            "Name",
            Some("optional"),
            &mut form.name,
            "How the dashboard should greet you",
            false,
            enabled,
        );
        submit |= entered(ui, &name);
    }

    // The first field still empty gets the cursor, so the form can be filled in without the mouse.
    let focus_id = ui.id().with("focused-once");
    if !ui.data(|d| d.get_temp::<bool>(focus_id).unwrap_or(false)) {
        ui.data_mut(|d| d.insert_temp(focus_id, true));
        if form.email.trim().is_empty() {
            email.request_focus();
        } else {
            password.request_focus();
        }
    }

    ui.add_space(10.0);
    let label = match (form.mode, form.busy) {
        (Mode::SignIn, false) => "Sign in",
        (Mode::SignIn, true) => "Signing in…",
        (Mode::Create, false) => "Create account",
        (Mode::Create, true) => "Creating your account…",
    };
    // Left looking enabled while busy, so "Signing in…" can be read; a second press is ignored anyway.
    let button = theme::primary(ui, label, ui.available_width(), true);
    if form.busy {
        let spinner = egui::Rect::from_center_size(
            button.rect.right_center() - egui::vec2(24.0, 0.0),
            egui::vec2(16.0, 16.0),
        );
        ui.put(
            spinner,
            egui::Spinner::new().size(16.0).color(theme::ON_ACCENT),
        );
    }
    if button.clicked() || submit {
        actions.push(Action::Submit);
    }

    if let Some(problem) = &form.problem {
        ui.add_space(12.0);
        ui.horizontal_wrapped(|ui| {
            ui.spacing_mut().item_spacing.x = 6.0;
            ui.label(RichText::new("⚠").font(theme::regular(14.0)).color(ERROR));
            ui.label(
                RichText::new(problem)
                    .font(theme::regular(13.5))
                    .color(ERROR),
            );
        });
    }

    if form.mode == Mode::Create {
        ui.add_space(12.0);
        ui.label(
            RichText::new(
                "The account is the one Noctorium signs in to: its plays are what this counts.",
            )
            .font(theme::regular(12.5))
            .color(FAINT),
        );
    }
}

/// A labelled text field. Returns its response, for the caller to read Enter from.
fn field(
    ui: &mut Ui,
    label: &str,
    note: Option<&str>,
    text: &mut String,
    hint: &str,
    password: bool,
    enabled: bool,
) -> egui::Response {
    let label = ui
        .horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = 6.0;
            let label = ui.label(
                RichText::new(label)
                    .font(theme::semibold(13.0))
                    .color(SUBTEXT),
            );
            if let Some(note) = note {
                ui.label(RichText::new(note).font(theme::regular(12.0)).color(FAINT));
            }
            label
        })
        .inner;
    ui.add_space(2.0);
    let edit = egui::TextEdit::singleline(text)
        .hint_text(RichText::new(hint).color(FAINT))
        .password(password)
        .font(theme::regular(15.0))
        .text_color(TEXT)
        .margin(Margin::symmetric(12, 10))
        .desired_width(f32::INFINITY);
    let response = ui.add_enabled(enabled, edit).labelled_by(label.id);
    ui.add_space(12.0);
    response
}

fn entered(ui: &Ui, response: &egui::Response) -> bool {
    response.lost_focus() && ui.input(|i| i.key_pressed(Key::Enter))
}

/// The version line, and a word about a newer one when there is one.
pub fn about(ui: &mut Ui, service: &str, newer: Option<&crate::update::Newer>) {
    let host = service.split("://").nth(1).unwrap_or(service);
    let mut line = format!("Noctorium Stats {}", crate::version());
    if service != crate::api::LIVE {
        line.push_str(&format!(" · {host}"));
    }
    ui.label(RichText::new(line).font(theme::regular(12.0)).color(FAINT));
    if let Some(newer) = newer {
        ui.add_space(4.0);
        ui.with_layout(Layout::top_down(Align::Center), |ui| {
            theme::link(
                ui,
                &format!(
                    "Noctorium Stats {} is out — get it from the release page",
                    newer.version
                ),
                &newer.url,
            );
        });
    }
}
