//! String rules of the design system: which words of a passage a highlight covers
//! (ScripturePassage) and which options a search keeps (Select).

/// One run of a verse: consecutive words that are all highlighted or all not, joined by
/// single spaces.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Run {
    pub on: bool,
    pub text: String,
}

/// `norm` in ScripturePassage.jsx: lower case, keeping letters, numbers and the apostrophe
/// (`/[^\p{L}\p{N}']/gu` removed).
pub fn norm(s: &str) -> String {
    s.to_lowercase()
        .chars()
        .filter(|c| *c == '\'' || c.is_numeric() || (c.is_alphabetic() && !is_mark(*c)))
        .collect()
}

/// Combining marks (`\p{M}`) of the scripts tC4 shows. `char::is_alphabetic` counts many of
/// them as letters (Hebrew points, Arabic harakat); `\p{L}` does not.
fn is_mark(c: char) -> bool {
    matches!(c,
        '\u{0300}'..='\u{036F}'
        | '\u{0483}'..='\u{0489}'
        | '\u{0591}'..='\u{05BD}'
        | '\u{05BF}'
        | '\u{05C1}'..='\u{05C2}'
        | '\u{05C4}'..='\u{05C5}'
        | '\u{05C7}'
        | '\u{0610}'..='\u{061A}'
        | '\u{064B}'..='\u{065F}'
        | '\u{0670}'
        | '\u{06D6}'..='\u{06DC}'
        | '\u{06DF}'..='\u{06E4}'
        | '\u{06E7}'..='\u{06E8}'
        | '\u{06EA}'..='\u{06ED}'
        | '\u{0900}'..='\u{0903}'
        | '\u{093A}'..='\u{094F}'
        | '\u{0951}'..='\u{0957}'
        | '\u{0962}'..='\u{0963}'
        | '\u{1AB0}'..='\u{1AFF}'
        | '\u{1DC0}'..='\u{1DFF}'
        | '\u{20D0}'..='\u{20FF}'
        | '\u{FE20}'..='\u{FE2F}')
}

/// `runs` in ScripturePassage.jsx. A phrase marks a contiguous run of words in order, never
/// every occurrence of each of its words; consecutive marked words form one run so the
/// highlight is continuous across the spaces inside a phrase.
pub fn runs(words: &[&str], phrases: &[String]) -> Vec<Run> {
    let w: Vec<String> = words.iter().map(|x| norm(x)).collect();
    let mut hit = vec![false; words.len()];
    for phrase in phrases {
        let t: Vec<String> = phrase
            .split_whitespace()
            .map(norm)
            .filter(|s| !s.is_empty())
            .collect();
        if t.is_empty() || t.len() > w.len() {
            continue;
        }
        for i in 0..=w.len() - t.len() {
            if t.iter().enumerate().all(|(j, tok)| w[i + j] == *tok) {
                for h in &mut hit[i..i + t.len()] {
                    *h = true;
                }
            }
        }
    }
    let mut out = Vec::new();
    let mut i = 0;
    while i < words.len() {
        let on = hit[i];
        let mut j = i;
        while j + 1 < words.len() && hit[j + 1] == on {
            j += 1;
        }
        out.push(Run {
            on,
            text: words[i..=j].join(" "),
        });
        i = j + 1;
    }
    out
}

/// `matches` in Select.jsx: the label contains the query, or the code starts with it, the two
/// ways a translator knows a book ("john", "JHN").
pub fn option_matches(label: &str, code: Option<&str>, query: &str) -> bool {
    let s = query.trim().to_lowercase();
    if s.is_empty() {
        return true;
    }
    label.to_lowercase().contains(&s) || code.unwrap_or("").to_lowercase().starts_with(&s)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn words(s: &str) -> Vec<&str> {
        s.split_whitespace().collect()
    }

    #[test]
    fn a_phrase_marks_its_run_not_every_word() {
        let w = words("Paul, a servant of God and the knowledge of the truth");
        let r = runs(&w, &["servant of God".to_string()]);
        assert_eq!(
            r,
            vec![
                Run {
                    on: false,
                    text: "Paul, a".into()
                },
                Run {
                    on: true,
                    text: "servant of God".into()
                },
                Run {
                    on: false,
                    text: "and the knowledge of the truth".into()
                },
            ]
        );
    }

    #[test]
    fn punctuation_and_case_do_not_block_a_match() {
        let w = words("in hope of Eternal life, that God");
        let r = runs(&w, &["eternal LIFE".to_string()]);
        assert_eq!(
            r[1],
            Run {
                on: true,
                text: "Eternal life,".into()
            }
        );
    }

    #[test]
    fn hebrew_points_are_ignored_like_unicode_letters() {
        assert_eq!(norm("אֶל־יוֹנָ֥ה"), "אליונה");
        let w = words("וַֽיְהִי֙ דְּבַר־יְהוָ֔ה אֶל־יוֹנָ֥ה בֶן־אֲמִתַּ֖י");
        let r = runs(&w, &["אל־יונה".to_string()]);
        assert_eq!(
            r[1],
            Run {
                on: true,
                text: "אֶל־יוֹנָ֥ה".into()
            }
        );
    }

    #[test]
    fn no_phrase_is_one_unmarked_run() {
        let w = words("a b c");
        assert_eq!(
            runs(&w, &[]),
            vec![Run {
                on: false,
                text: "a b c".into()
            }]
        );
    }

    #[test]
    fn a_book_matches_by_name_or_code_prefix() {
        assert!(option_matches("John", Some("JHN"), " jhn "));
        assert!(option_matches("1 John", Some("1JN"), "john"));
        assert!(!option_matches("Jonah", Some("JON"), "zz"));
        assert!(option_matches("Jonah", None, ""));
    }
}
