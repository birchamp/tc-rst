//! USFM 3 attribute lists: `|key="value" key2="value2"` or the default form `|value`.

use crate::Error;

/// An attribute list exactly as written, from the `|` up to the closing marker.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Attributes<'a> {
    raw: &'a str,
}

/// One attribute. `key` is `None` for the default attribute (`\w gracious|grace\w*`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Attribute<'a> {
    pub key: Option<&'a str>,
    pub value: &'a str,
}

impl<'a> Attributes<'a> {
    pub(crate) fn new(raw: &'a str) -> Self {
        Self { raw }
    }

    /// The exact bytes, including the leading `|`.
    pub fn raw(&self) -> &'a str {
        self.raw
    }

    /// The attributes in written order.
    pub fn parse(&self) -> Result<Vec<Attribute<'a>>, Error> {
        let bad = || Error::InvalidAttributes(self.raw.to_owned());
        let body = self.raw.strip_prefix('|').ok_or_else(bad)?;
        if !body.contains('=') {
            return Ok(vec![Attribute {
                key: None,
                value: body.trim(),
            }]);
        }
        let mut list = Vec::new();
        let mut rest = body.trim_start();
        while !rest.is_empty() {
            let (key, after_key) = rest.split_once('=').ok_or_else(bad)?;
            let key_ok = !key.is_empty()
                && key
                    .bytes()
                    .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_');
            if !key_ok {
                return Err(bad());
            }
            let quoted = after_key.strip_prefix('"').ok_or_else(bad)?;
            let (value, after_value) = quoted.split_once('"').ok_or_else(bad)?;
            list.push(Attribute {
                key: Some(key),
                value,
            });
            let trimmed = after_value.trim_start();
            if trimmed.len() == after_value.len() && !trimmed.is_empty() {
                return Err(bad());
            }
            rest = trimmed;
        }
        Ok(list)
    }

    /// The value of the first attribute named `key`.
    pub fn get(&self, key: &str) -> Result<Option<&'a str>, Error> {
        Ok(self
            .parse()?
            .into_iter()
            .find(|a| a.key == Some(key))
            .map(|a| a.value))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn named_and_default_forms_parse() {
        let attrs = Attributes::new("|x-occurrence=\"1\" x-occurrences=\"2\" ");
        assert_eq!(attrs.get("x-occurrences").unwrap(), Some("2"));
        let default = Attributes::new("|grace");
        assert_eq!(
            default.parse().unwrap(),
            vec![Attribute {
                key: None,
                value: "grace"
            }]
        );
        for bad in ["|a=\"1\"b=\"2\"", "|a=1", "|=\"x\"", "|a=\"open", "no bar"] {
            assert!(Attributes::new(bad).parse().is_err(), "{bad:?} should fail");
        }
    }
}
