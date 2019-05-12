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
}
