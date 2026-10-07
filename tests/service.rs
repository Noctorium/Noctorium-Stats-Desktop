//! The client against a real copy of the service, never the live one.
//!
//! Ignored unless asked for, since it needs a service running and an account on it:
//!
//! ```text
//! NOCTORIUM_SERVICE_URL=http://localhost:3000 NOCTORIUM_TEST_EMAIL=... NOCTORIUM_TEST_PASSWORD=... \
//!   cargo test --test service -- --ignored
//! ```
//!
//! Frugal with sign-ins, since the service counts them per address: one that works, one for an address
//! nobody has, and no new accounts at all.

use noctorium_stats::api::{self, Client, Problem};
use noctorium_stats::stats::{Bucket, Range};

struct Account {
    service: String,
    email: String,
    password: String,
}

fn account() -> Option<Account> {
    let service = std::env::var("NOCTORIUM_SERVICE_URL")
        .ok()?
        .trim()
        .trim_end_matches('/')
        .to_string();
    assert_ne!(
        service,
        api::LIVE,
        "these tests sign in and are not for the live service"
    );
    Some(Account {
        service,
        email: std::env::var("NOCTORIUM_TEST_EMAIL").ok()?,
        password: std::env::var("NOCTORIUM_TEST_PASSWORD").ok()?,
    })
}

#[test]
#[ignore = "needs a running service and an account on it"]
fn signed_in_every_range_is_read() {
    let Some(account) = account() else {
        eprintln!(
            "skipped: set NOCTORIUM_SERVICE_URL, NOCTORIUM_TEST_EMAIL and NOCTORIUM_TEST_PASSWORD"
        );
        return;
    };
    let client = Client::new(&account.service);
    let session = client
        .sign_in(&account.email, &account.password)
        .expect("the account signs in");
    assert!(!session.token.is_empty());
    assert_eq!(
        session.user.email.to_lowercase(),
        account.email.to_lowercase()
    );

    let me = client.me(&session.token).expect("the token is good");
    assert_eq!(me, session.user);

    for range in Range::ALL {
        let stats = client
            .stats(&session.token, range, 60)
            .unwrap_or_else(|p| panic!("{range:?}: {p:?}"));
        assert_eq!(stats.range, Some(range));
        assert_eq!(stats.hourly.len(), 24);
        assert_eq!(stats.weekdays.len(), 7);
        assert_eq!(
            stats.services().iter().map(|r| r.streams).sum::<u64>(),
            stats.streams
        );
        match range {
            Range::Week => assert_eq!(stats.timeline.points.len(), 7),
            Range::Month => assert_eq!(stats.timeline.points.len(), 30),
            Range::Quarter => assert_eq!(stats.timeline.bucket, Bucket::Day),
            Range::Year | Range::All => {}
        }
    }

    // The streak is the same whichever range was asked for, being about the whole history.
    let week = client.stats(&session.token, Range::Week, 60).unwrap();
    let all = client.stats(&session.token, Range::All, 60).unwrap();
    assert_eq!(week.streak, all.streak);
    assert!(all.streams >= week.streams);
}

#[test]
#[ignore = "needs a running service"]
fn the_service_says_no_in_words() {
    let Some(account) = account() else {
        eprintln!(
            "skipped: set NOCTORIUM_SERVICE_URL, NOCTORIUM_TEST_EMAIL and NOCTORIUM_TEST_PASSWORD"
        );
        return;
    };
    let client = Client::new(&account.service);

    // An address with no account, so no real account's allowance of attempts is spent.
    match client.sign_in("nobody-at-all@noctorium.test", "not the password") {
        Err(Problem::Refused(words)) => assert!(words.contains("do not match"), "{words}"),
        Err(Problem::Throttled(words)) => {
            eprintln!("throttled, which is also an answer in words: {words}")
        }
        other => panic!("{other:?}"),
    }

    // Refused before anything is counted against the address, so it costs nothing to ask.
    assert_eq!(
        client
            .sign_up("someone@noctorium.test", "short", "")
            .unwrap_err(),
        Problem::Refused("Use at least 10 characters.".into())
    );

    assert_eq!(
        client.stats("not-a-token", Range::Week, 0).unwrap_err(),
        Problem::SignedOut
    );
    assert_eq!(client.me("not-a-token").unwrap_err(), Problem::SignedOut);
}
