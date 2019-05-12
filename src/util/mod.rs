/// Extension trait to check if a string begins with a prefix, and return the tail if so.
pub(crate) trait ConsumePrefix {
  /// Checks if a string starts with a prefix, and returns the part after the prefix if so.
  fn consume_prefix(&self, prefix: &str) -> Option<&str>;
}

impl<T: AsRef<str>> ConsumePrefix for T {
  fn consume_prefix(&self, prefix: &str) -> Option<&str> {
    let s = self.as_ref();
    if s.starts_with(prefix) {
      Some(&s[prefix.len()..])
    } else {
      None
    }
  }
}

/// Extension trait to add helpers to split a string once.
pub(crate) trait SplitOnce {
  // TODO: Pattern instead of &str?
  fn split_once(&self, pattern: &str) -> Option<(&str, &str)>;
  fn rsplit_once(&self, pattern: &str) -> Option<(&str, &str)>;
}

impl<T: AsRef<str>> SplitOnce for T {
  fn split_once(&self, pattern: &str) -> Option<(&str, &str)> {
    let mut s = self.as_ref().splitn(2, pattern);
    let first = s.next().unwrap();
    if let Some(second) = s.next() {
      Some((first, second))
    } else {
      None
    }
  }

  fn rsplit_once(&self, pattern: &str) -> Option<(&str, &str)> {
    let mut s = self.as_ref().rsplitn(2, pattern);
    let first = s.next().unwrap();
    if let Some(second) = s.next() {
      Some((first, second))
    } else {
      None
    }
  }
}

#[cfg(test)]
mod test {
  #[test]
  fn consume_prefix() {
    use super::ConsumePrefix;
    assert_eq!("foobar".consume_prefix("bar"), None);
    assert_eq!("foobar".consume_prefix("foobar"), Some(""));
    assert_eq!("foobar".consume_prefix("foo"), Some("bar"));
    assert_eq!("foobar".consume_prefix(""), Some("foobar"));
  }

  #[test]
  fn split_once() {
    use super::SplitOnce;
    assert_eq!("foo,bar,baz".split_once("foo,bar,baz"), Some(("", "")));
    assert_eq!("foo,bar,baz".split_once(","), Some(("foo", "bar,baz")));
    assert_eq!("foo,bar,baz".split_once("!"), None);
  }

  #[test]
  fn rsplit_once() {
    use super::SplitOnce;
    assert_eq!("foo,bar,baz".rsplit_once("foo,bar,baz"), Some(("", "")));
    assert_eq!("foo,bar,baz".rsplit_once(","), Some(("baz", "foo,bar")));
    assert_eq!("foo,bar,baz".rsplit_once("!"), None);
  }
}
