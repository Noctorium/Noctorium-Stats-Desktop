//! The charts, drawn with egui's own painter rather than a plotting library: four small charts, each a few
//! dozen rectangles, which want the dashboard's colours and rounding more than they want axes and zooming.
//!
//! Every chart says what is under the pointer, in words, so a bar never has to be measured against a grid.

use super::theme::{self, ACCENT, ACCENT_BRIGHT, FAINT, LINE, SUBTEXT, TEXT};
use crate::format;
use crate::stats::{Bucket, Hour, ServiceRow, Timeline, Weekday};
use eframe::egui::{
    self, Align2, Color32, CornerRadius, Pos2, Rect, Response, RichText, Sense, Ui,
};
use std::f32::consts::{FRAC_PI_2, TAU};

/// The empty part of a bar or a wedge: there, so a quiet hour reads as quiet rather than missing.
const TRACK: Color32 = Color32::from_rgb(0x1D, 0x16, 0x26);

/// A step for the grid lines that a person would choose: 1, 2 or 5 of some power of ten, about a third of
/// the tallest bar, so there are two or three lines and they land on round numbers.
pub fn nice_step(max: u64) -> u64 {
    if max <= 3 {
        return 1;
    }
    let raw = max as f64 / 3.0;
    let magnitude = 10f64.powf(raw.log10().floor());
    let nice = match raw / magnitude {
        n if n <= 1.0 => 1.0,
        n if n <= 2.0 => 2.0,
        n if n <= 5.0 => 5.0,
        _ => 10.0,
    };
    ((nice * magnitude) as u64).max(1)
}

/// The top of the scale: the tallest bar rounded up to the next grid line.
fn scale_top(max: u64) -> u64 {
    let step = nice_step(max);
    max.div_ceil(step).max(1) * step
}

fn top_rounded(radius: f32) -> CornerRadius {
    let r = radius.clamp(0.0, 255.0) as u8;
    CornerRadius {
        nw: r,
        ne: r,
        sw: 0,
        se: 0,
    }
}

fn tooltip(response: Response, title: &str, detail: &str) {
    response.on_hover_ui_at_pointer(|ui| {
        ui.label(RichText::new(title).font(theme::semibold(13.5)).color(TEXT));
        ui.label(
            RichText::new(detail)
                .font(theme::regular(12.5))
                .color(SUBTEXT),
        );
    });
}

/// Plays per day, or per month for the longer ranges, as bars.
pub fn timeline(ui: &mut Ui, timeline: &Timeline, height: f32) {
    let points = &timeline.points;
    let width = ui.available_width();
    let axis = 24.0;
    let gutter = 36.0;
    let (rect, response) = ui.allocate_exact_size(egui::vec2(width, height + axis), Sense::hover());
    let painter = ui.painter_at(rect);
    let plot = Rect::from_min_max(
        rect.min + egui::vec2(gutter, 8.0),
        egui::pos2(rect.right(), rect.top() + height),
    );

    let max = points.iter().map(|p| p.streams).max().unwrap_or(0);
    let top = scale_top(max);
    let step = nice_step(max);
    let y = |value: u64| plot.bottom() - plot.height() * value as f32 / top as f32;

    let mut line = 0;
    while line <= top {
        let at = y(line);
        painter.line_segment(
            [egui::pos2(plot.left(), at), egui::pos2(plot.right(), at)],
            theme::stroke(1.0, LINE),
        );
        painter.text(
            egui::pos2(plot.left() - 10.0, at),
            Align2::RIGHT_CENTER,
            format::count(line),
            theme::regular(11.5),
            FAINT,
        );
        line += step;
    }

    if points.is_empty() {
        return;
    }
    let slot = plot.width() / points.len() as f32;
    // A week has room for bars as wide as a hand; they are kept slimmer than that and centred in their day.
    let bar = (slot - (slot * 0.3).clamp(1.0, 10.0)).clamp(1.0, 56.0);
    let gap = slot - bar;
    let hovered = response
        .hover_pos()
        .filter(|p| p.x >= plot.left() && p.x < plot.right())
        .map(|p| (((p.x - plot.left()) / slot) as usize).min(points.len() - 1));

    for (index, point) in points.iter().enumerate() {
        let left = plot.left() + slot * index as f32 + gap / 2.0;
        if point.streams == 0 {
            // A day with nothing played is a mark on the baseline, so the run of days stays visible.
            painter.rect_filled(
                Rect::from_min_size(egui::pos2(left, plot.bottom() - 2.0), egui::vec2(bar, 2.0)),
                CornerRadius::same(1),
                TRACK,
            );
            continue;
        }
        let bar_rect = Rect::from_min_max(
            egui::pos2(left, y(point.streams).min(plot.bottom() - 2.0)),
            egui::pos2(left + bar, plot.bottom()),
        );
        let colour = if hovered == Some(index) {
            ACCENT_BRIGHT
        } else {
            ACCENT
        };
        painter.rect_filled(bar_rect, top_rounded((bar / 2.0).min(5.0)), colour);
    }

    // As many labels under the bars as fit without touching: the first, the last, and evenly between.
    let label_width = if timeline.bucket == Bucket::Month {
        62.0
    } else {
        46.0
    };
    let every = ((label_width / slot).ceil() as usize).max(1);
    let last = points.len() - 1;
    for (index, point) in points.iter().enumerate() {
        let near_end = index != last && last - index < every;
        if index % every != 0 && index != last || near_end {
            continue;
        }
        let text = match timeline.bucket {
            Bucket::Day if slot >= 84.0 => point.start.format("%a %-d %b").to_string(),
            Bucket::Day => format::short_day(point.start),
            Bucket::Month => format::short_month(point.start, index == 0),
        };
        let x = plot.left() + slot * (index as f32 + 0.5);
        let align = if index == 0 && slot < label_width {
            Align2::LEFT_TOP
        } else if index == last && slot < label_width {
            Align2::RIGHT_TOP
        } else {
            Align2::CENTER_TOP
        };
        let x = match align {
            Align2::LEFT_TOP => plot.left() + gap / 2.0,
            Align2::RIGHT_TOP => plot.right() - gap / 2.0,
            _ => x,
        };
        painter.text(
            egui::pos2(x, plot.bottom() + 8.0),
            align,
            text,
            theme::regular(11.5),
            FAINT,
        );
    }

    if let Some(index) = hovered {
        let point = &points[index];
        let title = match timeline.bucket {
            Bucket::Day => format::long_day(point.start),
            Bucket::Month => format::long_month(point.start),
        };
        tooltip(
            response,
            &title,
            &format!(
                "{} · {}",
                format::plays(point.streams),
                format::minutes(point.minutes)
            ),
        );
    }
}

/// The share of plays by service: one bar in the services' colours, and a row for each beneath it.
pub fn services(ui: &mut Ui, rows: &[ServiceRow]) {
    let width = ui.available_width();
    let (rect, response) = ui.allocate_exact_size(egui::vec2(width, 14.0), Sense::hover());
    let painter = ui.painter_at(rect.expand(1.0));
    let gap = 3.0;
    let usable = width - gap * rows.len().saturating_sub(1) as f32;
    let mut x = rect.left();
    let mut hovered = None;
    for (index, row) in rows.iter().enumerate() {
        // Never narrower than a sliver, so a service played once is still there to be seen.
        let w = (usable * row.share as f32).max(4.0).min(rect.right() - x);
        let segment = Rect::from_min_size(egui::pos2(x, rect.top()), egui::vec2(w, rect.height()));
        let r = 7u8;
        let radius = CornerRadius {
            nw: if index == 0 { r } else { 2 },
            sw: if index == 0 { r } else { 2 },
            ne: if index + 1 == rows.len() { r } else { 2 },
            se: if index + 1 == rows.len() { r } else { 2 },
        };
        painter.rect_filled(segment, radius, theme::rgb(row.provider.colour()));
        if response.hover_pos().is_some_and(|p| segment.contains(p)) {
            hovered = Some(row);
        }
        x += w + gap;
    }
    if let Some(row) = hovered {
        tooltip(
            response,
            &row.provider.name(),
            &format!(
                "{} · {}",
                format::plays(row.streams),
                format::percent(row.share)
            ),
        );
    }
    ui.add_space(14.0);

    for row in rows {
        ui.horizontal(|ui| {
            ui.set_height(30.0);
            theme::dot(ui, theme::rgb(row.provider.colour()), 14.0);
            ui.label(
                RichText::new(row.provider.name())
                    .font(theme::regular(14.0))
                    .color(TEXT),
            );
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                ui.add_sized(
                    egui::vec2(44.0, 20.0),
                    egui::Label::new(
                        RichText::new(format::percent(row.share))
                            .font(theme::semibold(13.5))
                            .color(TEXT),
                    ),
                );
                ui.label(
                    RichText::new(format!(
                        "{} · {}",
                        format::plays(row.streams),
                        hours_short(row.hours)
                    ))
                    .font(theme::regular(12.5))
                    .color(FAINT),
                );
            });
        });
    }
}

/// Hours as the service rounds them, to the tenth: "32 h", "0.4 h", and "<0.1 h" for a song or two.
fn hours_short(hours: f64) -> String {
    if hours <= 0.0 {
        "<0.1 h".into()
    } else if hours < 10.0 {
        format!("{:.1} h", hours)
    } else {
        format!("{} h", format::count(hours.round() as u64))
    }
}

/// The hours of the day as a clock face: a wedge for each hour, longer the more was played in it, midnight
/// at the top as on a watch. The busiest hour is lit and named in the middle.
pub fn clock(ui: &mut Ui, hours: &[Hour; 24], size: f32) {
    let width = ui.available_width();
    let (rect, response) = ui.allocate_exact_size(egui::vec2(width, size), Sense::hover());
    let painter = ui.painter_at(rect);
    let centre = rect.center();
    let outer = size / 2.0 - 20.0;
    let inner = outer * 0.44;
    let max = hours.iter().map(|h| h.streams).max().unwrap_or(0);
    let peak = hours
        .iter()
        .filter(|h| h.streams > 0)
        .max_by_key(|h| (h.streams, std::cmp::Reverse(h.hour)));

    let angle = |hour: f32| hour / 24.0 * TAU - FRAC_PI_2;
    let gap = 0.035;
    for hour in hours {
        let a0 = angle(hour.hour as f32) + gap;
        let a1 = angle(hour.hour as f32 + 1.0) - gap;
        wedge(&painter, centre, inner, outer, a0, a1, TRACK);
        if hour.streams > 0 && max > 0 {
            let reach = inner + (outer - inner) * (hour.streams as f32 / max as f32);
            let lit = peak.is_some_and(|p| p.hour == hour.hour);
            let colour = if lit {
                ACCENT_BRIGHT
            } else {
                ACCENT.gamma_multiply(0.62)
            };
            wedge(
                &painter,
                centre,
                inner,
                reach.max(inner + 2.0),
                a0,
                a1,
                colour,
            );
        }
    }
    for (hour, align) in [
        (0, Align2::CENTER_BOTTOM),
        (6, Align2::LEFT_CENTER),
        (12, Align2::CENTER_TOP),
        (18, Align2::RIGHT_CENTER),
    ] {
        let a = angle(hour as f32);
        let at = centre + egui::vec2(a.cos(), a.sin()) * (outer + 7.0);
        painter.text(at, align, format!("{hour:02}"), theme::regular(11.5), FAINT);
    }
    match peak {
        Some(peak) => {
            painter.text(
                centre - egui::vec2(0.0, 8.0),
                Align2::CENTER_CENTER,
                format!("{:02}:00", peak.hour),
                theme::bold(20.0),
                TEXT,
            );
            painter.text(
                centre + egui::vec2(0.0, 13.0),
                Align2::CENTER_CENTER,
                "busiest hour",
                theme::regular(11.5),
                FAINT,
            );
        }
        None => {
            painter.text(
                centre,
                Align2::CENTER_CENTER,
                "No plays",
                theme::regular(12.5),
                FAINT,
            );
        }
    }

    if let Some(pointer) = response.hover_pos() {
        let offset = pointer - centre;
        let distance = offset.length();
        if distance >= inner - 4.0 && distance <= outer + 6.0 {
            let turned = (offset.y.atan2(offset.x) + FRAC_PI_2).rem_euclid(TAU);
            let index = ((turned / TAU * 24.0) as usize).min(23);
            let hour = &hours[index];
            tooltip(
                response,
                &format::hour_span(hour.hour),
                &format!(
                    "{} · {}",
                    format::plays(hour.streams),
                    format::minutes(hour.minutes)
                ),
            );
        }
    }
}

/// A ring segment between two radii and two angles. Drawn as one thick stroke along the middle radius: a
/// stroke's width is perpendicular to its path, which on a circle is the radius, so its ends come out as
/// clean radial cuts and its edges are smoothed like everything else egui draws.
fn wedge(
    painter: &egui::Painter,
    centre: Pos2,
    inner: f32,
    outer: f32,
    a0: f32,
    a1: f32,
    colour: Color32,
) {
    let middle = (inner + outer) / 2.0;
    let steps = 8;
    let points: Vec<Pos2> = (0..=steps)
        .map(|i| {
            let a = a0 + (a1 - a0) * i as f32 / steps as f32;
            centre + egui::vec2(a.cos(), a.sin()) * middle
        })
        .collect();
    painter.add(egui::Shape::line(
        points,
        theme::stroke(outer - inner, colour),
    ));
}

/// Monday to Sunday as bars, the busiest lit, each with its count above it.
pub fn week(ui: &mut Ui, days: &[Weekday; 7], height: f32) {
    let width = ui.available_width();
    let (rect, response) = ui.allocate_exact_size(egui::vec2(width, height), Sense::hover());
    let painter = ui.painter_at(rect);
    let label = 22.0;
    let value = 20.0;
    let plot = Rect::from_min_max(
        rect.min + egui::vec2(0.0, value),
        egui::pos2(rect.right(), rect.bottom() - label),
    );
    let max = days.iter().map(|d| d.streams).max().unwrap_or(0);
    let peak = days
        .iter()
        .filter(|d| d.streams > 0)
        .max_by_key(|d| (d.streams, std::cmp::Reverse(d.day)))
        .map(|d| d.day);
    let slot = plot.width() / 7.0;
    let bar = (slot * 0.56).min(46.0);

    let mut hovered = None;
    for (index, day) in days.iter().enumerate() {
        let x = plot.left() + slot * (index as f32 + 0.5);
        let track = Rect::from_min_max(
            egui::pos2(x - bar / 2.0, plot.top()),
            egui::pos2(x + bar / 2.0, plot.bottom()),
        );
        painter.rect_filled(track, CornerRadius::same(8), TRACK);
        let lit = peak == Some(day.day);
        if day.streams > 0 && max > 0 {
            let h = (plot.height() * day.streams as f32 / max as f32).max(6.0);
            let filled = Rect::from_min_max(egui::pos2(track.left(), plot.bottom() - h), track.max);
            painter.rect_filled(
                filled,
                CornerRadius::same(8),
                if lit {
                    ACCENT_BRIGHT
                } else {
                    ACCENT.gamma_multiply(0.62)
                },
            );
        }
        painter.text(
            egui::pos2(x, plot.top() - 6.0),
            Align2::CENTER_BOTTOM,
            format::count(day.streams),
            if lit {
                theme::semibold(12.0)
            } else {
                theme::regular(12.0)
            },
            if lit { TEXT } else { FAINT },
        );
        painter.text(
            egui::pos2(x, plot.bottom() + 8.0),
            Align2::CENTER_TOP,
            format::weekday_short(day.day),
            if lit {
                theme::semibold(12.5)
            } else {
                theme::regular(12.5)
            },
            if lit { TEXT } else { SUBTEXT },
        );
        if response
            .hover_pos()
            .is_some_and(|p| (p.x - x).abs() <= slot / 2.0)
        {
            hovered = Some(day);
        }
    }
    if let Some(day) = hovered {
        tooltip(
            response,
            format::weekday(day.day),
            &format!(
                "{} · {}",
                format::plays(day.streams),
                format::minutes(day.minutes)
            ),
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn grid_lines_land_on_round_numbers() {
        assert_eq!(nice_step(0), 1);
        assert_eq!(nice_step(3), 1);
        assert_eq!(nice_step(9), 5);
        assert_eq!(nice_step(152), 100);
        assert_eq!(nice_step(60), 20);
        assert_eq!(nice_step(1_284), 500);
        assert_eq!(scale_top(152), 200);
        assert_eq!(scale_top(9), 10);
        assert_eq!(scale_top(0), 1);
    }
}
