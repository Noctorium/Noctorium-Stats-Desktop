//! What the service says about a listener: the reply of `GET /api/stats`, as Noctorium-Service's
//! `src/lib/stats.ts` builds it.
//!
//! Read leniently. Everything but the four totals has a default, so a service older than some part of this
//! reply still gives a dashboard with that part empty rather than no dashboard at all, and a service that
//! learns a new music service names it in a way this version shows as it is rather than refusing the reply.

use chrono::{DateTime, NaiveDate, Utc};
use serde::{Deserialize, Deserializer, Serialize};

/// How far back the figures reach. The service counts days on the listener's own clock, so "7 days" is
/// today and the six before it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Default)]
pub enum Range {
    #[serde(rename = "7d")]
    Week,
    #[default]
    #[serde(rename = "30d")]
    Month,
    #[serde(rename = "90d")]
    Quarter,
    #[serde(rename = "365d")]
    Year,
    #[serde(rename = "all")]
    All,
}

impl Range {
    pub const ALL: [Range; 5] = [
        Range::Week,
        Range::Month,
        Range::Quarter,
        Range::Year,
        Range::All,
    ];

    /// As the service's `range` parameter has it.
    pub fn code(self) -> &'static str {
        match self {
            Range::Week => "7d",
            Range::Month => "30d",
            Range::Quarter => "90d",
            Range::Year => "365d",
            Range::All => "all",
        }
    }

    pub fn from_code(code: &str) -> Option<Range> {
        Range::ALL.into_iter().find(|r| r.code() == code)
    }

    /// As the switcher has it.
    pub fn label(self) -> &'static str {
        match self {
            Range::Week => "7 days",
            Range::Month => "30 days",
            Range::Quarter => "90 days",
            Range::Year => "Year",
            Range::All => "All time",
        }
    }

    /// The days it covers, or none for all of it.
    pub fn days(self) -> Option<i64> {
        match self {
            Range::Week => Some(7),
            Range::Month => Some(30),
            Range::Quarter => Some(90),
            Range::Year => Some(365),
            Range::All => None,
        }
    }
}

/// Where a song was played from.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum Provider {
    YouTubeMusic,
    /// A YouTube video played as music. Shown as YouTube Music, which is where the listener thinks it was.
    YouTubeVideo,
    SoundCloud,
    Bandcamp,
    Spotify,
    Vk,
    Local,
    /// A service this version does not know yet, by the name the service sent.
    Other(String),
}

impl Provider {
    pub fn from_code(code: &str) -> Provider {
        match code {
            "YOUTUBE_MUSIC" => Provider::YouTubeMusic,
            "YOUTUBE_VIDEO" => Provider::YouTubeVideo,
            "SOUNDCLOUD" => Provider::SoundCloud,
            "BANDCAMP" => Provider::Bandcamp,
            "SPOTIFY" => Provider::Spotify,
            "VK" => Provider::Vk,
            "LOCAL" => Provider::Local,
            other => Provider::Other(other.to_string()),
        }
    }

    pub fn name(&self) -> String {
        match self {
            Provider::YouTubeMusic | Provider::YouTubeVideo => "YouTube Music".into(),
            Provider::SoundCloud => "SoundCloud".into(),
            Provider::Bandcamp => "Bandcamp".into(),
            Provider::Spotify => "Spotify".into(),
            Provider::Vk => "VK".into(),
            Provider::Local => "Your files".into(),
            Provider::Other(code) => title_case(code),
        }
    }

    /// The service's own colour, as the player draws it.
    pub fn colour(&self) -> [u8; 3] {
        match self {
            Provider::YouTubeMusic | Provider::YouTubeVideo => [0xFF, 0x4E, 0x45],
            Provider::SoundCloud => [0xFF, 0x7A, 0x1A],
            Provider::Spotify => [0x1E, 0xD7, 0x60],
            Provider::Bandcamp => [0x62, 0x9A, 0xA9],
            Provider::Vk => [0x4C, 0x9B, 0xFF],
            // Files on the listener's own disk have no brand, so they take Noctorium's own violet.
            Provider::Local => [0xB4, 0x7C, 0xFF],
            Provider::Other(_) => [0x9A, 0x8F, 0xAD],
        }
    }
}

/// SOME_SERVICE as Some Service.
fn title_case(code: &str) -> String {
    code.split('_')
        .filter(|w| !w.is_empty())
        .map(|w| {
            let mut chars = w.chars();
            match chars.next() {
                Some(first) => first
                    .to_uppercase()
                    .chain(chars.flat_map(|c| c.to_lowercase()))
                    .collect(),
                None => String::new(),
            }
        })
        .collect::<Vec<String>>()
        .join(" ")
}

impl<'de> Deserialize<'de> for Provider {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Provider, D::Error> {
        Ok(Provider::from_code(&String::deserialize(deserializer)?))
    }
}

impl Serialize for Provider {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(match self {
            Provider::YouTubeMusic => "YOUTUBE_MUSIC",
            Provider::YouTubeVideo => "YOUTUBE_VIDEO",
            Provider::SoundCloud => "SOUNDCLOUD",
            Provider::Bandcamp => "BANDCAMP",
            Provider::Spotify => "SPOTIFY",
            Provider::Vk => "VK",
            Provider::Local => "LOCAL",
            Provider::Other(code) => code,
        })
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Stats {
    #[serde(default)]
    pub range: Option<Range>,
    #[serde(default)]
    pub from: Option<DateTime<Utc>>,
    pub streams: u64,
    pub unique_tracks: u64,
    pub artists: u64,
    pub ms_played: u64,
    #[serde(default)]
    pub first_play: Option<DateTime<Utc>>,
    #[serde(default)]
    pub last_play: Option<DateTime<Utc>>,
    #[serde(default)]
    pub by_provider: Vec<ProviderShare>,
    #[serde(default)]
    pub top_tracks: Vec<TopTrack>,
    #[serde(default)]
    pub top_artists: Vec<TopArtist>,
    #[serde(default)]
    pub timeline: Timeline,
    #[serde(default)]
    pub hourly: Vec<Hour>,
    #[serde(default)]
    pub weekdays: Vec<Weekday>,
    #[serde(default)]
    pub recent: Vec<Play>,
    #[serde(default)]
    pub streak: Streak,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProviderShare {
    pub provider: Provider,
    pub streams: u64,
    #[serde(default)]
    pub hours: f64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TopTrack {
    pub title: String,
    pub artist: String,
    pub provider: Provider,
    pub streams: u64,
    #[serde(default)]
    pub minutes: u64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TopArtist {
    pub artist: String,
    pub streams: u64,
    #[serde(default)]
    pub minutes: u64,
    #[serde(default)]
    pub tracks: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum Bucket {
    #[default]
    Day,
    Month,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct Timeline {
    pub bucket: Bucket,
    pub points: Vec<Point>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Point {
    /// The day, or the first of the month, on the listener's own calendar.
    pub start: NaiveDate,
    pub streams: u64,
    #[serde(default)]
    pub minutes: u64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Hour {
    /// 0 to 23, on the listener's own clock.
    pub hour: u32,
    pub streams: u64,
    #[serde(default)]
    pub minutes: u64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Weekday {
    /// 1 for Monday to 7 for Sunday.
    pub day: u32,
    pub streams: u64,
    #[serde(default)]
    pub minutes: u64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Play {
    pub title: String,
    pub artist: String,
    pub provider: Provider,
    pub played_at: DateTime<Utc>,
    #[serde(default)]
    pub ms_played: u64,
}

/// Days in a row with something played. About the listener's whole history, whatever the range.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct Streak {
    pub current: u64,
    pub longest: u64,
}

/// One row of the split by service, with the two YouTubes already together.
#[derive(Debug, Clone, PartialEq)]
pub struct ServiceRow {
    pub provider: Provider,
    pub streams: u64,
    pub hours: f64,
    /// Of all the plays in the range, 0 to 1.
    pub share: f64,
}

impl Stats {
    pub fn parse(json: &str) -> Result<Stats, serde_json::Error> {
        serde_json::from_str(json)
    }

    /// Nothing played in this range.
    pub fn is_empty(&self) -> bool {
        self.streams == 0
    }

    /// Nothing played ever: the streak is counted over the listener's whole history, so a longest of none
    /// means there is no history at all, whatever range was asked for.
    pub fn never_played(&self) -> bool {
        self.streams == 0 && self.streak.longest == 0
    }

    /// The split by service, with YouTube videos counted as YouTube Music, as the listener would put it,
    /// most played first.
    pub fn services(&self) -> Vec<ServiceRow> {
        let mut rows: Vec<ServiceRow> = Vec::new();
        for share in &self.by_provider {
            let provider = match &share.provider {
                Provider::YouTubeVideo => Provider::YouTubeMusic,
                other => other.clone(),
            };
            match rows.iter_mut().find(|r| r.provider == provider) {
                Some(row) => {
                    row.streams += share.streams;
                    row.hours += share.hours;
                }
                None => rows.push(ServiceRow {
                    provider,
                    streams: share.streams,
                    hours: share.hours,
                    share: 0.0,
                }),
            }
        }
        let total: u64 = rows.iter().map(|r| r.streams).sum();
        for row in &mut rows {
            row.share = if total == 0 {
                0.0
            } else {
                row.streams as f64 / total as f64
            };
        }
        rows.sort_by_key(|row| std::cmp::Reverse(row.streams));
        rows
    }

    /// All 24 hours, whatever the service sent, so the clock always has a full face.
    pub fn hours_of_day(&self) -> [Hour; 24] {
        std::array::from_fn(|hour| {
            self.hourly
                .iter()
                .find(|h| h.hour as usize == hour)
                .cloned()
                .unwrap_or(Hour {
                    hour: hour as u32,
                    streams: 0,
                    minutes: 0,
                })
        })
    }

    /// Monday to Sunday, whatever the service sent.
    pub fn days_of_week(&self) -> [Weekday; 7] {
        std::array::from_fn(|index| {
            let day = index as u32 + 1;
            self.weekdays
                .iter()
                .find(|d| d.day == day)
                .cloned()
                .unwrap_or(Weekday {
                    day,
                    streams: 0,
                    minutes: 0,
                })
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fixture(name: &str) -> Stats {
        let path = format!("{}/tests/fixtures/{name}.json", env!("CARGO_MANIFEST_DIR"));
        Stats::parse(&std::fs::read_to_string(&path).unwrap())
            .unwrap_or_else(|e| panic!("{path}: {e}"))
    }

    #[test]
    fn every_captured_reply_is_read() {
        for range in Range::ALL {
            let stats = fixture(&format!("listener-{}", range.code()));
            assert_eq!(stats.range, Some(range));
            assert!(stats.streams > 0);
            assert_eq!(stats.hours_of_day().len(), 24);
            assert_eq!(stats.hourly.len(), 24);
            assert_eq!(stats.weekdays.len(), 7);
            assert!(stats.recent.len() <= 30);
            assert_eq!(stats.from.is_none(), range == Range::All);
        }
    }

    #[test]
    fn a_week_is_read_field_by_field() {
        let stats = fixture("listener-7d");
        assert_eq!(stats.streams, 31);
        assert_eq!(stats.unique_tracks, 18);
        assert_eq!(stats.artists, 11);
        assert_eq!(stats.ms_played, 5_835_196);
        assert_eq!(
            stats.from.unwrap().to_rfc3339(),
            "2026-09-30T23:00:00+00:00"
        );
        assert_eq!(stats.timeline.bucket, Bucket::Day);
        assert_eq!(stats.timeline.points.len(), 7);
        assert_eq!(
            stats.timeline.points[0].start,
            NaiveDate::from_ymd_opt(2026, 10, 1).unwrap()
        );
        assert_eq!(stats.top_tracks[0].title, "Harbour Lights");
        assert_eq!(stats.top_tracks[0].provider, Provider::YouTubeMusic);
        assert_eq!(stats.top_tracks[0].streams, 5);
        assert_eq!(stats.top_tracks[0].minutes, 21);
        assert_eq!(
            stats.streak,
            Streak {
                current: 12,
                longest: 21
            }
        );
        assert_eq!(stats.recent[0].provider, Provider::SoundCloud);
        assert_eq!(
            stats.recent[0].played_at.to_rfc3339(),
            "2026-10-07T09:00:00.405+00:00"
        );
    }

    #[test]
    fn longer_ranges_come_by_month() {
        assert_eq!(fixture("listener-365d").timeline.bucket, Bucket::Month);
        let all = fixture("listener-all");
        assert_eq!(all.timeline.bucket, Bucket::Month);
        assert_eq!(
            all.timeline.points.first().unwrap().start,
            NaiveDate::from_ymd_opt(2025, 8, 1).unwrap()
        );
        assert_eq!(fixture("listener-90d").timeline.points.len(), 90);
    }

    #[test]
    fn an_empty_account_is_read_and_known_for_one() {
        let stats = fixture("empty-all");
        assert!(stats.is_empty());
        assert!(stats.never_played());
        assert!(stats.services().is_empty());
        assert!(stats.timeline.points.is_empty());
        assert_eq!(stats.first_play, None);
        assert!(stats.hours_of_day().iter().all(|h| h.streams == 0));
    }

    #[test]
    fn an_empty_range_is_not_an_empty_history() {
        let mut stats = fixture("listener-7d");
        stats.streams = 0;
        assert!(stats.is_empty());
        assert!(!stats.never_played());
    }

    #[test]
    fn the_split_by_service_adds_up() {
        let stats = fixture("listener-all");
        let rows = stats.services();
        assert_eq!(rows[0].provider, Provider::YouTubeMusic);
        assert_eq!(rows.iter().map(|r| r.streams).sum::<u64>(), stats.streams);
        assert!((rows.iter().map(|r| r.share).sum::<f64>() - 1.0).abs() < 1e-9);
    }

    #[test]
    fn youtube_videos_count_as_youtube_music() {
        let stats = Stats::parse(
            r#"{"streams":5,"uniqueTracks":3,"artists":2,"msPlayed":1000,
                "byProvider":[{"provider":"SPOTIFY","streams":3,"hours":0.1},
                              {"provider":"YOUTUBE_MUSIC","streams":1,"hours":0.2},
                              {"provider":"YOUTUBE_VIDEO","streams":1,"hours":0.3}]}"#,
        )
        .unwrap();
        let rows = stats.services();
        assert_eq!(rows.len(), 2);
        assert_eq!(rows[0].provider, Provider::Spotify);
        assert_eq!(rows[1].provider, Provider::YouTubeMusic);
        assert_eq!(rows[1].streams, 2);
        assert!((rows[1].hours - 0.5).abs() < 1e-9);
    }

    #[test]
    fn an_older_service_still_gives_a_dashboard() {
        // What the player's account page has always been sent: the totals and nothing else.
        let stats = Stats::parse(
            r#"{"streams":12,"uniqueTracks":4,"artists":3,"msPlayed":3600000,"hours":1}"#,
        )
        .unwrap();
        assert_eq!(stats.streams, 12);
        assert!(stats.timeline.points.is_empty());
        assert_eq!(stats.hours_of_day()[23].streams, 0);
        assert_eq!(stats.days_of_week()[6].day, 7);
        assert_eq!(stats.streak, Streak::default());
    }

    #[test]
    fn a_service_nobody_has_heard_of_is_shown_by_its_name() {
        let provider: Provider = serde_json::from_str(r#""APPLE_MUSIC""#).unwrap();
        assert_eq!(provider, Provider::Other("APPLE_MUSIC".into()));
        assert_eq!(provider.name(), "Apple Music");
        assert_eq!(
            serde_json::to_string(&provider).unwrap(),
            r#""APPLE_MUSIC""#
        );
        assert_eq!(Provider::YouTubeVideo.name(), "YouTube Music");
    }

    #[test]
    fn what_is_cached_reads_back_the_same() {
        let stats = fixture("listener-30d");
        let again = Stats::parse(&serde_json::to_string(&stats).unwrap()).unwrap();
        assert_eq!(stats, again);
    }

    #[test]
    fn ranges_go_both_ways() {
        for range in Range::ALL {
            assert_eq!(Range::from_code(range.code()), Some(range));
        }
        assert_eq!(Range::from_code("12d"), None);
    }
}
