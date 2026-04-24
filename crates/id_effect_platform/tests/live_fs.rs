//! Integration tests for [`id_effect_platform::fs::LiveFileSystem`] (Tokio-backed I/O).

use id_effect_platform::error::FsError;
use id_effect_platform::fs::{FileSystem, LiveFileSystem};
use id_effect_tokio::run_async;
use tempfile::tempdir;

#[tokio::test]
async fn live_write_read_append_metadata_remove_round_trip() {
  let dir = tempdir().expect("tempdir");
  let path = dir.path().join("data.bin");
  let fs = LiveFileSystem::new();

  run_async(fs.write(&path, b"ab"), ()).await.expect("write");
  let n = run_async(fs.metadata_len(&path), ())
    .await
    .expect("metadata");
  assert_eq!(n, 2);

  run_async(fs.append(&path, b"c"), ()).await.expect("append");
  let body = run_async(fs.read(&path), ()).await.expect("read");
  assert_eq!(body, b"abc");

  run_async(fs.remove_file(&path), ()).await.expect("remove");
  let err = run_async(fs.read(&path), ())
    .await
    .expect_err("missing file");
  assert!(
    matches!(err, FsError::Io(ref e) if e.kind() == std::io::ErrorKind::NotFound),
    "unexpected err: {err:?}"
  );
}

#[tokio::test]
async fn live_create_dir_all_nested_then_write_under() {
  let dir = tempdir().expect("tempdir");
  let nested_dir = dir.path().join("p").join("q");
  let file = nested_dir.join("f.txt");
  let fs = LiveFileSystem::new();

  run_async(fs.create_dir_all(&nested_dir), ())
    .await
    .expect("mkdir");
  run_async(fs.write(&file, b"x"), ())
    .await
    .expect("write under nested");
  let got = run_async(fs.read(&file), ()).await.expect("read");
  assert_eq!(got, b"x");
}
