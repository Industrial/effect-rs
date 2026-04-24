//! Exercise [`id_effect_platform::fs::read`] with [`LiveFileSystem`] in `R`.

use std::path::PathBuf;

use id_effect::{Cons, Context, Layer, Nil, run_async};
use id_effect_platform::fs::{FileSystem, FileSystemKey, LiveFileSystem, layer_file_system, read};

type Env = Context<Cons<id_effect::Service<FileSystemKey, LiveFileSystem>, Nil>>;

#[tokio::test]
async fn read_via_layer_and_live_fs() {
  let dir = tempfile::tempdir().expect("tempdir");
  let rel = PathBuf::from("layer-read.txt");
  let abs = dir.path().join(&rel);
  let fs = LiveFileSystem::new();
  run_async(fs.write(&abs, b"layered"), ())
    .await
    .expect("seed");

  let stack = layer_file_system(fs);
  let svc = stack.build().unwrap();
  let env = Context::new(Cons(svc, Nil));
  let bytes = run_async(read::<Env, _>(abs), env)
    .await
    .expect("read via layer");
  assert_eq!(bytes, b"layered");
}
