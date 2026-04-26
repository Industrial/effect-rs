//! HTTP URI helpers (`http::Uri` parse / build).

use http::Uri;
use rayon::prelude::*;

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

/// Parse many URI strings in parallel; results are in the same order as `urls`.
#[inline]
pub fn parse_uris_par(urls: &[&str]) -> Vec<Result<Uri, http::uri::InvalidUri>> {
  urls.par_iter().map(|s| s.parse()).collect()
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

  mod parse_uris_par {
    use super::*;

    #[test]
    fn preserves_order_and_mixed_ok_err() {
      let urls: &[&str] = &[
        "https://a.example/",
        "not a uri at all",
        "https://b.example/",
      ];
      let out = parse_uris_par(urls);
      assert_eq!(out.len(), 3);
      assert!(out[0].as_ref().unwrap().host() == Some("a.example"));
      assert!(out[1].is_err());
      assert!(out[2].as_ref().unwrap().host() == Some("b.example"));
    }
  }
}
