use crate::ParseError;
use serde::{Deserialize, Serialize};
use std::fmt;
use std::str::FromStr;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Testament {
    Old,
    New,
}

/// The 66 books of the Protestant canon, in canonical order. The order of the variants is
/// the canonical order, so `Ord` sorts books the way a Bible does.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum BookId {
    Genesis,
    Exodus,
    Leviticus,
    Numbers,
    Deuteronomy,
    Joshua,
    Judges,
    Ruth,
    FirstSamuel,
    SecondSamuel,
    FirstKings,
    SecondKings,
    FirstChronicles,
    SecondChronicles,
    Ezra,
    Nehemiah,
    Esther,
    Job,
    Psalms,
    Proverbs,
    Ecclesiastes,
    SongofSongs,
    Isaiah,
    Jeremiah,
    Lamentations,
    Ezekiel,
    Daniel,
    Hosea,
    Joel,
    Amos,
    Obadiah,
    Jonah,
    Micah,
    Nahum,
    Habakkuk,
    Zephaniah,
    Haggai,
    Zechariah,
    Malachi,
    Matthew,
    Mark,
    Luke,
    John,
    Acts,
    Romans,
    FirstCorinthians,
    SecondCorinthians,
    Galatians,
    Ephesians,
    Philippians,
    Colossians,
    FirstThessalonians,
    SecondThessalonians,
    FirstTimothy,
    SecondTimothy,
    Titus,
    Philemon,
    Hebrews,
    James,
    FirstPeter,
    SecondPeter,
    FirstJohn,
    SecondJohn,
    ThirdJohn,
    Jude,
    Revelation,
}

const TABLE: [(BookId, &str, &str); 66] = [
    (BookId::Genesis, "GEN", "Genesis"),
    (BookId::Exodus, "EXO", "Exodus"),
    (BookId::Leviticus, "LEV", "Leviticus"),
    (BookId::Numbers, "NUM", "Numbers"),
    (BookId::Deuteronomy, "DEU", "Deuteronomy"),
    (BookId::Joshua, "JOS", "Joshua"),
    (BookId::Judges, "JDG", "Judges"),
    (BookId::Ruth, "RUT", "Ruth"),
    (BookId::FirstSamuel, "1SA", "1 Samuel"),
    (BookId::SecondSamuel, "2SA", "2 Samuel"),
    (BookId::FirstKings, "1KI", "1 Kings"),
    (BookId::SecondKings, "2KI", "2 Kings"),
    (BookId::FirstChronicles, "1CH", "1 Chronicles"),
    (BookId::SecondChronicles, "2CH", "2 Chronicles"),
    (BookId::Ezra, "EZR", "Ezra"),
    (BookId::Nehemiah, "NEH", "Nehemiah"),
    (BookId::Esther, "EST", "Esther"),
    (BookId::Job, "JOB", "Job"),
    (BookId::Psalms, "PSA", "Psalms"),
    (BookId::Proverbs, "PRO", "Proverbs"),
    (BookId::Ecclesiastes, "ECC", "Ecclesiastes"),
    (BookId::SongofSongs, "SNG", "Song of Songs"),
    (BookId::Isaiah, "ISA", "Isaiah"),
    (BookId::Jeremiah, "JER", "Jeremiah"),
    (BookId::Lamentations, "LAM", "Lamentations"),
    (BookId::Ezekiel, "EZK", "Ezekiel"),
    (BookId::Daniel, "DAN", "Daniel"),
    (BookId::Hosea, "HOS", "Hosea"),
    (BookId::Joel, "JOL", "Joel"),
    (BookId::Amos, "AMO", "Amos"),
    (BookId::Obadiah, "OBA", "Obadiah"),
    (BookId::Jonah, "JON", "Jonah"),
    (BookId::Micah, "MIC", "Micah"),
    (BookId::Nahum, "NAM", "Nahum"),
    (BookId::Habakkuk, "HAB", "Habakkuk"),
    (BookId::Zephaniah, "ZEP", "Zephaniah"),
    (BookId::Haggai, "HAG", "Haggai"),
    (BookId::Zechariah, "ZEC", "Zechariah"),
    (BookId::Malachi, "MAL", "Malachi"),
    (BookId::Matthew, "MAT", "Matthew"),
    (BookId::Mark, "MRK", "Mark"),
    (BookId::Luke, "LUK", "Luke"),
    (BookId::John, "JHN", "John"),
    (BookId::Acts, "ACT", "Acts"),
    (BookId::Romans, "ROM", "Romans"),
    (BookId::FirstCorinthians, "1CO", "1 Corinthians"),
    (BookId::SecondCorinthians, "2CO", "2 Corinthians"),
    (BookId::Galatians, "GAL", "Galatians"),
    (BookId::Ephesians, "EPH", "Ephesians"),
    (BookId::Philippians, "PHP", "Philippians"),
    (BookId::Colossians, "COL", "Colossians"),
    (BookId::FirstThessalonians, "1TH", "1 Thessalonians"),
    (BookId::SecondThessalonians, "2TH", "2 Thessalonians"),
    (BookId::FirstTimothy, "1TI", "1 Timothy"),
    (BookId::SecondTimothy, "2TI", "2 Timothy"),
    (BookId::Titus, "TIT", "Titus"),
    (BookId::Philemon, "PHM", "Philemon"),
    (BookId::Hebrews, "HEB", "Hebrews"),
    (BookId::James, "JAS", "James"),
    (BookId::FirstPeter, "1PE", "1 Peter"),
    (BookId::SecondPeter, "2PE", "2 Peter"),
    (BookId::FirstJohn, "1JN", "1 John"),
    (BookId::SecondJohn, "2JN", "2 John"),
    (BookId::ThirdJohn, "3JN", "3 John"),
    (BookId::Jude, "JUD", "Jude"),
    (BookId::Revelation, "REV", "Revelation"),
];

impl BookId {
    pub const ALL: [BookId; 66] = {
        let mut all = [BookId::Genesis; 66];
        let mut i = 0;
        while i < 66 {
            all[i] = TABLE[i].0;
            i += 1;
        }
        all
    };

    fn index(self) -> usize {
        self as usize
    }

    /// The USFM book code, such as `TIT`.
    pub fn code(self) -> &'static str {
        TABLE[self.index()].1
    }

    /// The English name, used only where no localized name exists.
    pub fn english_name(self) -> &'static str {
        TABLE[self.index()].2
    }

    /// Position in the canon, 1 for Genesis through 66 for Revelation.
    pub fn ordinal(self) -> u8 {
        // The table has 66 entries, so the index always fits in a u8.
        (self.index() + 1) as u8
    }

    /// The number Door43 resource repositories put in file names, such as `57` in
    /// `57-TIT.usfm`. The New Testament starts at 41, so 40 is never used.
    pub fn door43_number(self) -> u8 {
        match self.testament() {
            Testament::Old => self.ordinal(),
            Testament::New => self.ordinal() + 1,
        }
    }

    pub fn testament(self) -> Testament {
        if self < BookId::Matthew {
            Testament::Old
        } else {
            Testament::New
        }
    }

    pub fn from_ordinal(ordinal: u8) -> Option<Self> {
        (1..=66)
            .contains(&ordinal)
            .then(|| TABLE[ordinal as usize - 1].0)
    }
}

impl fmt::Display for BookId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.code())
    }
}

impl FromStr for BookId {
    type Err = ParseError;

    /// Accepts the USFM code in any letter case.
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        TABLE
            .iter()
            .find(|(_, code, _)| code.eq_ignore_ascii_case(s))
            .map(|(book, _, _)| *book)
            .ok_or_else(|| ParseError::UnknownBook(s.to_owned()))
    }
}

impl Serialize for BookId {
    fn serialize<S: serde::Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        s.serialize_str(self.code())
    }
}

impl<'de> Deserialize<'de> for BookId {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        let s = String::deserialize(d)?;
        s.parse().map_err(serde::de::Error::custom)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn table_matches_variant_order() {
        for (i, (book, _, _)) in TABLE.iter().enumerate() {
            assert_eq!(book.index(), i);
        }
    }

    #[test]
    fn codes_and_numbers() {
        assert_eq!("tit".parse::<BookId>().unwrap(), BookId::Titus);
        assert_eq!(BookId::Titus.code(), "TIT");
        assert_eq!(BookId::Titus.door43_number(), 57);
        assert_eq!(BookId::Malachi.door43_number(), 39);
        assert_eq!(BookId::Matthew.door43_number(), 41);
        assert_eq!(BookId::Revelation.ordinal(), 66);
        assert_eq!(BookId::from_ordinal(32), Some(BookId::Jonah));
        assert_eq!(BookId::from_ordinal(0), None);
        assert_eq!(BookId::Jonah.testament(), Testament::Old);
        assert_eq!(BookId::ALL.len(), 66);
        assert!("FRT".parse::<BookId>().is_err());
    }
}
