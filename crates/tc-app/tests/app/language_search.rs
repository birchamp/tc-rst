//! The language suggestions (AD27, tC4 #492) and the code a suggestion offers (Q35, #582).

use std::collections::{BTreeMap, BTreeSet};

use serde_json::json;
use sha1::{Digest, Sha1};
use tc_app::language_search::{
    LanguageIndex, LanguageRow, Offer, SUGGESTION_LIMIT, language_name, offered_code,
};

fn index() -> &'static LanguageIndex {
    LanguageIndex::shared().expect("the shipped list reads")
}

fn codes_limited(query: &str, limit: usize) -> Vec<String> {
    index()
        .suggest(query, limit)
        .into_iter()
        .map(|row| row.lc.clone())
        .collect()
}

fn codes(query: &str) -> Vec<String> {
    codes_limited(query, SUGGESTION_LIMIT)
}

fn position(codes: &[String], code: &str) -> usize {
    codes
        .iter()
        .position(|c| c == code)
        .unwrap_or_else(|| panic!("{code} is not in {codes:?}"))
}

#[test]
fn the_language_list_is_tc4s() {
    let bytes = std::fs::read(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/assets/langnames.json"
    ))
    .unwrap();
    let mut hasher = Sha1::new();
    hasher.update(format!("blob {}\0", bytes.len()).as_bytes());
    hasher.update(&bytes);
    let blob: String = hasher
        .finalize()
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect();
    // tC4 d310bf3:src/data/langnames.json (assets/README.md).
    assert_eq!(blob, "dc3db76abb73002532d428fcad596c1895583bdd");
    assert_eq!(index().rows().count(), 9_166);
}

/// tC4's ten cases of `test/languageSearch.test.ts`, on the list the app ships.
#[test]
fn language_suggestions_rank_as_tc4_offline() {
    // (1) A code typed whole comes first.
    assert_eq!(codes("en")[0], "en");
    assert_eq!(codes("es-419"), ["es-419"]);
    assert_eq!(
        codes("las")[..6],
        ["las", "llm", "lsa", "lsi", "lss", "es-419"]
    );

    // (2) A name typed whole comes before the longer names that start with it.
    assert_eq!(codes("Hausa")[..2], ["ha", "hsl"]);
    assert_eq!(codes("arabic")[0], "ar");

    // (3) Capital letters and accents do not change the result.
    assert_eq!(codes("hausa"), codes("Hausa"));
    assert_eq!(codes("HAUSA"), codes("Hausa"));
    assert_eq!(codes("Espanol"), codes("español"));
    assert_eq!(codes("Espanol")[0], "es");

    // (4) An alternate name finds the language, after a language whose own name matches.
    assert_eq!(codes("Castilian"), ["es"]);
    let kiswahili = codes("Kiswahili");
    assert_eq!(kiswahili[0], "sw");
    assert!(kiswahili.contains(&"swh".to_owned()));
    let oromo = codes("Oromo");
    assert_eq!(oromo[0], "om");
    assert!(position(&oromo, "om") < position(&oromo, "gax"));
    let persian = codes("Persian");
    assert_eq!(persian[..3], ["fa", "psc", "pes"]);
    assert!(position(&persian, "prs") > position(&persian, "pes"));
    assert_eq!(codes("Kanuri"), ["kr", "bms", "knc", "kby", "krt", "kbl"]);

    // (5) A word inside the name matches after a name that starts with the query.
    let american = codes("american");
    assert!(position(&american, "ase") < position(&american, "es-419"));

    // (6, 8) A name that only contains the query comes last, in name order.
    assert_eq!(
        codes("ausa"),
        ["ha", "hsl", "swp-x-lausaha", "mot", "pwn", "swp", "tio"]
    );

    // (7) A language with no anglicized name is found and named by its own name.
    let aramaic = index().suggest("Official Aramaic", SUGGESTION_LIMIT)[0];
    assert_eq!(aramaic.lc, "arc");
    assert_eq!(aramaic.ang, "");
    assert_eq!(language_name(aramaic), "Official Aramaic (700-300 BCE)");

    // (8) Two languages with the same name keep one order: by code.
    assert_eq!(codes("Acholi")[..2], ["ach", "ach-ss-acholi"]);
    assert_eq!(codes("Acholi"), codes("Acholi"));

    // (9) No query and no match give no language.
    assert!(codes("").is_empty());
    assert!(codes("   ").is_empty());
    assert!(codes("zzzzqqq").is_empty());

    // (10) The limit holds.
    assert_eq!(codes("a").len(), 100);
    assert_eq!(codes_limited("a", 5).len(), 5);
}

/// The codes Door43 lists that a Scripture Burrito cannot hold get a code it can (Q35), never
/// "Undetermined" (#582): every offered code passes the Burrito's own check of `languages`,
/// no letter of the Door43 code is lost, and no offer is another language's code.
#[test]
fn a_suggestion_offers_a_code_a_burrito_can_hold() {
    let rows: Vec<&LanguageRow> = index().rows().collect();
    let offers: Vec<(&LanguageRow, Offer)> =
        rows.iter().map(|row| (*row, offered_code(&row.lc))).collect();

    let unusable: Vec<&str> = offers
        .iter()
        .filter(|(_, offer)| *offer == Offer::Unusable)
        .map(|(row, _)| row.lc.as_str())
        .collect();
    assert_eq!(unusable, ["-x-", "iba-x-", "mas-x-", "mdj-x-"]);
    let adjusted: Vec<(&str, &str)> = offers
        .iter()
        .filter_map(|(row, offer)| match offer {
            Offer::Adjusted(code) => Some((row.lc.as_str(), code.as_str())),
            _ => None,
        })
        .collect();
    assert_eq!(adjusted.len(), 468);
    assert!(adjusted.contains(&("acm-x-lebarabic", "acm-x-lebarabi-c")));

    // Lossless: the offer holds every letter and digit of the Door43 code, in order.
    let letters = |code: &str| -> String {
        code.split('-')
            .filter(|part| !part.eq_ignore_ascii_case("x"))
            .flat_map(str::chars)
            .filter(char::is_ascii_alphanumeric)
            .map(|c| c.to_ascii_lowercase())
            .collect()
    };
    for (door43, code) in &adjusted {
        assert_eq!(letters(door43), letters(code), "{door43} -> {code}");
    }

    // Only the two spellings of one language share an offer, and no offer is a listed code.
    let listed: BTreeSet<&str> = rows.iter().map(|row| row.lc.as_str()).collect();
    let mut by_offer: BTreeMap<&str, Vec<&str>> = BTreeMap::new();
    for (door43, code) in &adjusted {
        by_offer.entry(code).or_default().push(door43);
        assert!(!listed.contains(code), "{door43} is offered {code}, a listed code");
    }
    let shared: Vec<(&str, Vec<&str>)> = by_offer
        .into_iter()
        .filter(|(_, codes)| codes.len() > 1)
        .collect();
    assert_eq!(
        shared,
        [(
            "alk-x-alakatap-ue",
            vec!["alk-alakatapue", "alk-x-alakatapue"]
        )]
    );

    // Every offered code passes the check a project's `languages` gets, and the tag rule
    // agrees with that check on every Door43 code. Checked on threads: it validates a whole
    // document per code.
    let chunks: Vec<&[(&LanguageRow, Offer)]> = offers.chunks(offers.len() / 8 + 1).collect();
    std::thread::scope(|scope| {
        for chunk in chunks {
            scope.spawn(move || {
                for (row, offer) in chunk {
                    let languages = |tag: &str| {
                        json!([{
                            "tag": tag,
                            "name": {"en": language_name(row)},
                            "scriptDirection": row.ld,
                        }])
                    };
                    let door43_holds = tc_burrito::check_field(
                        tc_burrito::ProjectKind::BibleText,
                        "languages",
                        &languages(&row.lc),
                    )
                    .is_ok();
                    assert_eq!(
                        door43_holds,
                        tc_burrito::is_language_tag(&row.lc),
                        "{}",
                        row.lc
                    );
                    let offered = match offer {
                        Offer::Same => row.lc.as_str(),
                        Offer::Adjusted(code) => code.as_str(),
                        Offer::Unusable => continue,
                    };
                    let checked = tc_burrito::check_field(
                        tc_burrito::ProjectKind::BibleText,
                        "languages",
                        &languages(offered),
                    );
                    assert_eq!(checked, Ok(()), "{} offers {offered}", row.lc);
                }
            });
        }
    });
}
