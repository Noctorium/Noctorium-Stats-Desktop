//! The Noctorium account service: signing in, creating an account, and the listener's statistics.
//!
//! Blocking calls, each made on a thread of its own by the window, which is never kept waiting for one.
//! Whatever goes wrong comes back as a [Problem] in words for the person looking at the window: the service's
//! own when it sent some, and ours when it never answered.

use crate::stats::{Range, Stats};
use serde::{Deserialize, Serialize};
use std::time::Duration;

/// Where the service lives unless told otherwise.
pub const LIVE: &str = "https://stats.noctorium.xyz";

/// Signing in and the account: its own server since 0.39. Used whenever the statistics are the live ones.
pub const ACCOUNT_LIVE: &str = "https://account.noctorium.xyz";

/// The service's address: `NOCTORIUM_SERVICE_URL` when it is set, as the player reads it, for a preview
/// deployment, a service of one's own or a copy running on this machine; the live one otherwise.
pub fn service_url() -> String {
    service_url_from(std::env::var("NOCTORIUM_SERVICE_URL").ok())
}

fn service_url_from(value: Option<String>) -> String {
    value
        .map(|v| v.trim().trim_end_matches('/').to_string())
        .filter(|v| !v.is_empty())
        .unwrap_or_else(|| LIVE.to_string())
}

/// How many songs and artists the top lists hold.
pub const TOP: u32 = 10;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct User {
    pub id: u64,
    pub email: String,
    pub display_name: String,
}

/// A sign-in: the token the service handed out, which lasts ninety days, and whose it is.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Session {
    pub token: String,
    pub user: User,
}

/// A browser sign-in, as the service starts it: the code to show, the page to open, and the secret that collects
/// the token once the listener has allowed it there.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DeviceStart {
    pub code: String,
    pub poll_token: String,
    pub url: String,
    #[serde(default = "ten_minutes")]
    pub expires_in: u64,
    #[serde(default = "three_seconds")]
    pub interval: u64,
}

fn ten_minutes() -> u64 {
    600
}

fn three_seconds() -> u64 {
    3
}

/// Why something did not work, in a form the window can say.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Problem {
    /// The token was refused: it ran out, or the account is gone. The only cure is to sign in again.
    SignedOut,
    /// Too many attempts from here, in the service's words, which say how long to wait.
    Throttled(String),
    /// The service said no to what was sent -- a wrong password, an address already taken -- in its words.
    Refused(String),
    /// Nothing answered: no network, the service unreachable, or a connection that dropped part of the way.
    Offline,
    /// The service answered with an error of its own.
    Server(u16),
    /// The service answered, but with something this version cannot read.
    Unreadable,
    /// The password was right, and the account's two-step verification asks for a code before the token.
    Verify(Challenge),
}

/// Two-step verification, asked for after the password: the ways the account can be verified ("totp" for an
/// authenticator app, "telegram" for a code the login bot sends), whether strict mode needs every one of them, what is
/// proved already, and the service's words about the last try.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Challenge {
    pub ticket: String,
    pub methods: Vec<String>,
    pub strict: bool,
    pub done: Vec<String>,
    pub message: Option<String>,
}

impl Challenge {
    /// What is still to be given: in strict mode every way not yet proved, otherwise any one of them.
    pub fn still_needed(&self) -> Vec<String> {
        if self.strict {
            self.methods
                .iter()
                .filter(|m| !self.done.contains(m))
                .cloned()
                .collect()
        } else {
            self.methods.clone()
        }
    }

    fn from_body(body: &str) -> Option<Challenge> {
        let v: serde_json::Value = serde_json::from_str(body).ok()?;
        let list = |name: &str| -> Vec<String> {
            v[name]
                .as_array()
                .map(|a| {
                    a.iter()
                        .filter_map(|x| x.as_str().map(String::from))
                        .collect()
                })
                .unwrap_or_default()
        };
        Some(Challenge {
            ticket: v["challenge"]
                .as_str()
                .filter(|t| !t.is_empty())?
                .to_string(),
            methods: list("methods"),
            strict: v["strict"].as_bool() == Some(true),
            done: list("done"),
            message: v["error"].as_str().map(String::from),
        })
    }
}

impl Problem {
    pub fn message(&self) -> String {
        match self {
            Problem::SignedOut => "Your sign-in has run out. Sign in again to see your statistics.".into(),
            Problem::Throttled(words) | Problem::Refused(words) => words.clone(),
            Problem::Verify(asked) => asked.message.clone().unwrap_or_else(|| "Enter a code to finish signing in.".into()),
            Problem::Offline => {
                "The Noctorium service could not be reached. Check the connection, then try again.".into()
            }
            Problem::Server(status) => {
                format!("The Noctorium service had a problem of its own ({status}). Try again in a minute.")
            }
            Problem::Unreadable => {
                "The Noctorium service sent an answer this version cannot read. An update may fix it.".into()
            }
        }
    }

    /// What a status code means, with the service's own words where it sent some. [signing_in] is whether
    /// the request was a sign-in or a new account, where a 401 is a wrong password rather than a token that
    /// has run out.
    pub fn from_status(status: u16, body: &str, signing_in: bool) -> Problem {
        #[derive(Deserialize)]
        struct Said {
            error: String,
        }
        let said = serde_json::from_str::<Said>(body)
            .ok()
            .map(|s| s.error.trim().to_string())
            .filter(|s| !s.is_empty());
        match status {
            401 if !signing_in => Problem::SignedOut,
            429 => Problem::Throttled(said.unwrap_or_else(|| {
                "Too many attempts. Wait a few minutes, then try again.".into()
            })),
            400..=499 => match said {
                Some(words) => Problem::Refused(words),
                None if status == 401 => Problem::Refused(
                    "That email address and password do not match an account.".into(),
                ),
                None => Problem::Server(status),
            },
            _ => Problem::Server(status),
        }
    }

    /// What a failure to get any answer at all means.
    pub fn from_transport(kind: ureq::ErrorKind) -> Problem {
        use ureq::ErrorKind::*;
        match kind {
            Dns | ConnectionFailed | Io | ProxyConnect => Problem::Offline,
            // A gateway or a captive portal answering for the service, with something that is not HTTP.
            BadStatus | BadHeader | TooManyRedirects => Problem::Unreadable,
            _ => Problem::Offline,
        }
    }
}

/// The address of the statistics for [range], counted on a clock [tz] minutes east of UTC.
pub fn stats_url(base: &str, range: Range, tz: i32, limit: u32) -> String {
    format!(
        "{base}/api/stats?range={}&tz={tz}&limit={limit}",
        range.code()
    )
}

#[derive(Clone)]
pub struct Client {
    base: String,
    /// Where sign-in is asked: the account server with the live statistics, or [base] for a server of one's own.
    account: String,
    agent: ureq::Agent,
}

impl Client {
    pub fn new(base: &str) -> Client {
        let agent = ureq::AgentBuilder::new()
            .timeout_connect(Duration::from_secs(10))
            // The service counts everything from the plays each time, and a cold start on Vercel adds a few
            // seconds to the first request; a minute is generous for both.
            .timeout(Duration::from_secs(60))
            .user_agent(&format!(
                "NoctoriumStats/{} ({})",
                crate::version(),
                std::env::consts::OS
            ))
            .build();
        let base = base.trim_end_matches('/').to_string();
        let account = if base == LIVE {
            ACCOUNT_LIVE.to_string()
        } else {
            base.clone()
        };
        Client {
            base,
            account,
            agent,
        }
    }

    pub fn base(&self) -> &str {
        &self.base
    }

    pub fn sign_in(&self, email: &str, password: &str) -> Result<Session, Problem> {
        let body = serde_json::json!({ "email": email.trim(), "password": password });
        let request = self.agent.post(&format!("{}/api/auth/login", self.account));
        // The first answer's words are for apps too old to ask for a code: not shown here.
        read(send(request.send_json(body), true).map_err(|p| match p {
            Problem::Verify(asked) => Problem::Verify(Challenge {
                message: None,
                ..asked
            }),
            other => other,
        })?)
    }

    /// The codes for [asked]'s sign-in; whichever are given count.
    pub fn verify(
        &self,
        asked: &Challenge,
        totp: &str,
        telegram: &str,
        backup: &str,
    ) -> Result<Session, Problem> {
        let mut body = serde_json::json!({ "challenge": asked.ticket });
        for (name, code) in [("totp", totp), ("telegram", telegram), ("backup", backup)] {
            if !code.trim().is_empty() {
                body[name] = code.trim().into();
            }
        }
        let request = self
            .agent
            .post(&format!("{}/api/auth/challenge/verify", self.account));
        read(send(request.send_json(body), true)?)
    }

    /// Asks the login bot to send [asked]'s code on Telegram.
    pub fn send_telegram_code(&self, asked: &Challenge) -> Result<(), Problem> {
        let request = self
            .agent
            .post(&format!("{}/api/auth/challenge/telegram", self.account));
        send(
            request.send_json(serde_json::json!({ "challenge": asked.ticket })),
            true,
        )
        .map(|_| ())
    }

    pub fn sign_up(
        &self,
        email: &str,
        password: &str,
        display_name: &str,
    ) -> Result<Session, Problem> {
        let mut body = serde_json::json!({ "email": email.trim(), "password": password });
        if !display_name.trim().is_empty() {
            body["displayName"] = display_name.trim().into();
        }
        let request = self
            .agent
            .post(&format!("{}/api/auth/signup", self.account));
        read(send(request.send_json(body), true)?)
    }

    /// Starts signing in from a browser -- with Discord, Telegram or email -- by a code.
    pub fn start_device(&self) -> Result<DeviceStart, Problem> {
        let request = self
            .agent
            .post(&format!("{}/api/auth/device", self.account));
        read(send(request.send_json(serde_json::json!({})), true)?)
    }

    /// Asks once whether the browser sign-in has been allowed: `None` while it waits.
    pub fn poll_device(&self, poll_token: &str) -> Result<Option<Session>, Problem> {
        let request = self
            .agent
            .get(&format!("{}/api/auth/device/poll", self.account))
            .query("token", poll_token);
        match request.call() {
            Ok(response) if response.status() == 202 => Ok(None),
            result => read(send(result, true)?).map(Some),
        }
    }

    pub fn me(&self, token: &str) -> Result<User, Problem> {
        #[derive(Deserialize)]
        struct Me {
            user: User,
        }
        let request = self
            .agent
            .get(&format!("{}/api/auth/me", self.account))
            .set("Authorization", &bearer(token));
        read::<Me>(send(request.call(), false)?).map(|me| me.user)
    }

    pub fn stats(&self, token: &str, range: Range, tz: i32) -> Result<Stats, Problem> {
        let request = self
            .agent
            .get(&stats_url(&self.base, range, tz, TOP))
            .set("Authorization", &bearer(token));
        let body = send(request.call(), false)?;
        Stats::parse(&body).map_err(|_| Problem::Unreadable)
    }
}

fn bearer(token: &str) -> String {
    format!("Bearer {token}")
}

fn send(result: Result<ureq::Response, ureq::Error>, signing_in: bool) -> Result<String, Problem> {
    match result {
        // A body cut off part of the way is a connection that dropped, not a service that misbehaved.
        Ok(response) => response.into_string().map_err(|_| Problem::Offline),
        Err(ureq::Error::Status(status, response)) => {
            let body = response.into_string().unwrap_or_default();
            if let Some(asked) = Challenge::from_body(&body) {
                return Err(Problem::Verify(asked));
            }
            Err(Problem::from_status(status, &body, signing_in))
        }
        Err(ureq::Error::Transport(transport)) => Err(Problem::from_transport(transport.kind())),
    }
}

fn read<T: for<'de> Deserialize<'de>>(body: String) -> Result<T, Problem> {
    serde_json::from_str(&body).map_err(|_| Problem::Unreadable)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_service_is_the_live_one_unless_told() {
        assert_eq!(service_url_from(None), LIVE);
        assert_eq!(service_url_from(Some("   ".into())), LIVE);
        assert_eq!(
            service_url_from(Some(" http://localhost:3000/ ".into())),
            "http://localhost:3000"
        );
    }

    #[test]
    fn statistics_are_asked_for_by_range_and_clock() {
        assert_eq!(
            stats_url("http://localhost:3000", Range::Week, 60, 10),
            "http://localhost:3000/api/stats?range=7d&tz=60&limit=10"
        );
        assert_eq!(
            stats_url(LIVE, Range::All, -300, 10),
            "https://stats.noctorium.xyz/api/stats?range=all&tz=-300&limit=10"
        );
        assert!(stats_url(LIVE, Range::Year, 0, TOP).contains("range=365d&tz=0"));
        assert!(stats_url(LIVE, Range::Quarter, 330, TOP).contains("range=90d&tz=330"));
        assert!(stats_url(LIVE, Range::Month, 0, TOP).contains("range=30d"));
    }

    #[test]
    fn a_browser_sign_in_reads_as_the_service_sends_it() {
        let start: DeviceStart = serde_json::from_str(
            r#"{"code":"ABCD-EFGH","pollToken":"p","url":"https://service.noctorium.xyz/device?code=ABCD-EFGH","expiresIn":600,"interval":3}"#,
        )
        .unwrap();
        assert_eq!((start.code.as_str(), start.interval), ("ABCD-EFGH", 3));
        // An account made with Discord may have no email: it comes back empty, and still reads.
        let session: Session = serde_json::from_str(
            r#"{"token":"t","user":{"id":9,"email":"","displayName":"Disco"}}"#,
        )
        .unwrap();
        assert_eq!(session.user.display_name, "Disco");
    }

    #[test]
    fn a_code_is_asked_for_and_strict_needs_what_is_not_done() {
        let asked = Challenge::from_body(r#"{"error":"Strict mode is on: both codes are needed.","challenge":"t1","methods":["totp","telegram"],"strict":true,"done":["totp"]}"#).unwrap();
        assert_eq!(asked.still_needed(), vec!["telegram".to_string()]);
        assert_eq!(
            Problem::Verify(asked).message(),
            "Strict mode is on: both codes are needed."
        );
        assert!(Challenge::from_body(r#"{"error":"no"}"#).is_none());
        assert_eq!(Client::new(LIVE).account, ACCOUNT_LIVE);
        assert_eq!(
            Client::new("http://localhost:3000").account,
            "http://localhost:3000"
        );
    }

    #[test]
    fn a_refused_token_signs_out() {
        assert_eq!(
            Problem::from_status(401, r#"{"error":"Not signed in."}"#, false),
            Problem::SignedOut
        );
    }

    #[test]
    fn a_wrong_password_is_not_a_sign_out() {
        let problem = Problem::from_status(
            401,
            r#"{"error":"That email address and password do not match an account."}"#,
            true,
        );
        assert_eq!(
            problem,
            Problem::Refused("That email address and password do not match an account.".into())
        );
        // And without the service's words, ours say the same.
        assert!(
            matches!(Problem::from_status(401, "", true), Problem::Refused(words) if words.contains("do not match"))
        );
    }

    #[test]
    fn throttling_says_how_long_to_wait() {
        let problem = Problem::from_status(
            429,
            r#"{"error":"Too many attempts. Try again in 15 minutes."}"#,
            true,
        );
        assert_eq!(
            problem.message(),
            "Too many attempts. Try again in 15 minutes."
        );
        // Behind a proxy that swallowed the body, the advice is still there.
        let bare = Problem::from_status(429, "<html>slow down</html>", false);
        assert!(matches!(bare, Problem::Throttled(_)));
        assert!(bare.message().contains("try again"));
    }

    #[test]
    fn the_services_own_words_are_kept() {
        assert_eq!(
            Problem::from_status(
                409,
                r#"{"error":"That email address already has an account."}"#,
                true
            ),
            Problem::Refused("That email address already has an account.".into())
        );
        assert_eq!(
            Problem::from_status(400, r#"{"error":"Use at least 10 characters."}"#, true).message(),
            "Use at least 10 characters."
        );
    }

    #[test]
    fn a_broken_service_is_not_the_listeners_fault() {
        assert_eq!(
            Problem::from_status(500, "Internal Server Error", false),
            Problem::Server(500)
        );
        assert_eq!(
            Problem::from_status(404, "<html>", false),
            Problem::Server(404)
        );
        assert!(Problem::Server(502).message().contains("502"));
    }

    #[test]
    fn no_answer_at_all_is_offline() {
        for kind in [
            ureq::ErrorKind::Dns,
            ureq::ErrorKind::ConnectionFailed,
            ureq::ErrorKind::Io,
        ] {
            assert_eq!(Problem::from_transport(kind), Problem::Offline);
        }
        assert_eq!(
            Problem::from_transport(ureq::ErrorKind::BadStatus),
            Problem::Unreadable
        );
        assert!(Problem::Offline.message().contains("could not be reached"));
    }

    #[test]
    fn nothing_listening_on_the_port_is_offline() {
        // Port 9 on this machine: discard, which nothing runs any more, so the connection is refused at once.
        let client = Client::new("http://127.0.0.1:9");
        assert_eq!(client.stats("token", Range::Week, 0), Err(Problem::Offline));
        assert_eq!(
            client.sign_in("a@b.test", "password").unwrap_err(),
            Problem::Offline
        );
    }

    #[test]
    fn a_session_is_read_as_the_service_sends_it() {
        let session: Session = serde_json::from_str(
            r#"{"token":"abc","user":{"id":7,"email":"robin@example.test","displayName":"Robin Vale"}}"#,
        )
        .unwrap();
        assert_eq!(session.user.display_name, "Robin Vale");
        assert_eq!(session.token, "abc");
    }
}
