//! The window.
//!
//! Immediate mode: every frame draws whatever the state is now, and the state changes only here, from what
//! the person did and from what the threads talking to the service have sent back. Nothing waits on the
//! network on the thread that draws -- each request is a thread of its own, which posts its answer and asks
//! for a repaint -- so the window stays alive however slow the service is.

mod charts;
mod dashboard;
mod sign_in;
pub mod theme;

#[cfg(test)]
mod offscreen;

use crate::api::{self, Client, Problem, Session, User};
use crate::format::Zone;
use crate::stats::{Range, Stats};
use crate::store::{self, Cached, Kept, Places, Settings};
use crate::update::{self, Newer};
use chrono::{DateTime, Utc};
use eframe::egui;
use std::collections::{HashMap, HashSet};
use std::sync::mpsc::{channel, Receiver, Sender};
use std::sync::Mutex;

/// Held while the sign-in is saved or forgotten. Both happen on threads, and a sign-out followed quickly by
/// a sign-in must not have the forgetting land after the saving.
static KEEPING: Mutex<()> = Mutex::new(());

/// The mark, decoded by the build script into raw pixels.
const MARK: &[u8] = include_bytes!(concat!(env!("OUT_DIR"), "/mark.rgba"));
include!(concat!(env!("OUT_DIR"), "/mark.rs"));

pub const TITLE: &str = "Noctorium Stats";

/// Opens the window, and returns when it is closed. The error is only ever "no window could be opened".
pub fn run() -> Result<(), String> {
    let places = Places::system();
    let service = api::service_url();
    // Read before the window opens: on Linux, the keyring may ask for its password, and that is better
    // asked before there is a window than while one sits frozen behind the question.
    let start = Start::load(&places, &service);

    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_title(TITLE)
            // What Wayland and X11 group the window under, and what the .desktop file's StartupWMClass names.
            .with_app_id("noctorium-stats")
            .with_inner_size([1180.0, 820.0])
            .with_min_inner_size([720.0, 540.0])
            .with_icon(egui::IconData {
                rgba: MARK.to_vec(),
                width: MARK_SIDE as u32,
                height: MARK_SIDE as u32,
            }),
        ..Default::default()
    };
    eframe::run_native(
        TITLE,
        options,
        Box::new(move |cc| {
            theme::install(&cc.egui_ctx);
            let effects = Effects::Threads {
                ctx: cc.egui_ctx.clone(),
                places: places.clone(),
            };
            Ok(Box::new(App::new(
                &cc.egui_ctx,
                service,
                places,
                start,
                effects,
            )))
        }),
    )
    .map_err(|e| e.to_string())
}

/// What is known before the window opens.
pub struct Start {
    pub token: Option<(String, Kept)>,
    pub settings: Settings,
}

impl Start {
    pub fn load(places: &Places, service: &str) -> Start {
        Start {
            token: store::load_token(places, service),
            settings: Settings::load(places),
        }
    }
}

/// Something to do away from the window.
#[derive(Debug, Clone, PartialEq)]
pub enum Job {
    SignIn {
        email: String,
        password: String,
    },
    SignUp {
        email: String,
        password: String,
        name: String,
    },
    Me {
        token: String,
    },
    Stats {
        token: String,
        range: Range,
        tz: i32,
        user: Option<u64>,
    },
    SignOut,
    CheckForUpdate,
}

/// What a job sends back. Each carries the sign-in it was started under, so an answer that arrives after
/// signing out -- or after signing in as somebody else -- is dropped rather than shown.
enum Reply {
    Session(u64, Result<(Session, Option<String>), Problem>),
    Me(u64, Result<User, Problem>),
    Stats(u64, Range, Result<(Stats, DateTime<Utc>), Problem>),
    Newer(Option<Newer>),
}

/// How jobs are done: on threads, for real; or written down and left undone, for the tests, which then
/// know exactly what the window asked for without anything leaving the machine.
pub enum Effects {
    Threads {
        ctx: egui::Context,
        places: Places,
    },
    #[cfg_attr(not(test), allow(dead_code))]
    Recorded(Vec<Job>),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Mode {
    SignIn,
    Create,
}

/// The form, and whatever it last had to say.
pub struct SignInForm {
    pub mode: Mode,
    pub email: String,
    pub password: String,
    pub name: String,
    pub busy: bool,
    pub problem: Option<String>,
    /// Why the form is showing, when it is not the person's own doing: a sign-in that ran out.
    pub notice: Option<String>,
}

impl SignInForm {
    fn new(email: String, notice: Option<String>) -> SignInForm {
        SignInForm {
            mode: Mode::SignIn,
            email,
            password: String::new(),
            name: String::new(),
            busy: false,
            problem: None,
            notice,
        }
    }
}

/// Figures for one range, and when they were fetched.
pub struct View {
    pub stats: Stats,
    pub fetched_at: DateTime<Utc>,
    /// Read from the cache at launch rather than fetched since; shown, but fetched again.
    pub from_cache: bool,
}

/// Signed in.
pub struct Board {
    pub token: String,
    pub user: Option<User>,
    pub range: Range,
    pub views: HashMap<Range, View>,
    pub pending: HashSet<Range>,
    pub problems: HashMap<Range, Problem>,
    /// Where the token is kept, when that is somewhere worth mentioning.
    pub kept_note: Option<String>,
}

pub enum Screen {
    SignIn(SignInForm),
    Board(Box<Board>),
}

/// What the person asked for, gathered while drawing and done once the frame is drawn.
#[derive(Debug, Clone, PartialEq)]
pub enum Action {
    Submit,
    SwitchMode(Mode),
    Range(Range),
    Refresh,
    SignOut,
}

pub struct App {
    pub service: String,
    places: Places,
    pub zone: Zone,
    /// A fixed "now", for pictures of the window that should not change with the day they are drawn on.
    pub now: Option<DateTime<Utc>>,
    effects: Effects,
    sender: Sender<Reply>,
    replies: Receiver<Reply>,
    pub screen: Screen,
    pub settings: Settings,
    pub newer: Option<Newer>,
    mark: egui::TextureHandle,
    /// Which sign-in this is; see [Reply].
    epoch: u64,
}

impl App {
    pub fn new(
        ctx: &egui::Context,
        service: String,
        places: Places,
        start: Start,
        effects: Effects,
    ) -> App {
        let (sender, replies) = channel();
        let mark = ctx.load_texture(
            "mark",
            egui::ColorImage::from_rgba_unmultiplied([MARK_SIDE, MARK_SIDE], MARK),
            egui::TextureOptions::LINEAR,
        );
        let settings = start.settings;
        let mut app = App {
            service,
            places,
            zone: Zone::Local,
            now: None,
            effects,
            sender,
            replies,
            screen: Screen::SignIn(SignInForm::new(settings.email.clone(), None)),
            settings,
            newer: None,
            mark,
            epoch: 0,
        };
        if let Some((token, kept)) = start.token {
            let user = app.settings.user_for(&app.service).cloned();
            app.open_board(token, user, kept.note());
            if let Screen::Board(board) = &app.screen {
                let token = board.token.clone();
                app.run(Job::Me { token });
            }
        }
        if std::env::var_os("NOCTORIUM_NO_UPDATE_CHECK").is_none() {
            app.run(Job::CheckForUpdate);
        }
        app
    }

    pub fn now(&self) -> DateTime<Utc> {
        self.now.unwrap_or_else(Utc::now)
    }

    pub fn mark(&self) -> &egui::TextureHandle {
        &self.mark
    }

    /// The jobs asked for so far, when they are being written down rather than done.
    #[cfg(test)]
    pub fn jobs(&self) -> &[Job] {
        match &self.effects {
            Effects::Recorded(jobs) => jobs,
            Effects::Threads { .. } => &[],
        }
    }

    fn run(&mut self, job: Job) {
        let (ctx, places) = match &mut self.effects {
            Effects::Recorded(jobs) => {
                jobs.push(job);
                return;
            }
            Effects::Threads { ctx, places } => (ctx.clone(), places.clone()),
        };
        let sender = self.sender.clone();
        let service = self.service.clone();
        let epoch = self.epoch;
        std::thread::spawn(move || {
            let client = Client::new(&service);
            let reply = match job {
                Job::SignIn { email, password } => Some(Reply::Session(
                    epoch,
                    client
                        .sign_in(&email, &password)
                        .map(|s| keep(&places, &service, s)),
                )),
                Job::SignUp {
                    email,
                    password,
                    name,
                } => Some(Reply::Session(
                    epoch,
                    client
                        .sign_up(&email, &password, &name)
                        .map(|s| keep(&places, &service, s)),
                )),
                Job::Me { token } => Some(Reply::Me(epoch, client.me(&token))),
                Job::Stats {
                    token,
                    range,
                    tz,
                    user,
                } => {
                    let result = client
                        .stats(&token, range, tz)
                        .map(|stats| (stats, Utc::now()));
                    if let (Ok((stats, fetched_at)), Some(user_id)) = (&result, user) {
                        let cached = Cached {
                            service: service.clone(),
                            user_id,
                            tz,
                            fetched_at: *fetched_at,
                            stats: stats.clone(),
                        };
                        let _ = store::save_cached(&places, range, &cached);
                    }
                    Some(Reply::Stats(epoch, range, result))
                }
                Job::SignOut => {
                    let _held = KEEPING
                        .lock()
                        .unwrap_or_else(|poisoned| poisoned.into_inner());
                    store::sign_out(&places, &service);
                    None
                }
                Job::CheckForUpdate => Some(Reply::Newer(update::check(crate::version()))),
            };
            if let Some(reply) = reply {
                let _ = sender.send(reply);
                ctx.request_repaint();
            }
        });
    }

    /// Opens the dashboard for [token], with whatever figures were cached for this listener, and asks for
    /// the range they were last looking at.
    fn open_board(&mut self, token: String, user: Option<User>, kept_note: Option<String>) {
        self.epoch += 1;
        let range = self.settings.range;
        let mut board = Board {
            token,
            user,
            range,
            views: HashMap::new(),
            pending: HashSet::new(),
            problems: HashMap::new(),
            kept_note,
        };
        let tz = self.tz();
        load_cached(&mut board, &self.places, &self.service, tz, range);
        self.screen = Screen::Board(Box::new(board));
        self.fetch(range);
    }

    /// The listener's offset from UTC now, which every request and every cached answer is counted on.
    fn tz(&self) -> i32 {
        self.zone.offset_minutes(self.now())
    }

    /// Asks for [range], unless it has been asked for already and the answer is not back.
    fn fetch(&mut self, range: Range) {
        let tz = self.tz();
        let Screen::Board(board) = &mut self.screen else {
            return;
        };
        if !board.pending.insert(range) {
            return;
        }
        let job = Job::Stats {
            token: board.token.clone(),
            range,
            tz,
            user: board.user.as_ref().map(|u| u.id),
        };
        self.run(job);
    }

    /// Back to the form, forgetting the sign-in. [notice] says why, when it was not the person's choice.
    fn sign_out(&mut self, notice: Option<String>) {
        self.epoch += 1;
        self.run(Job::SignOut);
        self.settings.user = None;
        let _ = self.settings.save(&self.places);
        self.screen = Screen::SignIn(SignInForm::new(self.settings.email.clone(), notice));
    }

    fn receive(&mut self) {
        while let Ok(reply) = self.replies.try_recv() {
            match reply {
                Reply::Newer(newer) => self.newer = newer,
                Reply::Session(epoch, result) if epoch == self.epoch => self.signed_in(result),
                Reply::Me(epoch, result) if epoch == self.epoch => match result {
                    Ok(user) => {
                        self.settings.user = Some(user.clone());
                        self.settings.service = self.service.clone();
                        let _ = self.settings.save(&self.places);
                        let tz = self.tz();
                        if let Screen::Board(board) = &mut self.screen {
                            board.user = Some(user);
                            // Without knowing whose they were, no cached figures could be shown; now they can.
                            let range = board.range;
                            load_cached(board, &self.places, &self.service, tz, range);
                        }
                    }
                    Err(Problem::SignedOut) => self.sign_out(Some(Problem::SignedOut.message())),
                    // Offline, or the service struggling: the figures will say so, and the name can wait.
                    Err(_) => {}
                },
                Reply::Stats(epoch, range, result) if epoch == self.epoch => {
                    self.got_stats(range, result)
                }
                // From an earlier sign-in.
                _ => {}
            }
        }
    }

    fn signed_in(&mut self, result: Result<(Session, Option<String>), Problem>) {
        match result {
            Ok((session, kept_note)) => {
                self.settings.signed_in(&session, &self.service);
                let _ = self.settings.save(&self.places);
                self.open_board(session.token, Some(session.user), kept_note);
            }
            Err(problem) => {
                if let Screen::SignIn(form) = &mut self.screen {
                    form.busy = false;
                    form.problem = Some(problem.message());
                }
            }
        }
    }

    fn got_stats(&mut self, range: Range, result: Result<(Stats, DateTime<Utc>), Problem>) {
        let Screen::Board(board) = &mut self.screen else {
            return;
        };
        board.pending.remove(&range);
        match result {
            Ok((stats, fetched_at)) => {
                board.problems.remove(&range);
                board.views.insert(
                    range,
                    View {
                        stats,
                        fetched_at,
                        from_cache: false,
                    },
                );
            }
            Err(Problem::SignedOut) => self.sign_out(Some(Problem::SignedOut.message())),
            Err(problem) => {
                board.problems.insert(range, problem);
            }
        }
    }

    fn act(&mut self, action: Action) {
        match action {
            Action::Submit => {
                let Screen::SignIn(form) = &mut self.screen else {
                    return;
                };
                if form.busy {
                    return;
                }
                if let Err(problem) = sign_in::check(form) {
                    form.problem = Some(problem);
                    return;
                }
                form.busy = true;
                form.problem = None;
                form.notice = None;
                let job = match form.mode {
                    Mode::SignIn => Job::SignIn {
                        email: form.email.trim().to_string(),
                        password: form.password.clone(),
                    },
                    Mode::Create => Job::SignUp {
                        email: form.email.trim().to_string(),
                        password: form.password.clone(),
                        name: form.name.trim().to_string(),
                    },
                };
                self.settings.email = form.email.trim().to_string();
                self.run(job);
            }
            Action::SwitchMode(mode) => {
                if let Screen::SignIn(form) = &mut self.screen {
                    form.mode = mode;
                    form.problem = None;
                }
            }
            Action::Range(range) => {
                self.settings.range = range;
                let _ = self.settings.save(&self.places);
                let (tz, now) = (self.tz(), self.now());
                let Screen::Board(board) = &mut self.screen else {
                    return;
                };
                board.range = range;
                load_cached(board, &self.places, &self.service, tz, range);
                // Fetched again unless it was fetched this session, and recently: switching back and forth
                // between two ranges should not ask the service the same question each time.
                let stale = board.views.get(&range).is_none_or(|v| {
                    v.from_cache || now - v.fetched_at > chrono::Duration::minutes(10)
                });
                if stale {
                    self.fetch(range);
                }
            }
            Action::Refresh => {
                if let Screen::Board(board) = &self.screen {
                    let range = board.range;
                    self.fetch(range);
                }
            }
            Action::SignOut => self.sign_out(None),
        }
    }

    /// One frame: what has come back, then the screen, then what the person did on it.
    pub fn ui(&mut self, ctx: &egui::Context) {
        self.receive();

        let refresh_keys = ctx.input_mut(|i| {
            i.consume_key(egui::Modifiers::NONE, egui::Key::F5)
                || i.consume_key(egui::Modifiers::COMMAND, egui::Key::R)
        });

        let mut actions = Vec::new();
        if refresh_keys && matches!(self.screen, Screen::Board(_)) {
            actions.push(Action::Refresh);
        }
        egui::CentralPanel::default()
            .frame(egui::Frame::new().fill(theme::PAGE))
            .show(ctx, |ui| match &self.screen {
                Screen::SignIn(_) => sign_in::show(ui, self, &mut actions),
                Screen::Board(_) => dashboard::show(ui, self, &mut actions),
            });
        for action in actions {
            self.act(action);
        }
    }

    /// The form, mutably, for the sign-in screen to type into.
    pub fn form_mut(&mut self) -> Option<&mut SignInForm> {
        match &mut self.screen {
            Screen::SignIn(form) => Some(form),
            Screen::Board(_) => None,
        }
    }
}

/// The figures cached for [range], into the board, if it has none for it yet and they are this listener's,
/// from this service, counted on this clock.
fn load_cached(board: &mut Board, places: &Places, service: &str, tz: i32, range: Range) {
    if board.views.contains_key(&range) {
        return;
    }
    let Some(user) = &board.user else { return };
    if let Some(cached) = store::load_cached(places, range, service, user.id, tz) {
        board.views.insert(
            range,
            View {
                stats: cached.stats,
                fetched_at: cached.fetched_at,
                from_cache: true,
            },
        );
    }
}

/// Puts a new sign-in's token away, on the job's own thread, since a keyring can be slow to answer.
fn keep(places: &Places, service: &str, session: Session) -> (Session, Option<String>) {
    let _held = KEEPING
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    let note = match store::save_token(places, service, &session.token) {
        Ok(kept) => kept.note(),
        Err(why) => Some(format!(
            "Your sign-in could not be saved ({why}), so you will be asked again next time."
        )),
    };
    (session, note)
}

impl eframe::App for App {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        self.ui(ctx);
    }

    fn clear_color(&self, _visuals: &egui::Visuals) -> [f32; 4] {
        [0.0, 0.0, 0.0, 1.0]
    }
}
