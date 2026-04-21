//! Letter writing at the Letter Office — a window to the wider world.
//!
//! The player may post letters to a fixed roster of correspondents outside
//! the parish. Each correspondent has a travel delay (in game-hours). When
//! enough game-time has elapsed, the reply arrives at the Letter Office
//! and can be fetched with `/mail`.
//!
//! Reply bodies are drawn from hand-written seasonal templates keyed by a
//! deterministic hash of `(correspondent, send_time)` so play-tests are
//! reproducible and the feature works with no LLM attached.

use chrono::{DateTime, Duration, Utc};
use serde::{Deserialize, Serialize};

use parish_types::time::Season;

/// A person the player may correspond with outside the parish.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Correspondent {
    /// Machine id used in `/post <id>`; lowercase, no spaces.
    pub id: &'static str,
    /// Display name shown in listings and letter headers.
    pub name: &'static str,
    /// Short relationship descriptor, e.g. "sister in Dublin".
    pub place: &'static str,
    /// Round-trip delay in game-hours from post to reply arrival.
    pub delay_hours: i64,
    /// One-line flavour shown when the letter is posted.
    pub send_blurb: &'static str,
}

/// The fixed roster of correspondents for the demo.
///
/// Hand-picked to offer varied reply latencies (a same-day courier, a few
/// days by mail coach, weeks by packet-ship) and varied tonal registers
/// (family, formal, clerical, emigrant).
pub const CORRESPONDENTS: &[Correspondent] = &[
    Correspondent {
        id: "maire",
        name: "Máire",
        place: "your sister, in Dublin",
        delay_hours: 60,
        send_blurb: "You seal the letter with a daub of candle-wax and slide it into the Dublin bag.",
    },
    Correspondent {
        id: "aodh",
        name: "Aodh",
        place: "your cousin, in Galway",
        delay_hours: 30,
        send_blurb: "A drover bound for Galway promises to carry the letter as far as Athenry himself.",
    },
    Correspondent {
        id: "brown",
        name: "Mr. Brown",
        place: "the attorney, in Athlone",
        delay_hours: 14,
        send_blurb: "The postmaster takes the letter with a nod; Athlone is only half a day's ride.",
    },
    Correspondent {
        id: "gerald",
        name: "Father Gerald",
        place: "the priest, in Tuam",
        delay_hours: 22,
        send_blurb: "You fold the letter thrice and entrust it to the curate riding west at dawn.",
    },
    Correspondent {
        id: "peadar",
        name: "Uncle Peadar",
        place: "your uncle, emigrated to Boston",
        delay_hours: 900,
        send_blurb: "You hand the letter over for the Liverpool packet; it'll be weeks before Boston sees it.",
    },
];

/// Looks up a correspondent by its machine id (case-insensitive).
pub fn find_correspondent(id: &str) -> Option<&'static Correspondent> {
    let lower = id.to_lowercase();
    CORRESPONDENTS.iter().find(|c| c.id == lower)
}

/// A letter posted by the player that is either in transit or awaiting pickup.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Letter {
    /// Stable, monotonic id assigned at post time.
    pub id: u32,
    /// Correspondent id — matches [`Correspondent::id`].
    pub correspondent_id: String,
    /// Short human-readable description the player supplied, e.g. `"news"`.
    pub topic: String,
    /// Game time at which the letter was posted.
    pub sent_at: DateTime<Utc>,
    /// Game time the reply becomes available at the Letter Office.
    pub reply_at: DateTime<Utc>,
    /// `true` once the player has fetched and read the reply.
    #[serde(default)]
    pub read: bool,
}

impl Letter {
    /// Returns `true` if the reply has arrived by `now` and has not yet been read.
    pub fn is_waiting(&self, now: DateTime<Utc>) -> bool {
        !self.read && now >= self.reply_at
    }

    /// Returns `true` if the letter is still in transit at `now`.
    pub fn is_in_transit(&self, now: DateTime<Utc>) -> bool {
        !self.read && now < self.reply_at
    }
}

/// The player's letter-book: every letter ever posted, by order of posting.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct LetterBook {
    /// All letters, oldest first.
    #[serde(default)]
    pub letters: Vec<Letter>,
    /// Monotonic id counter for the next posted letter.
    #[serde(default)]
    next_id: u32,
}

impl LetterBook {
    /// Creates an empty letter-book.
    pub fn new() -> Self {
        Self::default()
    }

    /// Posts a new letter and returns the scheduled arrival time.
    ///
    /// The arrival time is `now + correspondent.delay_hours`, i.e. a fixed
    /// round-trip. Deterministic by design so play-tests and the snapshot
    /// restore are bit-for-bit stable.
    pub fn post(&mut self, correspondent: &Correspondent, topic: &str, now: DateTime<Utc>) -> u32 {
        let id = self.next_id + 1;
        self.next_id = id;
        let reply_at = now + Duration::hours(correspondent.delay_hours);
        self.letters.push(Letter {
            id,
            correspondent_id: correspondent.id.to_string(),
            topic: topic.to_string(),
            sent_at: now,
            reply_at,
            read: false,
        });
        id
    }

    /// Returns every letter whose reply has arrived but has not yet been read.
    pub fn waiting(&self, now: DateTime<Utc>) -> Vec<&Letter> {
        self.letters.iter().filter(|l| l.is_waiting(now)).collect()
    }

    /// Returns every letter still in transit at `now`.
    pub fn in_transit(&self, now: DateTime<Utc>) -> Vec<&Letter> {
        self.letters
            .iter()
            .filter(|l| l.is_in_transit(now))
            .collect()
    }

    /// Marks every currently-waiting letter as read and returns the rendered
    /// reply text for each, in posting order.
    ///
    /// The `season` and the player's elapsed delay are woven into the
    /// rendered text so the reply feels situated in the story.
    pub fn collect_waiting(&mut self, now: DateTime<Utc>, season: Season) -> Vec<ReadLetter> {
        let mut out = Vec::new();
        for letter in self.letters.iter_mut() {
            if !letter.read && now >= letter.reply_at {
                letter.read = true;
                if let Some(correspondent) = find_correspondent(&letter.correspondent_id) {
                    out.push(ReadLetter {
                        id: letter.id,
                        correspondent,
                        topic: letter.topic.clone(),
                        sent_at: letter.sent_at,
                        reply_at: letter.reply_at,
                        body: render_reply_body(correspondent, &letter.topic, letter.id, season),
                    });
                }
            }
        }
        out
    }

    /// Returns the total number of letters ever posted.
    pub fn posted_count(&self) -> usize {
        self.letters.len()
    }
}

/// A rendered, player-readable letter — the fully-baked output of
/// [`LetterBook::collect_waiting`].
#[derive(Debug, Clone)]
pub struct ReadLetter {
    pub id: u32,
    pub correspondent: &'static Correspondent,
    pub topic: String,
    pub sent_at: DateTime<Utc>,
    pub reply_at: DateTime<Utc>,
    pub body: String,
}

/// Renders the reply body for a given correspondent / topic / season.
///
/// Each correspondent has a small table of hand-written seasonal paragraphs.
/// Selection is deterministic — `(letter_id mod N)` — so a given letter always
/// yields the same reply, which keeps play-test logs reproducible and makes
/// snapshot restore trivially correct.
fn render_reply_body(
    correspondent: &Correspondent,
    topic: &str,
    letter_id: u32,
    season: Season,
) -> String {
    let bank = reply_bank(correspondent.id, season);
    let choice = bank[(letter_id as usize) % bank.len()];
    let topic_line = if topic.is_empty() {
        String::new()
    } else {
        format!(" You wrote of {topic}, and I've turned the matter over more than once.")
    };
    format!("{choice}{topic_line}\n\n— {}", correspondent.name)
}

/// Seasonal reply bank, keyed by correspondent id.
///
/// Tables are intentionally short (3 entries each) so that within a
/// reasonable play-test the player sees the full variety. Prose is written
/// plain-spoken, in a register loosely consistent with 1820s rural Ireland —
/// no anachronistic idiom, no quest-giver pleasantries.
fn reply_bank(id: &str, season: Season) -> &'static [&'static str] {
    match (id, season) {
        ("maire", Season::Spring) => &[
            "Dublin is all smoke and bluster this spring, but the crocus is up along the canal and I think of home.",
            "The lodgings above the apothecary are draughty but cheap. Write me about the lambs — I miss the noise of them.",
            "A letter from Aunt Ellen at last: she is well. The city is no place for her, she says, and I cannot disagree.",
        ],
        ("maire", Season::Summer) => &[
            "The heat in the city is something awful — the river stinks and the gentry are all fled to the sea.",
            "Mr. Halpin has offered me work copying for the courts. It will keep me in tea and candles through the autumn.",
            "I walked out to Rathfarnham on Sunday for the air. It nearly made a Christian of me.",
        ],
        ("maire", Season::Autumn) => &[
            "The evenings are drawing in and the cough is back in my chest. Don't fret — it always comes with October.",
            "Tell Father the oats fetched a fair price at Smithfield — better than last year, in any case.",
            "There is talk everywhere of O'Connell and the Catholic Board. The pubs are thick with it.",
        ],
        ("maire", Season::Winter) => &[
            "The Liffey half froze last week, a thing I'd never seen. Children skated on the canal at Portobello.",
            "I have knitted you a muffler but will not post it till the coaches run safely again — the roads west are treacherous.",
            "A midnight Mass in Marlborough Street, and I thought of our own chapel. Christmas blessings to you all.",
        ],

        ("aodh", Season::Spring) => &[
            "The potatoes are in and the field is black with crows. Tell your father the new spade held up.",
            "A fair at Loughrea next week — I may see you there if the roads dry out.",
            "Brigid's girl was married Shrovetide. A quiet match, but a good one.",
        ],
        ("aodh", Season::Summer) => &[
            "The hay is cut and standing in cocks. A wet week now would ruin us, but the glass holds steady.",
            "We lost a heifer to the bloat — a bad loss. Otherwise the summer has been kind.",
            "Two of the Keane boys have gone for the harvest in England. A terrible thing to watch them walk the road.",
        ],
        ("aodh", Season::Autumn) => &[
            "Michaelmas goose was tough as rope this year. The geese have had a hard summer same as ourselves.",
            "The rents were demanded on the dot. I paid and said nothing. What else is there to say?",
            "A stranger came through asking after the Whiteboys. I told him nothing. Say the same if he comes your way.",
        ],
        ("aodh", Season::Winter) => &[
            "The turf is in and the barn is dry. We will not starve, at any rate.",
            "A wake for old Peg Hanratty — ninety-one and sharp to the last. The house was full three nights running.",
            "Snow on the Sliabh for a fortnight. The children have never seen the like.",
        ],

        ("brown", Season::Spring) | ("brown", Season::Summer) => &[
            "Sir, — I have lodged your enquiry with the Recorder's clerk. A reply may be expected within the month. Yours faithfully, E. Brown.",
            "Sir, — Touching the matter of the disputed boundary: the surveyor's report favours your interest. Await my full opinion by next post. Yours, E. Brown.",
            "Sir, — The fee is three shillings sixpence and a glass of brandy when next you are in Athlone. Yours faithfully, E. Brown.",
        ],
        ("brown", Season::Autumn) | ("brown", Season::Winter) => &[
            "Sir, — The assizes sit the second Monday. I advise you attend in person; written depositions seldom carry the day. Yours, E. Brown.",
            "Sir, — I have taken the liberty of writing to Dublin on your behalf. Expect no swift answer: the Four Courts are a slow beast. Yours, E. Brown.",
            "Sir, — The magistrate is a reasonable man if approached sober and before noon. Govern yourself accordingly. Yours, E. Brown.",
        ],

        ("gerald", _) => &[
            "Dear friend, — I had your letter at Vespers and read it twice. There is grace in plain speech; keep to it. Fr. G.",
            "Dear friend, — The bishop visits in the autumn. Say a word for us all — it costs nothing and may do some good. Fr. G.",
            "Dear friend, — I think often of Kilteevan and of the rain on the chapel roof. Pray for us in Tuam; we pray for you. Fr. G.",
        ],

        ("peadar", _) => &[
            "Dear nephew, — The ship was sixty-three days at sea and the youngest took the fever, but we are all landed and well. Work is to be had on the waterfront if a man will take it.",
            "Dear nephew, — Boston is loud as a fair-day and twice as strange. The Irish keep to one quarter and the Yankees to another and the two scarcely cross. I send two dollars, against the rent.",
            "Dear nephew, — There is no going back — not for me — but I will not let you be forgotten here. Write again. Tell me of the land and the weather and who is living and who is not.",
        ],

        _ => &["Your letter reached me and I am grateful for it. Write again when you can."],
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;

    fn t(hours: i64) -> DateTime<Utc> {
        Utc.with_ymd_and_hms(1820, 3, 20, 8, 0, 0).unwrap() + Duration::hours(hours)
    }

    #[test]
    fn find_correspondent_is_case_insensitive() {
        assert!(find_correspondent("MAIRE").is_some());
        assert!(find_correspondent("Maire").is_some());
        assert!(find_correspondent("nobody").is_none());
    }

    #[test]
    fn post_schedules_reply_at_delay() {
        let mut book = LetterBook::new();
        let aodh = find_correspondent("aodh").unwrap();
        let id = book.post(aodh, "the harvest", t(0));
        assert_eq!(id, 1);
        assert_eq!(book.posted_count(), 1);
        let letter = &book.letters[0];
        assert_eq!(letter.reply_at, t(aodh.delay_hours));
        assert_eq!(letter.topic, "the harvest");
        assert!(!letter.read);
    }

    #[test]
    fn post_ids_are_monotonic() {
        let mut book = LetterBook::new();
        let c = find_correspondent("aodh").unwrap();
        assert_eq!(book.post(c, "", t(0)), 1);
        assert_eq!(book.post(c, "", t(0)), 2);
        assert_eq!(book.post(c, "", t(0)), 3);
    }

    #[test]
    fn letter_waiting_vs_in_transit_by_time() {
        let mut book = LetterBook::new();
        let brown = find_correspondent("brown").unwrap();
        book.post(brown, "land title", t(0));
        // Still in transit just before arrival.
        assert_eq!(book.waiting(t(brown.delay_hours - 1)).len(), 0);
        assert_eq!(book.in_transit(t(brown.delay_hours - 1)).len(), 1);
        // Arrived at the delay threshold.
        assert_eq!(book.waiting(t(brown.delay_hours)).len(), 1);
        assert_eq!(book.in_transit(t(brown.delay_hours)).len(), 0);
    }

    #[test]
    fn collect_waiting_marks_letters_read_and_renders_reply() {
        let mut book = LetterBook::new();
        let c = find_correspondent("aodh").unwrap();
        book.post(c, "the oats", t(0));
        let now = t(c.delay_hours);
        let read = book.collect_waiting(now, Season::Spring);
        assert_eq!(read.len(), 1);
        let letter = &read[0];
        assert!(letter.body.contains("Aodh"));
        assert!(letter.body.contains("the oats"));
        // Second call returns nothing — the letter is now marked read.
        assert!(book.collect_waiting(now, Season::Spring).is_empty());
        assert!(book.letters[0].read);
    }

    #[test]
    fn collect_waiting_leaves_in_transit_untouched() {
        let mut book = LetterBook::new();
        let peadar = find_correspondent("peadar").unwrap();
        let aodh = find_correspondent("aodh").unwrap();
        book.post(peadar, "", t(0)); // 900h — still in transit
        book.post(aodh, "", t(0)); // 30h — will arrive first

        let read = book.collect_waiting(t(30), Season::Spring);
        assert_eq!(read.len(), 1);
        assert_eq!(read[0].correspondent.id, "aodh");
        assert!(!book.letters[0].read);
        assert_eq!(book.in_transit(t(30)).len(), 1);
    }

    #[test]
    fn reply_selection_is_deterministic() {
        let c = find_correspondent("maire").unwrap();
        let a = render_reply_body(c, "news", 7, Season::Autumn);
        let b = render_reply_body(c, "news", 7, Season::Autumn);
        assert_eq!(a, b);
    }

    #[test]
    fn letter_book_serialises_round_trip() {
        let mut book = LetterBook::new();
        let c = find_correspondent("gerald").unwrap();
        book.post(c, "the bishop's visit", t(0));
        book.post(c, "", t(5));
        let _ = book.collect_waiting(t(c.delay_hours + 10), Season::Summer);

        let json = serde_json::to_string(&book).unwrap();
        let restored: LetterBook = serde_json::from_str(&json).unwrap();
        assert_eq!(restored, book);
    }
}
