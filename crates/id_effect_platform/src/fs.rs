//! Virtual filesystem ([`FileSystem`]): live Tokio ([`LiveFileSystem`]) and in-memory test double ([`TestFileSystem`]).
//!
//! ## Path handling (Phase A / `iep-a-050`)
//!
//! The public API uses [`std::path::Path`] for maximum compatibility. Callers that need UTF-8 paths
//! should validate or convert at the boundary (e.g. with [`camino::Utf8PathBuf`](https://docs.rs/camino)
//! in application code) before calling these traits.
//!
//! ## Security (Phase A / `iep-a-033`)
//!
//! - **Path traversal:** [`TestFileSystem`] rejects path keys containing `..`. Live I/O follows the OS;
//!   sandboxed apps should canonicalize or jail paths at a higher layer.
//! - **Symlinks:** No special symlink policy here; treat remote paths as untrusted unless you control the tree.

use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use id_effect::kernel::Effect;

use crate::error::FsError;

id_effect::service_key!(
  /// Tag for the active [`FileSystem`] implementation in `R`.
  pub struct FileSystemKey
);

/// Capability: portable filesystem operations as [`Effect`] values.
pub trait FileSystem: Send + Sync + 'static {
  /// Read entire file into a byte vector.
  fn read(&self, path: &Path) -> Effect<Vec<u8>, FsError, ()>;
  /// Write bytes (overwrite).
  fn write(&self, path: &Path, data: &[u8]) -> Effect<(), FsError, ()>;
  /// Append bytes (create if missing).
  fn append(&self, path: &Path, data: &[u8]) -> Effect<(), FsError, ()>;
  /// Create a directory (and parents), Unix semantics.
  fn create_dir_all(&self, path: &Path) -> Effect<(), FsError, ()>;
  /// Remove a file.
  fn remove_file(&self, path: &Path) -> Effect<(), FsError, ()>;
  /// Metadata: file length if a regular file.
  fn metadata_len(&self, path: &Path) -> Effect<u64, FsError, ()>;
}

/// Tokio-backed live filesystem.
#[derive(Clone, Default)]
pub struct LiveFileSystem;

impl LiveFileSystem {
  /// New live filesystem handle.
  #[inline]
  pub fn new() -> Self {
    Self
  }
}

impl FileSystem for LiveFileSystem {
  fn read(&self, path: &Path) -> Effect<Vec<u8>, FsError, ()> {
    let path = path.to_path_buf();
    Effect::new_async(move |_r: &mut ()| {
      Box::pin(async move { tokio::fs::read(&path).await.map_err(FsError::from) })
    })
  }

  fn write(&self, path: &Path, data: &[u8]) -> Effect<(), FsError, ()> {
    let path = path.to_path_buf();
    let data = data.to_vec();
    Effect::new_async(move |_r: &mut ()| {
      Box::pin(async move { tokio::fs::write(&path, &data).await.map_err(FsError::from) })
    })
  }

  fn append(&self, path: &Path, data: &[u8]) -> Effect<(), FsError, ()> {
    let path = path.to_path_buf();
    let data = data.to_vec();
    Effect::new_async(move |_r: &mut ()| {
      Box::pin(async move {
        use tokio::io::AsyncWriteExt;
        let mut f = tokio::fs::OpenOptions::new()
          .create(true)
          .append(true)
          .open(&path)
          .await
          .map_err(FsError::from)?;
        f.write_all(&data).await.map_err(FsError::from)?;
        f.flush().await.map_err(FsError::from)?;
        Ok(())
      })
    })
  }

  fn create_dir_all(&self, path: &Path) -> Effect<(), FsError, ()> {
    let path = path.to_path_buf();
    Effect::new_async(move |_r: &mut ()| {
      Box::pin(async move {
        tokio::fs::create_dir_all(&path)
          .await
          .map_err(FsError::from)
      })
    })
  }

  fn remove_file(&self, path: &Path) -> Effect<(), FsError, ()> {
    let path = path.to_path_buf();
    Effect::new_async(move |_r: &mut ()| {
      Box::pin(async move { tokio::fs::remove_file(&path).await.map_err(FsError::from) })
    })
  }

  fn metadata_len(&self, path: &Path) -> Effect<u64, FsError, ()> {
    let path = path.to_path_buf();
    Effect::new_async(move |_r: &mut ()| {
      Box::pin(async move {
        let m = tokio::fs::metadata(&path).await.map_err(FsError::from)?;
        Ok(m.len())
      })
    })
  }
}

/// In-memory filesystem for tests (single mutex; not for production concurrency).
#[derive(Clone, Default)]
pub struct TestFileSystem {
  inner: Arc<Mutex<std::collections::BTreeMap<String, Vec<u8>>>>,
}

impl TestFileSystem {
  /// Empty tree.
  #[inline]
  pub fn new() -> Self {
    Self {
      inner: Arc::new(Mutex::new(std::collections::BTreeMap::new())),
    }
  }

  fn key(path: &Path) -> Result<String, FsError> {
    let s = path
      .to_str()
      .ok_or_else(|| FsError::PathNotAllowed("non-utf8 path".into()))?;
    if s.contains("..") {
      return Err(FsError::PathNotAllowed(
        "`..` not allowed in test paths".into(),
      ));
    }
    Ok(s.to_string())
  }
}

impl FileSystem for TestFileSystem {
  fn read(&self, path: &Path) -> Effect<Vec<u8>, FsError, ()> {
    let key = match Self::key(path) {
      Ok(k) => k,
      Err(e) => return id_effect::fail(e),
    };
    let inner = Arc::clone(&self.inner);
    Effect::new(move |_r: &mut ()| {
      let map = inner
        .lock()
        .map_err(|e| FsError::PathNotAllowed(e.to_string()))?;
      map.get(&key).cloned().ok_or_else(|| {
        FsError::Io(std::io::Error::new(
          std::io::ErrorKind::NotFound,
          "test file not found",
        ))
      })
    })
  }

  fn write(&self, path: &Path, data: &[u8]) -> Effect<(), FsError, ()> {
    let key = match Self::key(path) {
      Ok(k) => k,
      Err(e) => return id_effect::fail(e),
    };
    let inner = Arc::clone(&self.inner);
    let data = data.to_vec();
    Effect::new(move |_r: &mut ()| {
      let mut map = inner
        .lock()
        .map_err(|e| FsError::PathNotAllowed(e.to_string()))?;
      map.insert(key, data);
      Ok(())
    })
  }

  fn append(&self, path: &Path, data: &[u8]) -> Effect<(), FsError, ()> {
    let key = match Self::key(path) {
      Ok(k) => k,
      Err(e) => return id_effect::fail(e),
    };
    let inner = Arc::clone(&self.inner);
    let data = data.to_vec();
    Effect::new(move |_r: &mut ()| {
      let mut map = inner
        .lock()
        .map_err(|e| FsError::PathNotAllowed(e.to_string()))?;
      map.entry(key).or_default().extend_from_slice(&data);
      Ok(())
    })
  }

  fn create_dir_all(&self, _path: &Path) -> Effect<(), FsError, ()> {
    id_effect::succeed(())
  }

  fn remove_file(&self, path: &Path) -> Effect<(), FsError, ()> {
    let key = match Self::key(path) {
      Ok(k) => k,
      Err(e) => return id_effect::fail(e),
    };
    let inner = Arc::clone(&self.inner);
    Effect::new(move |_r: &mut ()| {
      let mut map = inner
        .lock()
        .map_err(|e| FsError::PathNotAllowed(e.to_string()))?;
      map.remove(&key).ok_or_else(|| {
        FsError::Io(std::io::Error::new(
          std::io::ErrorKind::NotFound,
          "test file not found",
        ))
      })?;
      Ok(())
    })
  }

  fn metadata_len(&self, path: &Path) -> Effect<u64, FsError, ()> {
    let key = match Self::key(path) {
      Ok(k) => k,
      Err(e) => return id_effect::fail(e),
    };
    let inner = Arc::clone(&self.inner);
    Effect::new(move |_r: &mut ()| {
      let map = inner
        .lock()
        .map_err(|e| FsError::PathNotAllowed(e.to_string()))?;
      let v = map.get(&key).ok_or_else(|| {
        FsError::Io(std::io::Error::new(
          std::io::ErrorKind::NotFound,
          "test file not found",
        ))
      })?;
      Ok(v.len() as u64)
    })
  }
}

/// [`id_effect::Service`] cell for [`FileSystemKey`].
pub type FileSystemService<F> = id_effect::Service<FileSystemKey, F>;

/// Install a [`FileSystem`] implementation.
#[inline]
pub fn layer_file_system<F>(
  fs: F,
) -> id_effect::layer::LayerFn<impl Fn() -> Result<FileSystemService<F>, std::convert::Infallible>>
where
  F: Clone + FileSystem + 'static,
{
  id_effect::layer_service::<FileSystemKey, _>(fs)
}

/// Supertrait: `R` exposes [`FileSystemKey`].
pub trait NeedsFileSystem<F>: id_effect::Get<FileSystemKey, id_effect::Here, Target = F> {}
impl<R, F> NeedsFileSystem<F> for R where
  R: id_effect::Get<FileSystemKey, id_effect::Here, Target = F>
{
}

/// Read via [`FileSystemKey`].
#[inline]
pub fn read<R, F>(path: PathBuf) -> Effect<Vec<u8>, FsError, R>
where
  R: NeedsFileSystem<F> + 'static,
  F: FileSystem + Clone + 'static,
{
  Effect::new_async(move |r: &mut R| {
    let fs = id_effect::Get::<FileSystemKey>::get(r).clone();
    let inner = fs.read(&path);
    Box::pin(async move { inner.run(&mut ()).await })
  })
}

#[cfg(test)]
mod tests {
  use super::*;

  use id_effect::run_blocking;
  use std::path::Path;

  mod test_file_system {
    use super::*;

    mod path_policy {
      use super::*;

      #[test]
      fn write_rejects_when_path_contains_dotdot() {
        let fs = TestFileSystem::new();
        let err = run_blocking(fs.write(Path::new("a/../b.txt"), b"x"), ()).unwrap_err();
        assert!(matches!(err, FsError::PathNotAllowed(_)));
      }

      #[cfg(unix)]
      #[test]
      fn write_rejects_when_path_not_utf8() {
        use std::ffi::OsString;
        use std::os::unix::ffi::OsStringExt;
        let fs = TestFileSystem::new();
        let p = PathBuf::from(OsString::from_vec(vec![0xFF, 0xFE]));
        let err = run_blocking(fs.write(&p, b"x"), ()).unwrap_err();
        assert!(matches!(err, FsError::PathNotAllowed(_)));
      }
    }

    mod read_write_round_trip {
      use super::*;

      #[test]
      fn read_returns_bytes_after_write() {
        let fs = TestFileSystem::new();
        let p = Path::new("dir/file.bin");
        run_blocking(fs.write(p, b"payload"), ()).unwrap();
        let got = run_blocking(fs.read(p), ()).unwrap();
        assert_eq!(got, b"payload");
      }

      #[test]
      fn append_extends_existing_file() {
        let fs = TestFileSystem::new();
        let p = Path::new("log.txt");
        run_blocking(fs.write(p, b"a"), ()).unwrap();
        run_blocking(fs.append(p, b"b"), ()).unwrap();
        let got = run_blocking(fs.read(p), ()).unwrap();
        assert_eq!(got, b"ab");
      }

      #[test]
      fn metadata_len_matches_written_length() {
        let fs = TestFileSystem::new();
        let p = Path::new("sized.dat");
        let data = vec![0u8; 42];
        run_blocking(fs.write(p, &data), ()).unwrap();
        let n = run_blocking(fs.metadata_len(p), ()).unwrap();
        assert_eq!(n, 42);
      }

      #[test]
      fn remove_file_deletes_then_read_fails() {
        let fs = TestFileSystem::new();
        let p = Path::new("gone.txt");
        run_blocking(fs.write(p, b"x"), ()).unwrap();
        run_blocking(fs.remove_file(p), ()).unwrap();
        let err = run_blocking(fs.read(p), ()).unwrap_err();
        assert!(matches!(err, FsError::Io(_)));
      }
    }

    mod create_dir_all {
      use super::*;

      #[test]
      fn succeeds_without_mutating_store() {
        let fs = TestFileSystem::new();
        run_blocking(fs.create_dir_all(Path::new("any/nested")), ()).unwrap();
      }
    }
  }
}
