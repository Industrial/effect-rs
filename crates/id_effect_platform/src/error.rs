//! Structured errors for platform I/O (`HttpError`, `FsError`, `ProcessError`, [`PlatformError`]).

use std::fmt;

/// HTTP client / transport failures.
#[derive(Debug)]
pub enum HttpError {
  /// Underlying [`reqwest::Error`].
  Reqwest(reqwest::Error),
  /// Response exceeded configured maximum body size.
  BodyTooLarge {
    /// Observed byte length.
    len: usize,
    /// Configured cap.
    max: usize,
  },
  /// URL or header could not be applied to the transport.
  InvalidRequest(String),
}

impl fmt::Display for HttpError {
  fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
    match self {
      HttpError::Reqwest(e) => write!(f, "http request failed: {e}"),
      HttpError::BodyTooLarge { len, max } => {
        write!(f, "http response body too large: {len} bytes (max {max})")
      }
      HttpError::InvalidRequest(s) => write!(f, "invalid http request: {s}"),
    }
  }
}

impl std::error::Error for HttpError {
  fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
    match self {
      HttpError::Reqwest(e) => Some(e),
      _ => None,
    }
  }
}

impl From<reqwest::Error> for HttpError {
  fn from(value: reqwest::Error) -> Self {
    HttpError::Reqwest(value)
  }
}

/// Filesystem failures.
#[derive(Debug)]
pub enum FsError {
  /// Underlying I/O error.
  Io(std::io::Error),
  /// Path escapes the virtual root (test filesystem) or similar policy violation.
  PathNotAllowed(String),
}

impl fmt::Display for FsError {
  fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
    match self {
      FsError::Io(e) => write!(f, "filesystem I/O: {e}"),
      FsError::PathNotAllowed(p) => write!(f, "path not allowed: {p}"),
    }
  }
}

impl std::error::Error for FsError {
  fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
    match self {
      FsError::Io(e) => Some(e),
      _ => None,
    }
  }
}

impl From<std::io::Error> for FsError {
  fn from(value: std::io::Error) -> Self {
    FsError::Io(value)
  }
}

/// Process spawn / wait failures.
#[derive(Debug)]
pub enum ProcessError {
  /// Underlying I/O error.
  Io(std::io::Error),
  /// Process was killed or could not be spawned.
  SpawnFailed(String),
}

impl fmt::Display for ProcessError {
  fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
    match self {
      ProcessError::Io(e) => write!(f, "process I/O: {e}"),
      ProcessError::SpawnFailed(s) => write!(f, "process spawn failed: {s}"),
    }
  }
}

impl std::error::Error for ProcessError {
  fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
    match self {
      ProcessError::Io(e) => Some(e),
      _ => None,
    }
  }
}

impl From<std::io::Error> for ProcessError {
  fn from(value: std::io::Error) -> Self {
    ProcessError::Io(value)
  }
}

/// Tagged union for boundaries that want one error type.
#[derive(Debug)]
pub enum PlatformError {
  /// Reserved for application boundaries that report an unsupported operation.
  #[allow(dead_code)]
  Unsupported(&'static str),
  /// HTTP failures.
  Http(HttpError),
  /// Filesystem failures.
  Fs(FsError),
  /// Process failures.
  Process(ProcessError),
}

impl fmt::Display for PlatformError {
  fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
    match self {
      PlatformError::Unsupported(s) => write!(f, "unsupported platform operation: {s}"),
      PlatformError::Http(e) => e.fmt(f),
      PlatformError::Fs(e) => e.fmt(f),
      PlatformError::Process(e) => e.fmt(f),
    }
  }
}

impl std::error::Error for PlatformError {
  fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
    match self {
      PlatformError::Http(e) => Some(e),
      PlatformError::Fs(e) => Some(e),
      PlatformError::Process(e) => Some(e),
      _ => None,
    }
  }
}

impl From<HttpError> for PlatformError {
  fn from(value: HttpError) -> Self {
    PlatformError::Http(value)
  }
}

impl From<FsError> for PlatformError {
  fn from(value: FsError) -> Self {
    PlatformError::Fs(value)
  }
}

impl From<ProcessError> for PlatformError {
  fn from(value: ProcessError) -> Self {
    PlatformError::Process(value)
  }
}

#[cfg(test)]
mod tests {
  use super::*;

  mod platform_error {
    use super::*;

    mod unsupported {
      use super::*;

      #[test]
      fn displays_message_including_label() {
        let e = PlatformError::Unsupported("no-op");
        let s = e.to_string();
        assert!(s.contains("no-op"));
        assert!(s.contains("unsupported"));
      }

      #[test]
      fn source_returns_none() {
        let e = PlatformError::Unsupported("x");
        assert!(std::error::Error::source(&e).is_none());
      }
    }

    mod from_http {
      use super::*;

      #[test]
      fn maps_body_too_large_display() {
        let e: PlatformError = HttpError::BodyTooLarge { len: 9, max: 8 }.into();
        assert!(e.to_string().contains("too large"));
        assert!(e.to_string().contains("9"));
      }

      #[test]
      fn error_chain_includes_inner_http_error_for_invalid_request() {
        let e: PlatformError = HttpError::InvalidRequest("bad".into()).into();
        let src = std::error::Error::source(&e).expect("PlatformError::Http chains inner error");
        assert_eq!(src.to_string(), "invalid http request: bad");
      }
    }

    mod from_fs {
      use super::*;

      #[test]
      fn maps_path_not_allowed_display() {
        let e: PlatformError = FsError::PathNotAllowed("..".into()).into();
        assert!(e.to_string().contains("path not allowed"));
      }

      #[test]
      fn source_delegates_to_io_when_fs_io() {
        let io = std::io::Error::other("x");
        let e: PlatformError = FsError::Io(io).into();
        assert!(std::error::Error::source(&e).is_some());
      }
    }

    mod from_process {
      use super::*;

      #[test]
      fn maps_spawn_failed_display() {
        let e: PlatformError = ProcessError::SpawnFailed("nope".into()).into();
        assert!(e.to_string().contains("spawn failed"));
      }
    }
  }

  mod http_error {
    use super::*;

    #[test]
    fn body_too_large_displays_len_and_max() {
      let e = HttpError::BodyTooLarge { len: 100, max: 10 };
      let s = e.to_string();
      assert!(s.contains("100"));
      assert!(s.contains("10"));
    }

    #[test]
    fn body_too_large_source_is_none() {
      let e = HttpError::BodyTooLarge { len: 1, max: 0 };
      assert!(std::error::Error::source(&e).is_none());
    }

    #[test]
    fn invalid_request_displays_message() {
      let e = HttpError::InvalidRequest("missing host".into());
      assert!(e.to_string().contains("missing host"));
    }

    #[test]
    fn invalid_request_source_is_none() {
      let e = HttpError::InvalidRequest("x".into());
      assert!(std::error::Error::source(&e).is_none());
    }

    #[tokio::test]
    async fn reqwest_variant_display_and_source_chain() {
      let inner = reqwest::get("http://127.0.0.1:1/nope")
        .await
        .expect_err("port 1 should refuse connection");
      let err = HttpError::from(inner);
      assert!(err.to_string().starts_with("http request failed:"));
      assert!(std::error::Error::source(&err).is_some());
    }
  }

  mod fs_error {
    use super::*;

    #[test]
    fn io_displays_kind_message() {
      let e = FsError::Io(std::io::Error::new(std::io::ErrorKind::NotFound, "gone"));
      assert!(e.to_string().contains("gone"));
    }

    #[test]
    fn io_source_returns_underlying() {
      let inner = std::io::Error::other("inner");
      let e = FsError::Io(inner);
      assert!(std::error::Error::source(&e).is_some());
    }

    #[test]
    fn path_not_allowed_displays_reason() {
      let e = FsError::PathNotAllowed("traversal".into());
      assert!(e.to_string().contains("traversal"));
    }

    #[test]
    fn path_not_allowed_source_is_none() {
      let e = FsError::PathNotAllowed("p".into());
      assert!(std::error::Error::source(&e).is_none());
    }

    #[test]
    fn from_io_error_maps_to_io_variant() {
      let io = std::io::Error::new(std::io::ErrorKind::Interrupted, "i");
      let e: FsError = io.into();
      assert!(matches!(e, FsError::Io(_)));
    }
  }

  mod process_error {
    use super::*;

    #[test]
    fn spawn_failed_displays_message() {
      let e = ProcessError::SpawnFailed("execv".into());
      assert!(e.to_string().contains("execv"));
    }

    #[test]
    fn spawn_failed_source_is_none() {
      let e = ProcessError::SpawnFailed("x".into());
      assert!(std::error::Error::source(&e).is_none());
    }

    #[test]
    fn io_wraps_underlying() {
      let inner = std::io::Error::new(std::io::ErrorKind::PermissionDenied, "denied");
      let e = ProcessError::Io(inner);
      assert!(e.to_string().contains("denied"));
    }

    #[test]
    fn io_source_returns_underlying() {
      let inner = std::io::Error::other("o");
      let e = ProcessError::Io(inner);
      assert!(std::error::Error::source(&e).is_some());
    }

    #[test]
    fn from_io_error_maps_to_io_variant() {
      let io = std::io::Error::new(std::io::ErrorKind::AlreadyExists, "e");
      let e: ProcessError = io.into();
      assert!(matches!(e, ProcessError::Io(_)));
    }
  }
}
