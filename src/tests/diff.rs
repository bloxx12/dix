use std::{
  collections::{
    HashMap,
    HashSet,
  },
  fmt,
  io,
  path::PathBuf,
};

use size::Size;

use crate::{
  StorePath,
  Version,
  diff::{
    self,
    Diff,
  },
};

struct WriteFmt<W: io::Write>(W);

impl<W: io::Write> fmt::Write for WriteFmt<W> {
  fn write_str(&mut self, string: &str) -> fmt::Result {
    self.0.write_all(string.as_bytes()).map_err(|_| fmt::Error)
  }
}

#[test]
fn test_deduplicate_versions() {
  let mut versions_pre: Vec<Version> = vec![
    "2.3".into(),
    "1.0".into(),
    "2.3".into(),
    "1.0.18343".into(),
    "1.0.18343".into(),
    "1.823.348".into(),
    "1.0.18343".into(),
    "1.823.348".into(),
    "A".into(),
    "B".into(),
    "B".into(),
    "1.823.348".into(),
    "2.3".into(),
    "1.0.18343".into(),
    "1.823.348".into(),
    "4.8".into(),
    "1.823.348".into(),
    "1.0".into(),
    "0".into(),
  ];
  let versions_post: Vec<Version> = vec![
    "A".into(),
    "B ×2".into(),
    "0".into(),
    "1.0 ×2".into(),
    "1.0.18343 ×4".into(),
    "1.823.348 ×5".into(),
    "2.3 ×3".into(),
    "4.8".into(),
  ];
  diff::deduplicate_versions(&mut versions_pre);

  assert_eq!(versions_pre, versions_post);
}

#[test]
fn test_deduplicate_versions_empty() {
  let mut versions_pre: Vec<Version> = vec![];
  let versions_post: Vec<Version> = vec![];
  diff::deduplicate_versions(&mut versions_pre);

  assert_eq!(versions_pre, versions_post);
}

#[test]
fn test_write_size_diffln() {
  let size_old = Size::from_bytes(10_i32);
  let size_new = Size::from_bytes(20_i32);

  let expected_output =
    "\u{1b}[1mSIZE\u{1b}[0m: \u{1b}[31m10 bytes\u{1b}[0m -> \u{1b}[32m20 \
     bytes\u{1b}[0m\n\u{1b}[1mDIFF\u{1b}[0m: \u{1b}[32m10 bytes\u{1b}[0m\n";

  let mut buf = WriteFmt(io::BufWriter::new(Vec::new()));

  let _ = diff::write_size_diffln(&mut buf, size_old, size_new);

  let buf_writer = buf.0;

  let vec = buf_writer.into_inner().expect("Failed to unwrap BufWriter");
  let result = String::from_utf8(vec).expect("Invalid UTF-8");

  assert_eq!(result, expected_output);
}

#[test]
fn test_write_size_diffln_empty() {
  let size_old = Size::from_bytes(0_i32);
  let size_new = Size::from_bytes(0_i32);

  let expected_output =
    "\u{1b}[1mSIZE\u{1b}[0m: \u{1b}[31m0 bytes\u{1b}[0m -> \u{1b}[32m0 \
     bytes\u{1b}[0m\n\u{1b}[1mDIFF\u{1b}[0m: \u{1b}[31m0 bytes\u{1b}[0m\n";
  let mut buf = WriteFmt(io::BufWriter::new(Vec::new()));

  let _ = diff::write_size_diffln(&mut buf, size_old, size_new);

  let buf_writer = buf.0;

  let vec = buf_writer.into_inner().expect("Failed to unwrap BufWriter");
  let result = String::from_utf8(vec).expect("Invalid UTF-8");

  assert_eq!(result, expected_output);
}

#[test]
fn test_get_status_from_versions() {
  let versions_1: Diff<Vec<Version>> = Diff {
    old: vec!["1.0".into()],
    new: vec!["1.0".into()],
  };
  let versions_2: Diff<Vec<Version>> = Diff {
    old: vec![],
    new: vec!["1.0".into()],
  };
  let versions_3: Diff<Vec<Version>> = Diff {
    old: vec!["1.0".into()],
    new: vec![],
  };
  let versions_4: Diff<Vec<Version>> = Diff {
    old: vec!["1.0".into()],
    new: vec!["1.1".into()],
  };
  let versions_5: Diff<Vec<Version>> = Diff {
    old: vec!["1.0".into()],
    new: vec!["0.9".into()],
  };
  let versions_6: Diff<Vec<Version>> = Diff {
    old: vec!["1.0".into(), "2.0".into()],
    new: vec!["1.1".into(), "1.9".into()],
  };

  assert_eq!(diff::get_status_from_versions(&versions_1), None);
  assert_eq!(
    diff::get_status_from_versions(&versions_2),
    Some(diff::DiffStatus::Added)
  );
  assert_eq!(
    diff::get_status_from_versions(&versions_3),
    Some(diff::DiffStatus::Removed)
  );
  assert_eq!(
    diff::get_status_from_versions(&versions_4),
    Some(diff::DiffStatus::Changed(diff::Change::Upgraded))
  );
  assert_eq!(
    diff::get_status_from_versions(&versions_5),
    Some(diff::DiffStatus::Changed(diff::Change::Downgraded))
  );
  assert_eq!(
    diff::get_status_from_versions(&versions_6),
    Some(diff::DiffStatus::Changed(diff::Change::UpgradeDowngrade))
  );
}

#[test]
fn test_push_parsed_name_and_version_old() {
  let path: StorePath = StorePath::try_from(PathBuf::from(
    "/nix/store/cg09nslw3w6afyynjw484b86d47ic1cb-coreutils-9.7",
  ))
  .expect("Could not create path");

  let mut paths: HashMap<String, Diff<Vec<Version>>> = HashMap::new();
  let () = diff::push_parsed_name_and_version_old(&path, &mut paths);

  assert_eq!(paths.keys().count(), 1);
  assert_eq!(paths.values().count(), 1);
  assert_eq!(paths.keys().take(1).next().unwrap(), "coreutils");
  assert_eq!(paths.values().take(1).next().unwrap().old, vec![
    "9.7".into()
  ]);
}

#[test]
fn test_push_parsed_name_and_version_new() {
  let path: StorePath = StorePath::try_from(PathBuf::from(
    "/nix/store/6d4dp25lani18z9sbnb5shwzzc3y5yh8-bacon-3.12.0",
  ))
  .expect("Could not create path");

  let mut paths: HashMap<String, Diff<Vec<Version>>> = HashMap::new();
  let () = diff::push_parsed_name_and_version_new(&path, &mut paths);

  assert_eq!(paths.keys().count(), 1);
  assert_eq!(paths.values().count(), 1);
  assert_eq!(paths.keys().take(1).next().unwrap(), "bacon");
  assert_eq!(paths.values().take(1).next().unwrap().new, vec![
    "3.12.0".into()
  ]);
}

#[test]
fn test_get_system_derivations() {
  let paths = vec![
    StorePath::try_from(PathBuf::from(
      "/nix/store/6d4dp25lani18z9sbnb5shwzzc3y5yh8-bacon-3.12.0",
    ))
    .unwrap(),
    StorePath::try_from(PathBuf::from(
      "/nix/store/cg09nslw3w6afyynjw484b86d47ic1cb-coreutils-9.7",
    ))
    .unwrap(),
    StorePath::try_from(PathBuf::from(
      "/nix/store/6d4dp25lani18z9sbnb5shwzzc3y5yh8-cool_package-189",
    ))
    .unwrap(),
  ];

  let mut names: HashSet<String> = HashSet::new();
  names.insert("coreutils".to_owned());
  names.insert("bacon".to_owned());
  names.insert("cool_package".to_owned());

  let system_paths = paths.into_iter();

  let bla = diff::get_system_derivations(system_paths);

  assert_eq!(bla, names);
}

#[test]
fn test_collect_diffs() {
  let old_versions: Vec<Version> = vec![
    "6.14.8".into(),
    "6.14.8-modules".into(),
    "6.14.8-modules-shrunk".into(),
    "11.1".into(),
    "11.1-man".into(),
  ];
  let new_versions: Vec<Version> = vec![
    "6.14.9".into(),
    "6.24.3-modules".into(),
    "6.12.9-modules-shrunk".into(),
    "11.0".into(),
    "11.3-man".into(),
  ];

  let versions = Diff {
    old: old_versions,
    new: new_versions,
  };

  let (diff_pre, diff_post) = diff::collect_diffs(&versions).unwrap();

  assert_ne!(diff_pre, diff_post);
  assert_eq!(diff_pre, diff_pre);
  assert_eq!(diff_post, diff_post);
}
