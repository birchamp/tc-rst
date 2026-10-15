//! Extra HTML attributes a component passes through to one of its elements: React's
//! `{...rest}`.

use std::borrow::Cow;

/// An ordered list of attributes (`id`, `title`, `aria-label`, `data-testid`, …). A later
/// entry with the same name replaces the earlier one, keeping its place.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Attrs(Vec<(Cow<'static, str>, String)>);

impl Attrs {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn set(&mut self, name: impl Into<Cow<'static, str>>, value: impl Into<String>) {
        let name = name.into();
        let value = value.into();
        match self.0.iter_mut().find(|(n, _)| *n == name) {
            Some(slot) => slot.1 = value,
            None => self.0.push((name, value)),
        }
    }

    #[must_use]
    pub fn with(mut self, name: impl Into<Cow<'static, str>>, value: impl Into<String>) -> Self {
        self.set(name, value);
        self
    }

    pub fn get(&self, name: &str) -> Option<&str> {
        self.0
            .iter()
            .find(|(n, _)| n == name)
            .map(|(_, v)| v.as_str())
    }

    /// The same list without `name`, and its value.
    pub fn take(&mut self, name: &str) -> Option<String> {
        let at = self.0.iter().position(|(n, _)| n == name)?;
        Some(self.0.remove(at).1)
    }

    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    pub fn iter(&self) -> impl Iterator<Item = (&str, &str)> {
        self.0.iter().map(|(n, v)| (n.as_ref(), v.as_str()))
    }

    /// `{ ...this, ...other }`.
    pub fn extend(&mut self, other: &Attrs) {
        for (n, v) in &other.0 {
            self.set(n.clone(), v.clone());
        }
    }
}

impl<const N: usize> From<[(&'static str, &str); N]> for Attrs {
    fn from(list: [(&'static str, &str); N]) -> Self {
        let mut a = Attrs::new();
        for (n, v) in list {
            a.set(n, v);
        }
        a
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_repeated_name_replaces_in_place() {
        let mut a = Attrs::from([("id", "x"), ("title", "t")]);
        a.set("id", "y");
        assert_eq!(
            a.iter().collect::<Vec<_>>(),
            vec![("id", "y"), ("title", "t")]
        );
        assert_eq!(a.take("id").as_deref(), Some("y"));
        assert_eq!(a.get("id"), None);
    }
}
