//! Letters from Away — a deterministic mail-coach system.
//!
//! Twice a week (Wednesday and Saturday), the mail coach leaves letters
//! at the Letter Office for parishioners. This module generates those
//! letters on demand from a fixed pool of distant correspondents —
//! emigrant siblings, soldiers abroad, relatives in Dublin and Manchester —
//! seeded purely by the game date so the same day always produces the
//! same letter.
//!
//! The generator is offline (no LLM), pure, and cheap. A backend asks
//! [`letters_between`] for all letters delivered in a game-time window
//! and renders them. See `game-ideas-brainstorm.md` §19 for the design
//! intent — the parish is not an island, and letters are the cheapest
//! way to remind the player of that.
//!
//! Feature-flagged as `letters` (kill-switchable).

use chrono::{DateTime, Datelike, NaiveDate, Utc, Weekday};

/// A distant correspondent with a fixed voice.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Sender {
    /// Name as it appears in the signature.
    pub name: &'static str,
    /// Place of posting (appears in the dateline).
    pub place: &'static str,
    /// Relation to the player, shown in the opening ("Dear {relation},").
    pub relation: &'static str,
    /// Stock news lines — one is picked per letter.
    pub news: &'static [&'static str],
    /// Closing salutations — one is picked per letter.
    pub closings: &'static [&'static str],
}

/// The full cast of letter-writers. Indexed deterministically.
pub const SENDERS: &[Sender] = &[
    Sender {
        name: "Seán",
        place: "Boston",
        relation: "brother",
        news: &[
            "I took up work on the new canal out of the Charles — twelve hours a day and eighty cents a day, which is more silver than I ever held at home.",
            "There is talk among the lads of buying a little parcel of land up the Hudson, but the winters here are a hard thing and I miss the soft rain.",
            "A priest from Kildare blessed our lodging-house on the feast of St. Patrick and the whole street came out singing.",
            "I saw a black man free and at his own ease on Water Street, a thing that would not happen in Roscommon and made me sit quiet a while.",
            "The foreman gave me a half-day for keening my own mother, though she is still living — the Yanks do not distinguish our grief from our superstitions.",
        ],
        closings: &[
            "Write me of the new calf and Mam's cough. Your brother, Seán.",
            "Kiss Mam for me and tell Father I have not taken the pledge nor broken it. Seán.",
            "God keep you all. — Seán.",
        ],
    },
    Sender {
        name: "Áine",
        place: "Dublin",
        relation: "sister",
        news: &[
            "The mistress took me to hear Mr. O'Connell at a meeting off Sackville Street and the crowd was so thick a pin could not fall between them.",
            "I have been given the keeping of the linen press, which is a trust, and an extra shilling with it for the month.",
            "There was a great fire in a tenement by the quays this week past and the whole city smelt of burnt oats for two days.",
            "A girl from Castlerea took ill with the typhus in the scullery and was taken away in a cart. The mistress prays the Latin prayers with her beads and will not come downstairs.",
            "I saw a lady in Grafton Street with a little dog on a silver chain and I laughed aloud, which is not a thing maids are supposed to do.",
        ],
        closings: &[
            "Mind the thatch before the first storm. Your loving sister, Áine.",
            "Tell Fr. Tierney I say my prayers and do not forget the Irish. — Áine.",
            "I am saving against Christmas and shall come home if the coaches are running. Áine.",
        ],
    },
    Sender {
        name: "Fr. Michael",
        place: "Maynooth",
        relation: "cousin",
        news: &[
            "We debated at supper whether the tithe is a just thing and I could not hold my tongue — the Dean was displeased but a Kerry man winked at me from the far end of the table.",
            "A bishop from Tuam came through to confirm us and asked after the parish by name. I told him the Well still draws pilgrims at Lughnasa and he was pleased.",
            "Latin comes to me now like weather — mostly cloudy, with a burst of sun when Cicero is kind.",
            "I read a letter from a Fr. Mathew in Cork who preaches against the drink. It is a new thing and I am not yet decided whether it is a holy thing.",
            "They fed us a fish on Friday that had never seen the Shannon. I think it was a cod. It had the face of a sorrowful man.",
        ],
        closings: &[
            "Your cousin in Christ, Michael.",
            "Pray for me, as I do for you all. — Michael.",
            "Ad majorem Dei gloriam. Michael.",
        ],
    },
    Sender {
        name: "Pádraig",
        place: "Gibraltar",
        relation: "nephew",
        news: &[
            "The Rock is hot as a griddle and the monkeys steal the washing off the line — you would laugh if you saw an officer chasing one in his shirtsleeves.",
            "We are fed salt beef and biscuit and a half-pint of wine that tastes of iron. A Limerick boy traded his belt for a lemon and was the envy of the barracks.",
            "There is a rumour we may be sent to the Cape, but rumour in the army is a kind of weather and passes.",
            "I met a Spanish girl at the market who sold me an orange for a smile. Do not tell my mother.",
            "The sergeant is a decent man from Meath and does not strike unless drink is in him, which is Sundays only.",
        ],
        closings: &[
            "Your soldier nephew, Pádraig.",
            "Tell Granny I wear the scapular she sent and it has kept a musket-ball out of me, or so I say. — Pádraig.",
            "God save the parish. Pádraig.",
        ],
    },
    Sender {
        name: "Bridget",
        place: "Manchester",
        relation: "cousin",
        news: &[
            "The mill is louder than the hurling green on a fair day and the air is the colour of wet turf — I cough in the morning and it passes by noon.",
            "I am called a spinner though I have never seen a wheel — the machines do the spinning and we only coax them.",
            "An Irish girl was killed in the carding room when her shawl caught the belt. We took up a collection and sent her home in a lead box.",
            "There are chapels here but no Irish in them. A priest from Sligo comes of a Sunday and says the Mass in a public-house's back room.",
            "I have put by two pounds and fourteen shillings and will not let the landlord take a penny of it.",
        ],
        closings: &[
            "Your cousin, Bridget.",
            "Send me a sprig of May-thorn if you can — I want to smell home once before winter. — Bridget.",
            "Slán go fóill, Bridget.",
        ],
    },
    Sender {
        name: "Tom",
        place: "Galway",
        relation: "friend",
        news: &[
            "We had a herring run this week that would have filled every barrel in Connacht and I thought of your da, who always swore herring were a rumour.",
            "The fair at Ballinasloe is talked of already though it is months off — a man from Athenry is bringing down two hundred head and wants a driver.",
            "A revenue cutter went aground at Mutton Island and the crew were all saved, more's the pity for the poitín-makers.",
            "Máire Duggan was wed to a cooper and there was dancing on the pier until the tide turned.",
            "I read in a Dublin paper that the King is to visit — he will not come so far as Galway but the quality are already putting on airs.",
        ],
        closings: &[
            "Your old friend, Tom.",
            "Stand me a pint when I come east at Michaelmas. — Tom.",
            "Mind yourself, mind the boreen. Tom.",
        ],
    },
];

/// A delivered letter, ready to be rendered as prose.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Letter {
    /// Day the coach delivered this letter to the Letter Office.
    pub delivered: NaiveDate,
    /// Sender index into [`SENDERS`] (kept for deterministic re-derivation).
    pub sender_idx: usize,
    /// The news body for this letter.
    pub body: String,
    /// The closing line (with signature).
    pub closing: String,
}

impl Letter {
    /// Returns the sender record backing this letter.
    pub fn sender(&self) -> &'static Sender {
        &SENDERS[self.sender_idx]
    }

    /// Renders the letter as a human-readable block.
    ///
    /// Format:
    /// ```text
    /// — 14 March 1820, from Boston —
    /// Dear brother,
    /// <body>
    /// <closing>
    /// ```
    pub fn render(&self) -> String {
        let s = self.sender();
        let dateline = format!(
            "— {} {} {}, from {} —",
            self.delivered.day(),
            month_name(self.delivered.month()),
            self.delivered.year(),
            s.place,
        );
        let opening = format!("Dear {},", s.relation);
        format!("{dateline}\n{opening}\n{}\n{}", self.body, self.closing)
    }
}

/// Returns `true` if the mail coach delivers on the given date.
///
/// The coach runs twice a week — Wednesday (outbound from Dublin) and
/// Saturday (the return). This mirrors the rough cadence of the royal
/// mail in rural Connacht in the early 1820s.
pub fn is_coach_day(date: NaiveDate) -> bool {
    matches!(date.weekday(), Weekday::Wed | Weekday::Sat)
}

/// Generates the letter (if any) that arrived on `date`.
///
/// Returns `None` on non-coach days. On coach days, picks a sender and
/// news fragment deterministically from the date — the same date always
/// produces the same letter.
pub fn letter_for(date: NaiveDate) -> Option<Letter> {
    if !is_coach_day(date) {
        return None;
    }
    let day_seed = days_since_epoch(date);
    let sender_idx = (day_seed as usize) % SENDERS.len();
    let sender = &SENDERS[sender_idx];
    let news_idx = ((day_seed / SENDERS.len() as i64) as usize) % sender.news.len();
    let close_idx =
        ((day_seed / (SENDERS.len() * sender.news.len()) as i64) as usize) % sender.closings.len();
    Some(Letter {
        delivered: date,
        sender_idx,
        body: sender.news[news_idx].to_string(),
        closing: sender.closings[close_idx].to_string(),
    })
}

/// All letters delivered in the half-open window `[since, until]` (inclusive).
///
/// Walks each date in the range and collects the coach-day letters.
/// `until < since` yields an empty vec.
pub fn letters_between(since: DateTime<Utc>, until: DateTime<Utc>) -> Vec<Letter> {
    let start = since.date_naive();
    let end = until.date_naive();
    if end < start {
        return Vec::new();
    }
    let mut out = Vec::new();
    let mut cur = start;
    loop {
        if let Some(l) = letter_for(cur) {
            out.push(l);
        }
        if cur >= end {
            break;
        }
        match cur.succ_opt() {
            Some(next) => cur = next,
            None => break,
        }
    }
    out
}

/// Days since a stable epoch (1820-01-01). Used only as a seed.
fn days_since_epoch(date: NaiveDate) -> i64 {
    let epoch = NaiveDate::from_ymd_opt(1820, 1, 1).expect("valid epoch");
    (date - epoch).num_days()
}

fn month_name(m: u32) -> &'static str {
    match m {
        1 => "January",
        2 => "February",
        3 => "March",
        4 => "April",
        5 => "May",
        6 => "June",
        7 => "July",
        8 => "August",
        9 => "September",
        10 => "October",
        11 => "November",
        12 => "December",
        _ => "",
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;

    #[test]
    fn coach_days_are_wed_and_sat() {
        // 1820-03-22 is a Wednesday, 1820-03-25 is a Saturday.
        assert!(is_coach_day(NaiveDate::from_ymd_opt(1820, 3, 22).unwrap()));
        assert!(is_coach_day(NaiveDate::from_ymd_opt(1820, 3, 25).unwrap()));
        // 1820-03-23 is a Thursday; no coach.
        assert!(!is_coach_day(NaiveDate::from_ymd_opt(1820, 3, 23).unwrap()));
    }

    #[test]
    fn letter_for_deterministic() {
        let d = NaiveDate::from_ymd_opt(1820, 3, 22).unwrap();
        let a = letter_for(d).expect("coach day");
        let b = letter_for(d).expect("coach day");
        assert_eq!(a, b, "same date must produce the same letter");
    }

    #[test]
    fn letter_for_none_on_non_coach_day() {
        let d = NaiveDate::from_ymd_opt(1820, 3, 23).unwrap(); // Thursday
        assert!(letter_for(d).is_none());
    }

    #[test]
    fn letters_between_collects_window() {
        let start = Utc.with_ymd_and_hms(1820, 3, 20, 8, 0, 0).unwrap(); // Mon
        let end = Utc.with_ymd_and_hms(1820, 4, 3, 23, 59, 59).unwrap(); // Mon
        let letters = letters_between(start, end);
        // In that 15-day window there should be 4 coach days (W, S, W, S).
        assert_eq!(letters.len(), 4);
        for l in &letters {
            assert!(matches!(l.delivered.weekday(), Weekday::Wed | Weekday::Sat));
        }
    }

    #[test]
    fn letters_between_empty_if_reversed() {
        let start = Utc.with_ymd_and_hms(1820, 4, 1, 0, 0, 0).unwrap();
        let end = Utc.with_ymd_and_hms(1820, 3, 1, 0, 0, 0).unwrap();
        assert!(letters_between(start, end).is_empty());
    }

    #[test]
    fn render_letter_contains_dateline_and_signature() {
        let d = NaiveDate::from_ymd_opt(1820, 3, 22).unwrap();
        let l = letter_for(d).unwrap();
        let text = l.render();
        assert!(text.contains("1820"));
        assert!(text.contains(l.sender().place));
        assert!(text.contains(&format!("Dear {},", l.sender().relation)));
    }

    #[test]
    fn letter_cycles_through_every_sender_within_3_weeks() {
        // Across 21 days (6 coach days) every sender index in the pool
        // should be distinct enough that the run hits multiple voices.
        let start = Utc.with_ymd_and_hms(1820, 3, 1, 0, 0, 0).unwrap();
        let end = Utc.with_ymd_and_hms(1820, 3, 21, 23, 59, 59).unwrap();
        let letters = letters_between(start, end);
        let distinct: std::collections::HashSet<_> = letters.iter().map(|l| l.sender_idx).collect();
        assert!(
            distinct.len() >= 3,
            "expected at least 3 distinct senders in 3 weeks, got {}",
            distinct.len()
        );
    }
}
