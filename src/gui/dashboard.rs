//! The dashboard: a range, four figures, and the charts and lists beneath them.
//!
//! Laid out in one column that is never wider than reads well, with the cards paired side by side when
//! there is room and stacked when there is not, so the same page works from a small laptop window to a wide
//! monitor.

use super::sign_in::about;
use super::theme::{self, ACCENT, CARD, ERROR, FAINT, GAP, LINE, PANEL, SUBTEXT, TEXT, WARNING};
use super::{charts, Action, App, Board, Screen, View};
use crate::api::{Problem, User};
use crate::format::{self, Zone};
use crate::stats::{Range, Stats};
use chrono::{DateTime, Utc};
use eframe::egui::{
    self, text::LayoutJob, Align, Align2, Color32, CornerRadius, FontId, Layout, Margin, Rect,
    RichText, Sense, TextFormat, Ui,
};

/// Wider than this and lines get too long to read; the page sits in the middle instead.
const MAX_WIDTH: f32 = 1240.0;
/// Narrower than this, paired cards are stacked.
const PAIRS_FROM: f32 = 700.0;

/// What the body of the page needs, borrowed from the app for one frame.
struct Page<'a> {
    board: &'a Board,
    now: DateTime<Utc>,
    zone: Zone,
}

pub fn show(ui: &mut Ui, app: &mut App, actions: &mut Vec<Action>) {
    let mark = app.mark().id();
    let (now, zone) = (app.now(), app.zone);
    let service = app.service.clone();
    let newer = app.newer.clone();
    let Screen::Board(board) = &app.screen else {
        return;
    };
    let page = Page { board, now, zone };

    top_bar(ui, &page, mark, actions);

    egui::ScrollArea::vertical()
        .auto_shrink(false)
        .show(ui, |ui| {
            let full = ui.available_width();
            let side = if full < 760.0 { 16.0 } else { 28.0 };
            let width = (full - side * 2.0).min(MAX_WIDTH);
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = 0.0;
                ui.add_space((full - width) / 2.0);
                ui.vertical(|ui| {
                    ui.set_width(width);
                    // The row this column sits in has no spacing of its own, which everything in it would inherit.
                    ui.spacing_mut().item_spacing = egui::vec2(8.0, GAP);
                    ui.add_space(26.0 - GAP);
                    body(ui, &page, actions);
                    ui.add_space(18.0);
                    ui.vertical_centered(|ui| {
                        ui.spacing_mut().item_spacing.y = 6.0;
                        if let Some(note) = &page.board.kept_note {
                            ui.label(RichText::new(note).font(theme::regular(12.0)).color(FAINT));
                        }
                        about(ui, &service, newer.as_ref());
                    });
                    ui.add_space(24.0);
                });
            });
        });
}

fn top_bar(ui: &mut Ui, page: &Page, mark: egui::TextureId, actions: &mut Vec<Action>) {
    let board = page.board;
    egui::Frame::new()
        .fill(PANEL)
        .inner_margin(Margin::symmetric(20, 12))
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.horizontal(|ui| {
                ui.set_height(36.0);
                ui.add(egui::Image::new((mark, egui::vec2(32.0, 32.0))));
                ui.add_space(4.0);
                ui.label(
                    RichText::new("Noctorium Stats")
                        .font(theme::semibold(17.0))
                        .color(TEXT),
                );
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    ui.spacing_mut().item_spacing.x = 10.0;
                    if theme::secondary(ui, "Sign out").clicked() {
                        actions.push(Action::SignOut);
                    }
                    let busy = board.pending.contains(&board.range);
                    let refresh = theme::secondary(
                        ui,
                        if busy {
                            "    Refreshing…"
                        } else {
                            "⟳  Refresh"
                        },
                    )
                    .on_hover_text("Fetch the figures again (F5)");
                    if busy {
                        let spot = egui::Rect::from_center_size(
                            refresh.rect.left_center() + egui::vec2(20.0, 0.0),
                            egui::vec2(14.0, 14.0),
                        );
                        ui.put(spot, egui::Spinner::new().size(14.0).color(ACCENT));
                    } else if refresh.clicked() {
                        actions.push(Action::Refresh);
                    }
                    if let Some(user) = &board.user {
                        ui.add_space(6.0);
                        who(ui, user);
                    }
                });
            });
        });
    // A hairline under the bar, the full width of the window.
    let rect = ui.min_rect();
    ui.painter()
        .hline(rect.x_range(), ui.cursor().top(), theme::stroke(1.0, LINE));
}

/// The listener's name with a disc holding its first letter, in the top bar.
fn who(ui: &mut Ui, user: &User) {
    let name = if user.display_name.trim().is_empty() {
        &user.email
    } else {
        &user.display_name
    };
    // Truncated rather than wrapped: a long name should not push the buttons off the bar.
    ui.add(
        egui::Label::new(
            RichText::new(name)
                .font(theme::regular(14.0))
                .color(SUBTEXT),
        )
        .truncate(),
    );
    let (rect, _) = ui.allocate_exact_size(egui::vec2(30.0, 30.0), Sense::hover());
    ui.painter()
        .circle_filled(rect.center(), 15.0, theme::accent_wash(0.25));
    let initial = name
        .chars()
        .next()
        .map(|c| c.to_uppercase().collect::<String>())
        .unwrap_or_default();
    ui.painter().text(
        rect.center(),
        Align2::CENTER_CENTER,
        initial,
        theme::semibold(14.0),
        theme::ACCENT_BRIGHT,
    );
}

fn body(ui: &mut Ui, page: &Page, actions: &mut Vec<Action>) {
    let board = page.board;
    let view = board.views.get(&board.range);
    heading(ui, page, view, actions);

    let pending = board.pending.contains(&board.range);
    let problem = board.problems.get(&board.range);
    match view {
        Some(view) => {
            if let Some(problem) = problem {
                banner(ui, problem, view, page, actions);
            }
            if view.stats.never_played() {
                never_played(ui, actions);
            } else if view.stats.is_empty() {
                figures(ui, &view.stats, board.range, page);
                quiet_range(ui, board.range, actions);
            } else {
                dashboard(ui, &view.stats, board.range, page);
            }
        }
        None => match problem {
            Some(problem) if !pending => trouble(ui, problem, actions),
            _ => loading(ui),
        },
    }
}

/// "Your listening", the days it covers, and the range switcher.
fn heading(ui: &mut Ui, page: &Page, view: Option<&View>, actions: &mut Vec<Action>) {
    let board = page.board;
    let width = ui.available_width();
    let switcher_width = 5.0 * 80.0 + 8.0;
    let beside = width >= switcher_width + 300.0;

    let mut detail = String::new();
    if let Some(view) = view {
        detail = format::span(view.stats.from, view.stats.first_play, page.now, page.zone);
        let updated = if board.pending.contains(&board.range) {
            "Refreshing…".to_string()
        } else {
            format::updated(view.fetched_at, page.now, page.zone)
        };
        detail = format!("{detail}  ·  {updated}");
    }

    let titles = |ui: &mut Ui| {
        ui.vertical(|ui| {
            ui.spacing_mut().item_spacing.y = 4.0;
            ui.label(
                RichText::new("Your listening")
                    .font(theme::bold(28.0))
                    .color(TEXT),
            );
            ui.label(
                RichText::new(if detail.is_empty() { " " } else { &detail })
                    .font(theme::regular(13.5))
                    .color(FAINT),
            );
        });
    };
    let switcher = |ui: &mut Ui, actions: &mut Vec<Action>| {
        let labels: Vec<&str> = Range::ALL.iter().map(|r| r.label()).collect();
        let chosen = Range::ALL
            .iter()
            .position(|r| *r == board.range)
            .unwrap_or(0);
        let segment = if beside {
            80.0
        } else {
            ((ui.available_width() - 8.0) / 5.0).min(80.0)
        };
        if let Some(index) = theme::segmented(ui, &labels, chosen, segment) {
            actions.push(Action::Range(Range::ALL[index]));
        }
    };

    if beside {
        ui.horizontal(|ui| {
            titles(ui);
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                switcher(ui, actions)
            });
        });
    } else {
        titles(ui);
        switcher(ui, actions);
    }
    ui.add_space(4.0);
}

// ------------------------------------------------------------------------------------------------ states

fn loading(ui: &mut Ui) {
    // The shape of what is coming, so the page does not jump when it arrives.
    let width = ui.available_width();
    let columns = if width >= 600.0 { 4 } else { 2 };
    let w = (width - GAP * (columns - 1) as f32) / columns as f32;
    for row in 0..(4 / columns) {
        ui.horizontal_top(|ui| {
            ui.spacing_mut().item_spacing.x = GAP;
            for _ in 0..columns {
                let (rect, _) = ui.allocate_exact_size(egui::vec2(w, 118.0), Sense::hover());
                ui.painter().rect_filled(
                    rect,
                    CornerRadius::same(theme::RADIUS),
                    CARD.gamma_multiply(0.6),
                );
            }
        });
        let _ = row;
    }
    theme::card().show(ui, |ui| {
        ui.set_width(ui.available_width());
        ui.set_min_height(240.0);
        ui.vertical_centered(|ui| {
            ui.add_space(70.0);
            ui.add(egui::Spinner::new().size(28.0).color(ACCENT));
            ui.add_space(12.0);
            ui.label(
                RichText::new("Counting your listening…")
                    .font(theme::regular(15.0))
                    .color(SUBTEXT),
            );
        });
    });
}

/// Nothing to show and something in the way of fetching it.
fn trouble(ui: &mut Ui, problem: &Problem, actions: &mut Vec<Action>) {
    let (title, colour) = match problem {
        Problem::Offline => ("You seem to be offline", WARNING),
        Problem::Throttled(_) => ("Too many requests", WARNING),
        _ => ("The figures could not be fetched", ERROR),
    };
    theme::card().show(ui, |ui| {
        ui.set_width(ui.available_width());
        ui.vertical_centered(|ui| {
            ui.add_space(40.0);
            ui.label(
                RichText::new(if colour == WARNING { "🔌" } else { "⚠" })
                    .font(theme::regular(34.0))
                    .color(colour),
            );
            ui.add_space(8.0);
            ui.label(RichText::new(title).font(theme::semibold(19.0)).color(TEXT));
            ui.add_space(4.0);
            ui.add(
                egui::Label::new(
                    RichText::new(problem.message())
                        .font(theme::regular(14.0))
                        .color(SUBTEXT),
                )
                .wrap(),
            );
            ui.add_space(16.0);
            if theme::primary(ui, "Try again", 160.0, true).clicked() {
                actions.push(Action::Refresh);
            }
            ui.add_space(36.0);
        });
    });
}

/// Figures are on the page, and something went wrong fetching newer ones: said in a strip above them.
fn banner(ui: &mut Ui, problem: &Problem, view: &View, page: &Page, actions: &mut Vec<Action>) {
    let (colour, text) = match problem {
        Problem::Offline => (
            WARNING,
            format!(
                "Offline. These are the figures from {}.",
                format::updated(view.fetched_at, page.now, page.zone)
                    .trim_start_matches("Updated ")
                    .trim_start_matches("at ")
            ),
        ),
        other => (ERROR, format!("Could not refresh: {}", other.message())),
    };
    egui::Frame::new()
        .fill(colour.gamma_multiply(0.1))
        .stroke(theme::stroke(1.0, colour.gamma_multiply(0.35)))
        .corner_radius(12)
        .inner_margin(Margin::symmetric(16, 10))
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.horizontal(|ui| {
                ui.label(
                    RichText::new(if colour == WARNING { "🔌" } else { "⚠" })
                        .font(theme::regular(15.0))
                        .color(colour),
                );
                ui.add(
                    egui::Label::new(RichText::new(text).font(theme::regular(13.5)).color(TEXT))
                        .truncate(),
                );
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    if theme::secondary(ui, "Try again").clicked() {
                        actions.push(Action::Refresh);
                    }
                });
            });
        });
}

/// An account with nothing played, ever: what to do about it.
fn never_played(ui: &mut Ui, actions: &mut Vec<Action>) {
    theme::card().show(ui, |ui| {
        ui.set_width(ui.available_width());
        let rect = ui.max_rect();
        theme::glow(ui.painter(), egui::pos2(rect.center().x, rect.top() + 70.0), egui::vec2(260.0, 120.0), 0.18);
        ui.vertical_centered(|ui| {
            ui.add_space(36.0);
            ui.label(RichText::new("♫").font(theme::regular(44.0)).color(theme::ACCENT_BRIGHT));
            ui.add_space(6.0);
            ui.label(RichText::new("Nothing played yet").font(theme::bold(24.0)).color(TEXT));
            ui.add_space(6.0);
            ui.label(
                RichText::new("Your statistics fill in as you listen. Every song Noctorium plays while you are signed in is counted here, whichever service it comes from.")
                    .font(theme::regular(14.5))
                    .color(SUBTEXT),
            );
            ui.add_space(22.0);
        });
        let steps = [
            ("1", "Open Noctorium and sign in to this same account."),
            ("2", "Play something: YouTube Music, SoundCloud, Spotify, Bandcamp, VK, or your own files."),
            ("3", "Come back here and refresh."),
        ];
        let width = ui.available_width().min(520.0);
        ui.horizontal(|ui| {
            ui.add_space((ui.available_width() - width) / 2.0);
            ui.vertical(|ui| {
                ui.set_width(width);
                for (number, text) in steps {
                    ui.horizontal(|ui| {
                        let (rect, _) = ui.allocate_exact_size(egui::vec2(26.0, 26.0), Sense::hover());
                        ui.painter().circle_filled(rect.center(), 13.0, theme::accent_wash(0.22));
                        ui.painter().text(rect.center(), Align2::CENTER_CENTER, number, theme::semibold(13.0), theme::ACCENT_BRIGHT);
                        ui.add_space(4.0);
                        ui.add(egui::Label::new(RichText::new(text).font(theme::regular(14.0)).color(TEXT)).wrap());
                    });
                    ui.add_space(6.0);
                }
            });
        });
        ui.add_space(16.0);
        ui.vertical_centered(|ui| {
            if theme::primary(ui, "Refresh", 160.0, true).clicked() {
                actions.push(Action::Refresh);
            }
            ui.add_space(30.0);
        });
    });
}

/// Something has been played, just not in this range.
fn quiet_range(ui: &mut Ui, range: Range, actions: &mut Vec<Action>) {
    let title = match range {
        Range::Week => "Nothing in the last 7 days",
        Range::Month => "Nothing in the last 30 days",
        Range::Quarter => "Nothing in the last 90 days",
        Range::Year => "Nothing in the last year",
        Range::All => "Nothing yet",
    };
    theme::card().show(ui, |ui| {
        ui.set_width(ui.available_width());
        ui.vertical_centered(|ui| {
            ui.add_space(30.0);
            ui.label(RichText::new(title).font(theme::semibold(19.0)).color(TEXT));
            ui.add_space(4.0);
            ui.label(
                RichText::new("You have listened before, just not lately. Choose a longer range, or play something in Noctorium and refresh.")
                    .font(theme::regular(14.0))
                    .color(SUBTEXT),
            );
            ui.add_space(14.0);
            if theme::secondary(ui, "Show all time").clicked() {
                actions.push(Action::Range(Range::All));
            }
            ui.add_space(30.0);
        });
    });
}

// --------------------------------------------------------------------------------------------- the page

fn dashboard(ui: &mut Ui, stats: &Stats, range: Range, page: &Page) {
    figures(ui, stats, range, page);

    let wide = ui.available_width() >= PAIRS_FROM;

    theme::card().show(ui, |ui| {
        ui.set_width(ui.available_width());
        let (title, busiest) = match stats.timeline.bucket {
            crate::stats::Bucket::Day => ("Plays each day", "Busiest day"),
            crate::stats::Bucket::Month => ("Plays each month", "Busiest month"),
        };
        let peak = stats
            .timeline
            .points
            .iter()
            .filter(|p| p.streams > 0)
            .max_by_key(|p| (p.streams, p.start));
        let detail = peak.map(|p| {
            let when = match stats.timeline.bucket {
                crate::stats::Bucket::Day => format::long_day(p.start),
                crate::stats::Bucket::Month => format::long_month(p.start),
            };
            format!("{busiest}: {when}, {}", format::plays(p.streams))
        });
        theme::card_title(ui, title, detail.as_deref());
        charts::timeline(ui, &stats.timeline, 190.0);
    });

    pair(
        ui,
        wide,
        "services-streak",
        |ui| {
            theme::card_title(ui, "Where you listened", None);
            charts::services(ui, &stats.services());
        },
        |ui| streak(ui, stats),
    );

    pair(
        ui,
        wide,
        "top-lists",
        |ui| {
            theme::card_title(ui, "Top songs", None);
            top_songs(ui, stats);
        },
        |ui| {
            theme::card_title(ui, "Top artists", None);
            top_artists(ui, stats);
        },
    );

    let hours = stats.hours_of_day();
    let days = stats.days_of_week();
    pair(
        ui,
        wide,
        "clock-week",
        |ui| {
            let peak = hours
                .iter()
                .filter(|h| h.streams > 0)
                .max_by_key(|h| (h.streams, std::cmp::Reverse(h.hour)));
            let detail = peak.map(|h| format!("Most at {}", format::hour_span(h.hour)));
            theme::card_title(ui, "Hours of the day", detail.as_deref());
            charts::clock(ui, &hours, 300.0);
        },
        |ui| {
            let peak = days
                .iter()
                .filter(|d| d.streams > 0)
                .max_by_key(|d| (d.streams, std::cmp::Reverse(d.day)));
            let detail = peak.map(|d| format!("Most on {}s", format::weekday(d.day)));
            theme::card_title(ui, "Days of the week", detail.as_deref());
            charts::week(ui, &days, 300.0);
        },
    );

    theme::card().show(ui, |ui| {
        ui.set_width(ui.available_width());
        let detail = (!stats.recent.is_empty())
            .then(|| format!("The last {}", format::plays(stats.recent.len() as u64)));
        theme::card_title(ui, "Recently played", detail.as_deref());
        recent(ui, stats, page);
    });
}

/// The four big figures.
fn figures(ui: &mut Ui, stats: &Stats, range: Range, page: &Page) {
    let days = match range.days() {
        Some(days) => days,
        // All of it: from the first play to today, on the listener's calendar.
        None => stats
            .first_play
            .map(|first| {
                (page.zone.today(page.now) - page.zone.local_time(first).date()).num_days() + 1
            })
            .unwrap_or(1),
    }
    .max(1) as f64;

    let per_day = stats.streams as f64 / days;
    let a_day = if per_day >= 10.0 {
        format!("About {} a day", format::count(per_day.round() as u64))
    } else if per_day >= 1.0 {
        format!("About {:.1} a day", per_day)
    } else if stats.streams > 0 {
        format!("About {} a week", ((per_day * 7.0).round() as u64).max(1))
    } else {
        "None in this range".into()
    };
    let per_song = if stats.unique_tracks > 0 {
        format!(
            "{:.1} plays a song",
            stats.streams as f64 / stats.unique_tracks as f64
        )
    } else {
        "—".into()
    };
    let top_artist = stats
        .top_artists
        .first()
        .map(|a| format!("Most of all, {}", a.artist))
        .unwrap_or_else(|| "—".into());
    let minutes_a_day = stats.ms_played as f64 / 60_000.0 / days;
    let listening = if stats.ms_played == 0 {
        "—".to_string()
    } else if minutes_a_day >= 1.0 {
        format!("{} a day", format::minutes(minutes_a_day.round() as u64))
    } else {
        "Under a minute a day".to_string()
    };

    let items = [
        ("Songs streamed", format::count(stats.streams), a_day),
        (
            "Different songs",
            format::count(stats.unique_tracks),
            per_song,
        ),
        ("Artists", format::count(stats.artists), top_artist),
        ("Hours listened", format::hours(stats.ms_played), listening),
    ];

    let width = ui.available_width();
    let columns = if width >= 600.0 { 4 } else { 2 };
    let w = (width - GAP * (columns - 1) as f32) / columns as f32;
    for row in items.chunks(columns) {
        ui.horizontal_top(|ui| {
            ui.spacing_mut().item_spacing.x = GAP;
            for (label, value, detail) in row {
                ui.allocate_ui_with_layout(
                    egui::vec2(w, 0.0),
                    Layout::top_down(Align::Min),
                    |ui| {
                        theme::card()
                            .inner_margin(Margin::symmetric(20, 18))
                            .show(ui, |ui| {
                                ui.set_width(w - 42.0);
                                ui.spacing_mut().item_spacing.y = 2.0;
                                ui.label(
                                    RichText::new(*label)
                                        .font(theme::regular(13.5))
                                        .color(SUBTEXT),
                                );
                                ui.label(RichText::new(value).font(theme::bold(36.0)).color(TEXT));
                                ui.add(
                                    egui::Label::new(
                                        RichText::new(detail)
                                            .font(theme::regular(12.5))
                                            .color(FAINT),
                                    )
                                    .truncate(),
                                );
                            });
                    },
                );
            }
        });
    }
}

/// Two cards side by side, as tall as each other, or one above the other when the page is narrow. The
/// height is the taller of the two as drawn last frame, which settles on the second.
fn pair(
    ui: &mut Ui,
    wide: bool,
    id: &str,
    left: impl FnOnce(&mut Ui),
    right: impl FnOnce(&mut Ui),
) {
    if !wide {
        for add in [Box::new(left) as Box<dyn FnOnce(&mut Ui)>, Box::new(right)] {
            theme::card().show(ui, |ui| {
                ui.set_width(ui.available_width());
                add(ui);
            });
        }
        return;
    }
    let key = ui.id().with(("pair", id));
    let height: f32 = ui.data(|d| d.get_temp(key)).unwrap_or(0.0);
    let w = (ui.available_width() - GAP) / 2.0;
    let mut tallest = 0.0f32;
    ui.horizontal_top(|ui| {
        ui.spacing_mut().item_spacing.x = GAP;
        for add in [Box::new(left) as Box<dyn FnOnce(&mut Ui)>, Box::new(right)] {
            ui.allocate_ui_with_layout(egui::vec2(w, 0.0), Layout::top_down(Align::Min), |ui| {
                theme::card().show(ui, |ui| {
                    ui.set_width(w - 42.0);
                    ui.set_min_height(height);
                    let top = ui.cursor().top();
                    add(ui);
                    tallest = tallest.max(ui.cursor().top() - top);
                });
            });
        }
    });
    ui.data_mut(|d| d.insert_temp(key, tallest));
}

fn streak(ui: &mut Ui, stats: &Stats) {
    let streak = stats.streak;
    theme::card_title(
        ui,
        "Your streak",
        Some("Days in a row, over all your listening"),
    );
    ui.add_space(6.0);
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = 10.0;
        ui.with_layout(Layout::left_to_right(Align::Max), |ui| {
            // Lit while there is a streak to keep, grey when there is none.
            let flame = if streak.current > 0 {
                theme::ACCENT_BRIGHT
            } else {
                FAINT
            };
            ui.label(RichText::new("🔥").font(theme::regular(40.0)).color(flame));
            ui.label(
                RichText::new(format::count(streak.current))
                    .font(theme::bold(56.0))
                    .color(TEXT),
            );
            ui.label(
                RichText::new(if streak.current == 1 { "day" } else { "days" })
                    .font(theme::regular(20.0))
                    .color(SUBTEXT),
            );
            ui.add_space(4.0);
        });
    });
    let words = match (streak.current, streak.longest) {
        (0, _) => "No streak running. Play something today to start one.".to_string(),
        (c, l) if c >= l => "Your longest yet. Keep it going.".to_string(),
        (c, l) => format!("{} more to beat your best.", format::days(l - c + 1)),
    };
    ui.label(
        RichText::new(words)
            .font(theme::regular(14.0))
            .color(SUBTEXT),
    );
    ui.add_space(18.0);

    // How far along the best run the current one is.
    let width = ui.available_width();
    let (rect, _) = ui.allocate_exact_size(egui::vec2(width, 10.0), Sense::hover());
    ui.painter().rect_filled(
        rect,
        CornerRadius::same(5),
        Color32::from_rgb(0x1D, 0x16, 0x26),
    );
    if streak.longest > 0 && streak.current > 0 {
        let share = (streak.current as f32 / streak.longest as f32).min(1.0);
        let filled = Rect::from_min_size(
            rect.min,
            egui::vec2((width * share).max(10.0), rect.height()),
        );
        ui.painter()
            .rect_filled(filled, CornerRadius::same(5), ACCENT);
    }
    ui.add_space(10.0);
    ui.horizontal(|ui| {
        ui.label(
            RichText::new("Longest")
                .font(theme::regular(13.0))
                .color(FAINT),
        );
        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
            ui.label(
                RichText::new(format::days(streak.longest))
                    .font(theme::semibold(14.0))
                    .color(TEXT),
            );
        });
    });
}

/// One line of text, cut short with an ellipsis rather than wrapped when it does not fit.
fn one_line(
    ui: &Ui,
    text: &str,
    font: FontId,
    colour: Color32,
    width: f32,
) -> std::sync::Arc<egui::Galley> {
    let mut job = LayoutJob::single_section(text.to_string(), TextFormat::simple(font, colour));
    job.wrap = egui::text::TextWrapping::truncate_at_width(width.max(10.0));
    ui.fonts(|f| f.layout_job(job))
}

/// A row of a top list: its rank, two lines on the left, two on the right, and a wash behind it as long as
/// its share of the first row's plays.
#[allow(clippy::too_many_arguments)]
fn ranked_row(
    ui: &mut Ui,
    rank: usize,
    share: f32,
    colour: Option<Color32>,
    title: &str,
    subtitle: &str,
    right: &str,
    right_below: &str,
) {
    let width = ui.available_width();
    let (rect, response) = ui.allocate_exact_size(egui::vec2(width, 46.0), Sense::hover());
    let painter = ui.painter_at(rect);
    let wash = Rect::from_min_size(
        rect.min,
        egui::vec2((width * share).max(46.0), rect.height()),
    );
    painter.rect_filled(
        wash,
        CornerRadius::same(10),
        theme::accent_wash(if response.hovered() { 0.16 } else { 0.08 }),
    );

    let mut x = rect.left() + 10.0;
    painter.text(
        egui::pos2(x + 16.0, rect.center().y),
        Align2::RIGHT_CENTER,
        rank.to_string(),
        theme::semibold(13.0),
        FAINT,
    );
    x += 28.0;
    if let Some(colour) = colour {
        painter.circle_filled(egui::pos2(x + 3.0, rect.top() + 15.0), 3.5, colour);
        x += 14.0;
    }
    let right_width = 108.0;
    let text_width = rect.right() - right_width - x - 8.0;
    let title = one_line(ui, title, theme::semibold(14.0), TEXT, text_width);
    let subtitle = one_line(ui, subtitle, theme::regular(12.5), SUBTEXT, text_width);
    // Both lines start after the dot, so the title and what is under it line up.
    painter.galley(egui::pos2(x, rect.top() + 6.0), title, TEXT);
    painter.galley(egui::pos2(x, rect.top() + 25.0), subtitle, SUBTEXT);
    painter.text(
        egui::pos2(rect.right() - 12.0, rect.top() + 7.0),
        Align2::RIGHT_TOP,
        right,
        theme::semibold(13.5),
        TEXT,
    );
    painter.text(
        egui::pos2(rect.right() - 12.0, rect.top() + 26.0),
        Align2::RIGHT_TOP,
        right_below,
        theme::regular(12.0),
        FAINT,
    );
}

fn top_songs(ui: &mut Ui, stats: &Stats) {
    let max = stats
        .top_tracks
        .first()
        .map(|t| t.streams)
        .unwrap_or(1)
        .max(1);
    ui.spacing_mut().item_spacing.y = 4.0;
    for (index, track) in stats.top_tracks.iter().enumerate() {
        ranked_row(
            ui,
            index + 1,
            track.streams as f32 / max as f32,
            Some(theme::rgb(track.provider.colour())),
            &track.title,
            &format!("{} · {}", track.artist, track.provider.name()),
            &format::plays(track.streams),
            &format::minutes(track.minutes),
        );
    }
}

fn top_artists(ui: &mut Ui, stats: &Stats) {
    let max = stats
        .top_artists
        .first()
        .map(|a| a.streams)
        .unwrap_or(1)
        .max(1);
    ui.spacing_mut().item_spacing.y = 4.0;
    for (index, artist) in stats.top_artists.iter().enumerate() {
        ranked_row(
            ui,
            index + 1,
            artist.streams as f32 / max as f32,
            None,
            &artist.artist,
            &format::songs(artist.tracks),
            &format::plays(artist.streams),
            &format::minutes(artist.minutes),
        );
    }
}

/// The last plays, newest first; in two columns, read down then across, when there is room.
fn recent(ui: &mut Ui, stats: &Stats, page: &Page) {
    let plays = &stats.recent;
    let width = ui.available_width();
    let columns = if width >= 860.0 { 2 } else { 1 };
    let per_column = plays.len().div_ceil(columns);
    let column_width = (width - 24.0 * (columns - 1) as f32) / columns as f32;
    ui.horizontal_top(|ui| {
        ui.spacing_mut().item_spacing.x = 24.0;
        for column in plays.chunks(per_column.max(1)) {
            ui.allocate_ui_with_layout(
                egui::vec2(column_width, 0.0),
                Layout::top_down(Align::Min),
                |ui| {
                    ui.spacing_mut().item_spacing.y = 0.0;
                    for (index, play) in column.iter().enumerate() {
                        let (rect, response) =
                            ui.allocate_exact_size(egui::vec2(column_width, 40.0), Sense::hover());
                        let painter = ui.painter_at(rect);
                        if response.hovered() {
                            painter.rect_filled(
                                rect,
                                CornerRadius::same(8),
                                theme::accent_wash(0.08),
                            );
                        }
                        if index > 0 {
                            painter.hline(
                                rect.x_range().shrink(8.0),
                                rect.top(),
                                theme::stroke(1.0, Color32::from_rgb(0x1F, 0x18, 0x29)),
                            );
                        }
                        painter.circle_filled(
                            egui::pos2(rect.left() + 12.0, rect.center().y),
                            4.0,
                            theme::rgb(play.provider.colour()),
                        );
                        let when = format::played_at(play.played_at, page.now, page.zone);
                        let when_galley = one_line(ui, &when, theme::regular(12.5), FAINT, 140.0);
                        let when_width = when_galley.size().x;
                        painter.galley(
                            egui::pos2(
                                rect.right() - 8.0 - when_width,
                                rect.center().y - when_galley.size().y / 2.0,
                            ),
                            when_galley,
                            FAINT,
                        );
                        let text_width = rect.width() - 28.0 - when_width - 20.0;
                        let mut job = LayoutJob::default();
                        job.append(
                            &play.title,
                            0.0,
                            TextFormat::simple(theme::regular(14.0), TEXT),
                        );
                        job.append("  ", 0.0, TextFormat::simple(theme::regular(14.0), TEXT));
                        job.append(
                            &play.artist,
                            0.0,
                            TextFormat::simple(theme::regular(13.0), FAINT),
                        );
                        job.wrap =
                            egui::text::TextWrapping::truncate_at_width(text_width.max(10.0));
                        let galley = ui.fonts(|f| f.layout_job(job));
                        painter.galley(
                            egui::pos2(rect.left() + 26.0, rect.center().y - galley.size().y / 2.0),
                            galley,
                            TEXT,
                        );
                        response.on_hover_text(format!(
                            "{} — {}\n{}, on {}",
                            play.title,
                            play.artist,
                            when,
                            play.provider.name()
                        ));
                    }
                },
            );
        }
    });
}
