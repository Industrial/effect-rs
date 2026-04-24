use std::path::Path;

use id_effect::{Cons, Context, Layer, Nil, run_blocking};
use id_effect_platform::fs::{FileSystem, FileSystemKey, TestFileSystem, layer_file_system, read};

type Env = Context<Cons<id_effect::Service<FileSystemKey, TestFileSystem>, Nil>>;

#[test]
fn test_fs_roundtrip() {
  let fs = TestFileSystem::new();
  let stack = layer_file_system(fs.clone());
  let svc = stack.build().unwrap();
  let env = Context::new(Cons(svc, Nil));
  run_blocking(FileSystem::write(&fs, Path::new("a/b.txt"), b"hi"), ()).unwrap();
  let bytes = run_blocking(read::<Env, _>("a/b.txt".into()), env).unwrap();
  assert_eq!(bytes, b"hi");
}
