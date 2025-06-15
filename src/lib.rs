use std::{
  path::PathBuf,
  sync,
};

use anyhow::{
  Context as _,
  Error,
  Result,
  anyhow,
  bail,
};
use derive_more::Deref;

mod diff;
pub use diff::{
  spawn_size_diff,
  write_paths_diffln,
  write_size_diffln,
};

mod store;

mod version;
use version::Version;

#[cfg(test)] mod tests;

#[derive(Deref, Debug, Clone, Copy, PartialEq, Eq, Hash)]
struct DerivationId(i64);

/// A validated store path. Always starts with `/nix/store`.
///
/// Can be created using `StorePath::try_from(path_buf)`.
#[derive(Deref, Debug, Clone, PartialEq, Eq, Hash)]
pub struct StorePath(PathBuf);

impl TryFrom<PathBuf> for StorePath {
  type Error = Error;

  fn try_from(path: PathBuf) -> Result<Self> {
    if !path.starts_with("/nix/store") {
      bail!(
        "path {path} must start with /nix/store",
        path = path.display(),
      );
    }

    Ok(Self(path))
  }
}

impl StorePath {
  /// Parses a Nix store path to extract the packages name and possibly its
  /// version.
  ///
  /// This function first drops the inputs first 44 chars, since that is exactly
  /// the length of the `/nix/store/0004yybkm5hnwjyxv129js3mjp7kbrax-` prefix.
  /// Then it matches that against our store path regex.
  fn parse_name_and_version(&self) -> Result<(&str, Option<Version>)> {
    static STORE_PATH_REGEX: sync::LazyLock<regex::Regex> =
      sync::LazyLock::new(|| {
        regex::Regex::new("(.+?)(-([0-9].*?))?$")
          .expect("failed to compile regex for Nix store paths")
      });

    let path = self.to_str().with_context(|| {
      format!(
        "failed to convert path '{path}' to valid unicode",
        path = self.display(),
      )
    })?;

    // We can strip the path since it _always_ follows the format:
    //
    // /nix/store/0004yybkm5hnwjyxv129js3mjp7kbrax-...
    // ^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^
    // This part is exactly 44 chars long, so we just remove it.
    assert_eq!(&path[..11], "/nix/store/");
    assert_eq!(&path[43..44], "-");
    let path = &path[44..];

    log::debug!("stripped path: {path}");

    let captures = STORE_PATH_REGEX.captures(path).ok_or_else(|| {
      anyhow!("path '{path}' does not match expected Nix store format")
    })?;

    let name = captures.get(1).map_or("", |capture| capture.as_str());
    if name.is_empty() {
      bail!("failed to extract name from path '{path}'");
    }

    let version: Option<Version> = captures.get(2).map(|capture| {
      Version::from(capture.as_str().trim_start_matches('-').to_owned())
    });

    Ok((name, version))
  }
}
