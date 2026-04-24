//! HTTP URI helpers (`http::Uri` parse / build).

use http::Uri;

/// Parse a string into [`http::Uri`].
#[inline]
pub fn parse_uri(s: &str) -> Result<Uri, http::uri::InvalidUri> {
  s.parse()
}

/// Build a URI from scheme, authority, and path (minimal helper).
#[inline]
pub fn from_parts(
  scheme: &str,
  authority: &str,
  path_and_query: &str,
) -> Result<Uri, http::uri::InvalidUri> {
  let s = format!("{scheme}://{authority}{path_and_query}");
  s.parse()
}

#[cfg(test)]
mod tests {
  use super::*;

  mod parse_uri {
    use super::*;

    #[test]
    fn succeeds_when_https_url_valid() {
      let u = parse_uri("https://example.com/path?q=1").unwrap();
      assert_eq!(u.scheme_str(), Some("https"));
      assert_eq!(u.host(), Some("example.com"));
    }

    #[rstest::rstest]
    #[case::empty("")]
    #[case::only_spaces("   ")]
    #[case::unclosed_bracket("http://[::1")]
    fn fails_when_input_invalid(#[case] input: &str) {
      assert!(parse_uri(input).is_err());
    }
  }

  mod from_parts {
    use super::*;

    #[test]
    fn builds_expected_uri() {
      let u = from_parts("https", "api.example.com", "/v1/items").unwrap();
      assert_eq!(u.to_string(), "https://api.example.com/v1/items");
    }

    #[test]
    fn rejects_when_scheme_has_invalid_chars() {
      assert!(from_parts("ht\tp", "example.com", "/").is_err());
    }
  }
}
