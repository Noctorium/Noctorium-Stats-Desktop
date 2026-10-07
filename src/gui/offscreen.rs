//! The window drawn without a window: every screen laid out by egui, painted into memory by a few dozen lines
//! of rasteriser, and written out as a PNG -- so what it looks like can be checked on a machine with no
//! display, or without anybody's desktop being touched to find out. The rasteriser is the installer's.
//!
//! The painting is the same arithmetic egui's own OpenGL painter does: vertex colour times texture in gamma
//! space, premultiplied, blended one-minus-source-alpha. It does not antialias -- egui's shapes are feathered
//! at their edges already, which is what the GPU is relied on for anyway.
//!
//! `cargo test --lib -- --ignored the_window_drawn` writes the pictures into the folder
//! NOCTORIUM_RENDER_TO names, or into noctorium-stats-render in the temporary folder. The tests that are not
//! ignored lay every screen out and read back what a screen reader would be told, which is quick and needs
//! no pictures.

use super::*;
use crate::store::Places;
use eframe::egui::accesskit::Role;
use std::path::{Path, PathBuf};

/// A texture as egui has described it so far.
struct Texture {
    size: [usize; 2],
    pixels: Vec<egui::Color32>,
}

impl Texture {
    /// Bilinear, clamped at the edges, as the painter's sampler is.
    fn sample(&self, uv: egui::Pos2) -> [f32; 4] {
        let [w, h] = self.size;
        let x = (uv.x * w as f32 - 0.5).clamp(0.0, (w - 1) as f32);
        let y = (uv.y * h as f32 - 0.5).clamp(0.0, (h - 1) as f32);
        let (x0, y0) = (x.floor() as usize, y.floor() as usize);
        let (x1, y1) = ((x0 + 1).min(w - 1), (y0 + 1).min(h - 1));
        let (fx, fy) = (x - x0 as f32, y - y0 as f32);
        let at = |x: usize, y: usize| {
            let c = self.pixels[y * w + x];
            [c.r(), c.g(), c.b(), c.a()].map(|v| v as f32 / 255.0)
        };
        let (a, b, c, d) = (at(x0, y0), at(x1, y0), at(x0, y1), at(x1, y1));
        std::array::from_fn(|i| {
            let top = a[i] + (b[i] - a[i]) * fx;
            let bottom = c[i] + (d[i] - c[i]) * fx;
            top + (bottom - top) * fy
        })
    }
}

/// One frame, as pixels, and what AccessKit was told about it.
pub struct Picture {
    pub width: usize,
    pub height: usize,
    rgba: Vec<[f32; 4]>,
    pub accessible: Vec<egui::accesskit::Node>,
}

/// Lays out [draw] in a window of [size] points, a few frames running so everything that measures itself
/// has, and paints the last of them at [scale] pixels a point when [paint] says to. [draw] says whether the
/// window has settled; until it has, frames go on being drawn, a little apart, for up to a minute -- which is
/// what lets a window waiting on a real service be drawn once the answer is in.
pub fn render(
    size: egui::Vec2,
    scale: f32,
    paint: bool,
    mut draw: impl FnMut(&egui::Context) -> bool,
) -> Picture {
    let ctx = egui::Context::default();
    ctx.enable_accesskit();
    // Before the first frame, as eframe does: fonts given during a frame only arrive with the next one.
    theme::install(&ctx);
    let mut textures: HashMap<egui::TextureId, Texture> = HashMap::new();
    let mut primitives = Vec::new();
    let mut accessible = Vec::new();
    let started = std::time::Instant::now();
    let mut settled_for = 0;
    let mut frame = 0;
    while settled_for < 5 {
        frame += 1;
        if frame > 5 {
            std::thread::sleep(std::time::Duration::from_millis(50));
        }
        let mut input = egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, size)),
            time: Some(1.0 + frame as f64 / 60.0),
            ..Default::default()
        };
        input
            .viewports
            .entry(egui::ViewportId::ROOT)
            .or_default()
            .native_pixels_per_point = Some(scale);
        let mut settled = false;
        let output = ctx.run(input, |ctx| settled = draw(ctx));
        settled_for = if settled { settled_for + 1 } else { 0 };
        assert!(started.elapsed().as_secs() < 60, "the window never settled");
        for (id, delta) in &output.textures_delta.set {
            let egui::ImageData::Color(image) = &delta.image;
            match delta.pos {
                None => {
                    textures.insert(
                        *id,
                        Texture {
                            size: image.size,
                            pixels: image.pixels.clone(),
                        },
                    );
                }
                Some([left, top]) => {
                    let texture = textures
                        .get_mut(id)
                        .expect("a patch of a texture never set");
                    for y in 0..image.size[1] {
                        for x in 0..image.size[0] {
                            texture.pixels[(top + y) * texture.size[0] + left + x] =
                                image.pixels[y * image.size[0] + x];
                        }
                    }
                }
            }
        }
        if let Some(update) = output.platform_output.accesskit_update {
            accessible = update.nodes.into_iter().map(|(_, node)| node).collect();
        }
        primitives = ctx.tessellate(output.shapes, output.pixels_per_point);
    }

    let width = (size.x * scale).round() as usize;
    let height = (size.y * scale).round() as usize;
    let mut picture = Picture {
        width,
        height,
        rgba: vec![[0.0, 0.0, 0.0, 1.0]; width * height],
        accessible,
    };
    if paint {
        for clipped in &primitives {
            if let egui::epaint::Primitive::Mesh(mesh) = &clipped.primitive {
                if let Some(texture) = textures.get(&mesh.texture_id) {
                    picture.mesh(mesh, texture, clipped.clip_rect, scale);
                }
            }
        }
    }
    picture
}

impl Picture {
    fn mesh(&mut self, mesh: &egui::Mesh, texture: &Texture, clip: egui::Rect, scale: f32) {
        let left = ((clip.min.x * scale).floor().max(0.0)) as usize;
        let top = ((clip.min.y * scale).floor().max(0.0)) as usize;
        let right = ((clip.max.x * scale).ceil().max(0.0) as usize).min(self.width);
        let bottom = ((clip.max.y * scale).ceil().max(0.0) as usize).min(self.height);
        for triangle in mesh.indices.as_chunks::<3>().0 {
            let [a, b, c] = [0, 1, 2].map(|i| &mesh.vertices[triangle[i] as usize]);
            let [pa, pb, pc] = [a, b, c].map(|v| v.pos * scale);
            let edge = |p: egui::Pos2, q: egui::Pos2, r: egui::Pos2| {
                (q.x - p.x) * (r.y - p.y) - (q.y - p.y) * (r.x - p.x)
            };
            let area = edge(pa, pb, pc);
            if area.abs() < 1e-9 {
                continue;
            }
            let x0 = (pa.x.min(pb.x).min(pc.x).floor().max(0.0) as usize).max(left);
            let y0 = (pa.y.min(pb.y).min(pc.y).floor().max(0.0) as usize).max(top);
            let x1 = ((pa.x.max(pb.x).max(pc.x).ceil().max(0.0)) as usize + 1).min(right);
            let y1 = ((pa.y.max(pb.y).max(pc.y).ceil().max(0.0)) as usize + 1).min(bottom);
            let colour = |v: &egui::epaint::Vertex| {
                [v.color.r(), v.color.g(), v.color.b(), v.color.a()].map(|c| c as f32 / 255.0)
            };
            let (ca, cb, cc) = (colour(a), colour(b), colour(c));
            for y in y0..y1 {
                for x in x0..x1 {
                    // A hair off the pixel's centre, so a point never lies exactly on the edge two triangles
                    // share, and is painted by one of them rather than by both or neither.
                    let p = egui::pos2(x as f32 + 0.500_37, y as f32 + 0.500_61);
                    let wa = edge(pb, pc, p) / area;
                    let wb = edge(pc, pa, p) / area;
                    let wc = 1.0 - wa - wb;
                    if wa < 0.0 || wb < 0.0 || wc < 0.0 {
                        continue;
                    }
                    let uv = egui::pos2(
                        a.uv.x * wa + b.uv.x * wb + c.uv.x * wc,
                        a.uv.y * wa + b.uv.y * wb + c.uv.y * wc,
                    );
                    let texel = texture.sample(uv);
                    let source: [f32; 4] =
                        std::array::from_fn(|i| (ca[i] * wa + cb[i] * wb + cc[i] * wc) * texel[i]);
                    let pixel = &mut self.rgba[y * self.width + x];
                    for i in 0..4 {
                        pixel[i] = source[i] + pixel[i] * (1.0 - source[3]);
                    }
                }
            }
        }
    }

    pub fn save(&self, path: &Path) {
        let bytes: Vec<u8> = self
            .rgba
            .iter()
            .flat_map(|pixel| pixel.map(|c| (c.clamp(0.0, 1.0) * 255.0).round() as u8))
            .collect();
        let file = std::io::BufWriter::new(std::fs::File::create(path).unwrap());
        let mut encoder = png::Encoder::new(file, self.width as u32, self.height as u32);
        encoder.set_color(png::ColorType::Rgba);
        encoder.set_depth(png::BitDepth::Eight);
        encoder
            .write_header()
            .unwrap()
            .write_image_data(&bytes)
            .unwrap();
    }

    /// Whether anything on screen says [text], as its label or its value.
    pub fn says(&self, text: &str) -> bool {
        self.accessible.iter().any(|node| {
            node.label().is_some_and(|l| l.contains(text))
                || node.value().is_some_and(|v| v.contains(text))
        })
    }

    fn buttons(&self) -> Vec<String> {
        self.accessible
            .iter()
            .filter(|node| node.role() == Role::Button)
            .filter_map(|node| node.label().map(str::to_string))
            .collect()
    }
}

// ------------------------------------------------------------------------------------------ the screens

const SERVICE: &str = "http://localhost:3000";

/// The moment the fixtures are looked at from: the evening they were captured, in London, which is the
/// clock they were counted on.
fn evening() -> DateTime<Utc> {
    DateTime::parse_from_rfc3339("2026-10-07T18:40:00Z")
        .unwrap()
        .with_timezone(&Utc)
}

fn fetched() -> DateTime<Utc> {
    DateTime::parse_from_rfc3339("2026-10-07T18:33:00Z")
        .unwrap()
        .with_timezone(&Utc)
}

fn fixture(name: &str) -> Stats {
    let path = format!("{}/tests/fixtures/{name}.json", env!("CARGO_MANIFEST_DIR"));
    Stats::parse(&std::fs::read_to_string(path).unwrap()).unwrap()
}

fn listener() -> User {
    User {
        id: 2,
        email: "robin@example.test".into(),
        display_name: "Robin Vale".into(),
    }
}

/// A folder for a test's settings, emptied first, so nothing of this machine's own is read.
fn places(name: &str) -> Places {
    let folder =
        std::env::temp_dir().join(format!("noctorium-stats-gui-{}-{name}", std::process::id()));
    let _ = std::fs::remove_dir_all(&folder);
    Places::at(folder)
}

/// The window with nothing behind it: jobs are written down, not done, and the clock is stopped.
fn app(ctx: &egui::Context, name: &str) -> App {
    let start = Start {
        token: None,
        settings: Settings::default(),
    };
    let mut app = App::new(
        ctx,
        SERVICE.into(),
        places(name),
        start,
        Effects::Recorded(Vec::new()),
    );
    app.zone = Zone::east_minutes(60);
    app.now = Some(evening());
    app
}

/// Signed in, looking at [range], with [stats] for it if there are any.
fn board(app: &mut App, range: Range, stats: Option<Stats>) {
    let mut views = HashMap::new();
    if let Some(stats) = stats {
        views.insert(
            range,
            View {
                stats,
                fetched_at: fetched(),
                from_cache: false,
            },
        );
    }
    app.epoch += 1;
    app.screen = Screen::Board(Box::new(Board {
        token: "token".into(),
        user: Some(listener()),
        range,
        views,
        pending: HashSet::new(),
        problems: HashMap::new(),
        kept_note: None,
    }));
}

fn the_board(app: &mut App) -> &mut Board {
    match &mut app.screen {
        Screen::Board(board) => board,
        Screen::SignIn(_) => panic!("not signed in"),
    }
}

/// The window at [size], set up by [setup] on its first frame.
fn picture(
    size: [f32; 2],
    paint: bool,
    name: &str,
    setup: impl FnOnce(&mut App),
) -> (Picture, App) {
    let mut app: Option<App> = None;
    let mut setup = Some(setup);
    let picture = render(egui::vec2(size[0], size[1]), 1.0, paint, |ctx| {
        let app = app.get_or_insert_with(|| {
            let mut app = self::app(ctx, name);
            (setup.take().unwrap())(&mut app);
            app
        });
        app.ui(ctx);
        true
    });
    (picture, app.unwrap())
}

#[test]
fn signed_out_it_asks_for_the_account() {
    let (picture, _) = picture([900.0, 700.0], false, "signed-out", |_| {});
    assert!(picture.says("Noctorium Stats"));
    assert!(picture.says("Email"));
    assert!(picture.says("Password"));
    assert!(picture.buttons().iter().any(|b| b == "Sign in"));
    assert!(picture.says("Open the Noctorium website"));
    assert!(picture.says(&format!("Noctorium Stats {}", crate::version())));
}

#[test]
fn the_form_says_what_is_missing_before_asking_anyone() {
    let ctx = egui::Context::default();
    let mut app = app(&ctx, "form");
    app.act(Action::Submit);
    let Screen::SignIn(form) = &app.screen else {
        panic!()
    };
    assert_eq!(form.problem.as_deref(), Some("Enter your email address."));
    assert!(app
        .jobs()
        .iter()
        .all(|job| !matches!(job, Job::SignIn { .. })));

    app.form_mut().unwrap().email = "robin@example.test".into();
    app.form_mut().unwrap().password = "short".into();
    app.act(Action::SwitchMode(Mode::Create));
    app.act(Action::Submit);
    let Screen::SignIn(form) = &app.screen else {
        panic!()
    };
    assert_eq!(
        form.problem.as_deref(),
        Some("Use at least 10 characters for the password.")
    );

    app.form_mut().unwrap().password = "long enough to pass".into();
    app.form_mut().unwrap().name = " Robin ".into();
    app.act(Action::Submit);
    assert!(app.jobs().contains(&Job::SignUp {
        email: "robin@example.test".into(),
        password: "long enough to pass".into(),
        name: "Robin".into()
    }));
    let Screen::SignIn(form) = &app.screen else {
        panic!()
    };
    assert!(form.busy);
}

#[test]
fn a_sign_in_opens_the_dashboard_and_asks_for_the_range() {
    let ctx = egui::Context::default();
    let mut app = app(&ctx, "sign-in");
    app.settings.range = Range::Week;
    let session = Session {
        token: "fresh".into(),
        user: listener(),
    };
    app.sender
        .send(Reply::Session(app.epoch, Ok((session, None))))
        .unwrap();
    app.receive();
    let board = the_board(&mut app);
    assert_eq!(board.range, Range::Week);
    assert!(board.pending.contains(&Range::Week));
    assert!(app.jobs().contains(&Job::Stats {
        token: "fresh".into(),
        range: Range::Week,
        tz: 60,
        user: Some(2)
    }));
    // Who signed in is remembered, without the token.
    assert_eq!(app.settings.user, Some(listener()));
}

#[test]
fn a_wrong_password_is_said_on_the_form() {
    let ctx = egui::Context::default();
    let mut app = app(&ctx, "wrong-password");
    let problem =
        Problem::Refused("That email address and password do not match an account.".into());
    app.sender
        .send(Reply::Session(app.epoch, Err(problem)))
        .unwrap();
    app.receive();
    let Screen::SignIn(form) = &app.screen else {
        panic!()
    };
    assert_eq!(
        form.problem.as_deref(),
        Some("That email address and password do not match an account.")
    );
    assert!(!form.busy);
}

#[test]
fn a_refused_token_goes_back_to_the_form_and_says_why() {
    let ctx = egui::Context::default();
    let mut app = app(&ctx, "refused");
    board(&mut app, Range::Month, None);
    app.sender
        .send(Reply::Stats(
            app.epoch,
            Range::Month,
            Err(Problem::SignedOut),
        ))
        .unwrap();
    app.receive();
    let Screen::SignIn(form) = &app.screen else {
        panic!("still signed in")
    };
    assert!(form
        .notice
        .as_deref()
        .is_some_and(|n| n.contains("run out")));
    assert!(app.jobs().contains(&Job::SignOut));
}

#[test]
fn an_answer_for_an_earlier_sign_in_is_dropped() {
    let ctx = egui::Context::default();
    let mut app = app(&ctx, "stale");
    board(&mut app, Range::Month, None);
    let old = app.epoch;
    app.act(Action::SignOut);
    app.sender
        .send(Reply::Stats(
            old,
            Range::Month,
            Ok((fixture("listener-30d"), fetched())),
        ))
        .unwrap();
    app.receive();
    assert!(matches!(app.screen, Screen::SignIn(_)));
}

#[test]
fn choosing_a_range_asks_for_it_once() {
    let ctx = egui::Context::default();
    let mut app = app(&ctx, "range");
    board(&mut app, Range::Month, Some(fixture("listener-30d")));
    app.act(Action::Range(Range::Year));
    app.act(Action::Range(Range::Month));
    app.act(Action::Range(Range::Year));
    let asked: Vec<Range> = app
        .jobs()
        .iter()
        .filter_map(|job| match job {
            Job::Stats { range, .. } => Some(*range),
            _ => None,
        })
        .collect();
    // The year once, though it was chosen twice before it came back; the month not at all, having just
    // been fetched.
    assert_eq!(asked, vec![Range::Year]);
    assert_eq!(app.settings.range, Range::Year);
}

#[test]
fn the_dashboard_shows_the_figures() {
    let (picture, _) = picture([1280.0, 3000.0], false, "dashboard", |app| {
        board(app, Range::All, Some(fixture("listener-all")));
    });
    for text in [
        "Your listening",
        "Songs streamed",
        "1,553",
        "Different songs",
        "Hours listened",
        "78",
        "Since 14 Aug 2025",
        "Updated at 19:33",
        "Plays each month",
        "Where you listened",
        "YouTube Music",
        "Your streak",
        "Top songs",
        "Top artists",
        "Hours of the day",
        "Days of the week",
        "Recently played",
        "Robin Vale",
    ] {
        assert!(picture.says(text), "nothing says {text:?}");
    }
    assert!(picture.buttons().iter().any(|b| b.contains("Refresh")));
    assert!(picture.buttons().iter().any(|b| b == "Sign out"));
}

#[test]
fn an_account_with_nothing_played_says_what_to_do() {
    let (picture, _) = picture([1000.0, 900.0], false, "empty", |app| {
        board(app, Range::All, Some(fixture("empty-all")));
    });
    assert!(picture.says("Nothing played yet"));
    assert!(picture.says("Open Noctorium and sign in to this same account."));
    assert!(!picture.says("Songs streamed"));
}

#[test]
fn a_quiet_week_is_not_an_empty_account() {
    let (picture, _) = picture([1000.0, 900.0], false, "quiet", |app| {
        let mut stats = fixture("listener-7d");
        stats.streams = 0;
        stats.unique_tracks = 0;
        stats.artists = 0;
        stats.ms_played = 0;
        board(app, Range::Week, Some(stats));
    });
    assert!(picture.says("Nothing in the last 7 days"));
    assert!(picture.says("Songs streamed"));
}

#[test]
fn offline_with_nothing_cached_says_so() {
    let (picture, _) = picture([1000.0, 800.0], false, "offline", |app| {
        board(app, Range::Month, None);
        the_board(app)
            .problems
            .insert(Range::Month, Problem::Offline);
    });
    assert!(picture.says("You seem to be offline"));
    assert!(picture.buttons().iter().any(|b| b == "Try again"));
}

#[test]
fn offline_with_figures_shows_them_and_says_how_old_they_are() {
    let (picture, _) = picture([1200.0, 900.0], false, "offline-cached", |app| {
        board(app, Range::Week, Some(fixture("listener-7d")));
        the_board(app)
            .problems
            .insert(Range::Week, Problem::Offline);
    });
    assert!(picture.says("Offline. These are the figures from 19:33."));
    assert!(picture.says("31"));
}

#[test]
fn every_range_has_its_switch() {
    let (picture, _) = picture([1200.0, 900.0], false, "switch", |app| {
        board(app, Range::Week, Some(fixture("listener-7d")));
    });
    for range in Range::ALL {
        assert!(picture.says(range.label()), "no switch for {range:?}");
    }
}

/// Every screen, painted, for a person to look at.
#[test]
#[ignore = "writes pictures; run with --ignored to look at the window"]
fn the_window_drawn() {
    let folder = std::env::var_os("NOCTORIUM_RENDER_TO")
        .map(PathBuf::from)
        .unwrap_or_else(|| std::env::temp_dir().join("noctorium-stats-render"));
    std::fs::create_dir_all(&folder).unwrap();
    let draw = |name: &str, size: [f32; 2], setup: &dyn Fn(&mut App)| {
        let (picture, _) = self::picture(size, true, name, |app| setup(app));
        picture.save(&folder.join(format!("{name}.png")));
    };

    draw("signed-out", [1180.0, 820.0], &|_| {});
    draw("signed-out-small", [800.0, 600.0], &|_| {});
    draw("create-account-problem", [1180.0, 820.0], &|app| {
        let form = app.form_mut().unwrap();
        form.mode = Mode::Create;
        form.email = "robin@example.test".into();
        form.password = "correct horse".into();
        form.problem = Some("That email address already has an account.".into());
    });
    draw("signed-out-ran-out", [1180.0, 820.0], &|app| {
        let form = app.form_mut().unwrap();
        form.email = "robin@example.test".into();
        form.notice = Some(Problem::SignedOut.message());
    });
    draw("signing-in", [800.0, 600.0], &|app| {
        let form = app.form_mut().unwrap();
        form.email = "robin@example.test".into();
        form.password = "correct horse battery".into();
        form.busy = true;
    });

    for (range, name) in [
        (Range::Week, "7d"),
        (Range::Month, "30d"),
        (Range::Year, "365d"),
        (Range::All, "all"),
    ] {
        let stats = fixture(&format!("listener-{name}"));
        let s = stats.clone();
        draw(
            &format!("dashboard-{name}-wide"),
            [1440.0, 2300.0],
            &move |app| board(app, range, Some(s.clone())),
        );
        let s = stats.clone();
        draw(
            &format!("dashboard-{name}-window"),
            [1180.0, 820.0],
            &move |app| board(app, range, Some(s.clone())),
        );
    }
    let stats = fixture("listener-90d");
    draw("dashboard-90d-narrow", [800.0, 3600.0], &move |app| {
        board(app, Range::Quarter, Some(stats.clone()))
    });
    let stats = fixture("listener-30d");
    draw("dashboard-30d-small-window", [800.0, 600.0], &move |app| {
        board(app, Range::Month, Some(stats.clone()))
    });
    let stats = fixture("listener-30d");
    draw("dashboard-30d-mid", [1000.0, 2600.0], &move |app| {
        board(app, Range::Month, Some(stats.clone()))
    });
    // The window at its smallest, where the cards stack and the switcher goes under the title.
    let stats = fixture("listener-all");
    draw("dashboard-all-smallest", [720.0, 540.0], &move |app| {
        board(app, Range::All, Some(stats.clone()))
    });
    let stats = fixture("listener-all");
    draw(
        "dashboard-all-smallest-page",
        [720.0, 4300.0],
        &move |app| board(app, Range::All, Some(stats.clone())),
    );
    draw("signed-out-smallest", [720.0, 540.0], &|_| {});

    draw("empty-account", [1180.0, 820.0], &|app| {
        board(app, Range::All, Some(fixture("empty-all")))
    });
    draw("loading", [1180.0, 820.0], &|app| {
        board(app, Range::Month, None);
        the_board(app).pending.insert(Range::Month);
    });
    draw("offline", [1180.0, 820.0], &|app| {
        board(app, Range::Month, None);
        the_board(app)
            .problems
            .insert(Range::Month, Problem::Offline);
    });
    draw("server-error", [1180.0, 820.0], &|app| {
        board(app, Range::Month, None);
        the_board(app)
            .problems
            .insert(Range::Month, Problem::Server(502));
    });
    let stats = fixture("listener-7d");
    draw("offline-with-figures", [1180.0, 820.0], &move |app| {
        board(app, Range::Week, Some(stats.clone()));
        the_board(app)
            .problems
            .insert(Range::Week, Problem::Offline);
        the_board(app).kept_note =
            Some("Your sign-in is kept in a file, readable only by you.".into());
        app.newer = Some(Newer {
            version: "0.13.0".into(),
            url: crate::update::RELEASES.into(),
        });
    });
    let stats = fixture("listener-30d");
    draw("refreshing", [1180.0, 820.0], &move |app| {
        board(app, Range::Month, Some(stats.clone()));
        the_board(app).pending.insert(Range::Month);
    });

    eprintln!("the window, drawn: {}", folder.display());
}

/// The whole of it against a real copy of the service: the form filled in and sent, the sign-in and the
/// figures fetched on threads as they are in the program, and the dashboard drawn from what came back. Needs
/// NOCTORIUM_SERVICE_URL and an account on it in NOCTORIUM_TEST_EMAIL and NOCTORIUM_TEST_PASSWORD; never
/// the live service. One sign-in for each picture, since the service counts them.
#[test]
#[ignore = "needs a running service and an account on it"]
fn the_window_against_a_service() {
    let (Ok(service), Ok(email), Ok(password)) = (
        std::env::var("NOCTORIUM_SERVICE_URL"),
        std::env::var("NOCTORIUM_TEST_EMAIL"),
        std::env::var("NOCTORIUM_TEST_PASSWORD"),
    ) else {
        eprintln!(
            "skipped: set NOCTORIUM_SERVICE_URL, NOCTORIUM_TEST_EMAIL and NOCTORIUM_TEST_PASSWORD"
        );
        return;
    };
    let service = service.trim().trim_end_matches('/').to_string();
    assert_ne!(
        service,
        api::LIVE,
        "this signs in, and is not for the live service"
    );
    let folder = std::env::var_os("NOCTORIUM_RENDER_TO")
        .map(PathBuf::from)
        .unwrap_or_else(|| std::env::temp_dir().join("noctorium-stats-render"));
    std::fs::create_dir_all(&folder).unwrap();

    for (range, name) in [(Range::Month, "live-30d"), (Range::Year, "live-365d")] {
        let places = places(name);
        let mut app: Option<App> = None;
        let picture = render(egui::vec2(1280.0, 2300.0), 1.0, true, |ctx| {
            let app = app.get_or_insert_with(|| {
                let start = Start {
                    token: None,
                    settings: Settings {
                        range,
                        ..Settings::default()
                    },
                };
                let effects = Effects::Threads {
                    ctx: ctx.clone(),
                    places: places.clone(),
                };
                let mut app = App::new(ctx, service.clone(), places.clone(), start, effects);
                let form = app.form_mut().unwrap();
                form.email = email.clone();
                form.password = password.clone();
                app.act(Action::Submit);
                app
            });
            app.ui(ctx);
            match &app.screen {
                Screen::Board(board) => {
                    board.pending.is_empty() && board.views.contains_key(&range)
                }
                Screen::SignIn(form) => {
                    assert!(
                        form.problem.is_none(),
                        "could not sign in: {:?}",
                        form.problem
                    );
                    false
                }
            }
        });
        assert!(picture.says("Songs streamed"));
        assert!(picture.says("Top songs"));
        // The token went into the folder's file, since the keyring is never used here.
        assert!(crate::store::load_token(&places, &service).is_some());
        picture.save(&folder.join(format!("{name}.png")));
    }
}
