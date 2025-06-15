use std::cmp;

use derive_more::{
  Deref,
  DerefMut,
  Display,
  From,
  FromStr,
};

#[derive(
  Deref, DerefMut, Display, Debug, Clone, PartialEq, Eq, From, FromStr,
)]
pub struct Version(String);

impl PartialOrd for Version {
  fn partial_cmp(&self, other: &Self) -> Option<cmp::Ordering> {
    Some(self.cmp(other))
  }
}

impl cmp::Ord for Version {
  fn cmp(&self, that: &Self) -> cmp::Ordering {
    let this = VersionComponentIter::from(&***self).filter_map(Result::ok);
    let that = VersionComponentIter::from(&***that).filter_map(Result::ok);

    this.cmp(that)
  }
}

impl From<&str> for Version {
  fn from(s: &str) -> Self {
    Self(s.to_owned())
  }
}

impl<'a> IntoIterator for &'a Version {
  type Item = Result<VersionComponent<'a>, &'a str>;

  type IntoIter = VersionComponentIter<'a>;

  fn into_iter(self) -> Self::IntoIter {
    VersionComponentIter::from(&***self)
  }
}

#[derive(Deref, Display, Debug, Clone, Copy)]
pub struct VersionComponent<'a>(&'a str);

impl PartialEq for VersionComponent<'_> {
  fn eq(&self, other: &Self) -> bool {
    self.cmp(other) == cmp::Ordering::Equal
  }
}

impl Eq for VersionComponent<'_> {}

impl PartialOrd for VersionComponent<'_> {
  fn partial_cmp(&self, other: &Self) -> Option<cmp::Ordering> {
    Some(self.cmp(other))
  }
}

impl cmp::Ord for VersionComponent<'_> {
  fn cmp(&self, other: &Self) -> cmp::Ordering {
    let self_digit = self.0.bytes().all(|char| char.is_ascii_digit());
    let other_digit = other.0.bytes().all(|char| char.is_ascii_digit());

    match (self_digit, other_digit) {
      (true, true) => {
        let self_nonzero = self.0.trim_start_matches('0');
        let other_nonzero = other.0.trim_start_matches('0');

        self_nonzero
          .len()
          .cmp(&other_nonzero.len())
          .then_with(|| self_nonzero.cmp(other_nonzero))
      },

      (false, false) => {
        match (self.0, other.0) {
          ("pre", _) => cmp::Ordering::Less,
          (_, "pre") => cmp::Ordering::Greater,
          _ => self.0.cmp(other.0),
        }
      },

      (true, false) => cmp::Ordering::Greater,
      (false, true) => cmp::Ordering::Less,
    }
  }
}

/// Yields [`VertionComponent`] from a version string.
#[derive(Deref, DerefMut, From)]
pub struct VersionComponentIter<'a>(&'a str);

impl<'a> Iterator for VersionComponentIter<'a> {
  type Item = Result<VersionComponent<'a>, &'a str>;

  fn next(&mut self) -> Option<Self::Item> {
    const SPLIT_CHARS: &[char] = &['.', '-', '+', '*', '=', '×', ' '];

    if self.starts_with(SPLIT_CHARS) {
      let len = self.chars().next().unwrap().len_utf8();
      let (this, rest) = self.split_at(len);

      **self = rest;
      return Some(Err(this));
    }

    // Get the next character and decide if it is a digit.
    let is_digit = self.chars().next()?.is_ascii_digit();

    // Based on this collect characters after this into the component.
    let component_len = self
      .chars()
      .take_while(|&char| {
        char.is_ascii_digit() == is_digit && !SPLIT_CHARS.contains(&char)
      })
      .map(char::len_utf8)
      .sum();

    let component = &self[..component_len];
    **self = &self[component_len..];

    assert!(!component.is_empty());

    Some(Ok(VersionComponent(component)))
  }
}

#[cfg(test)]
mod tests {
  use proptest::proptest;

  use super::{
    VersionComponent,
    VersionComponentIter,
  };

  #[test]
  fn version_component_iter() {
    let version = "132.1.2test234-1-man----.--.......---------..---";

    assert_eq!(
      VersionComponentIter::from(version)
        .filter_map(Result::ok)
        .collect::<Vec<_>>(),
      [
        VersionComponent("132"),
        VersionComponent("1"),
        VersionComponent("2"),
        VersionComponent("test"),
        VersionComponent("234"),
        VersionComponent("1"),
        VersionComponent("man")
      ]
    );
  }

  proptest! {
    #[test]
    fn version_cmp_number(this: u128, that: u128) {
      let real_ord = this.cmp(&that);

      let component_ord = VersionComponent(&this.to_string())
        .cmp(&VersionComponent(&that.to_string()));

      assert_eq!(real_ord, component_ord);
    }
  }
}
