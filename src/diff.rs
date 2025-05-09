use std::{
  collections::HashMap,
  fmt::{
    self,
    Write as _,
  },
  path::{
    Path,
    PathBuf,
  },
  thread,
};

use anyhow::{
  Context as _,
  Error,
  Result,
};
use itertools::{
  EitherOrBoth,
  Itertools,
};
use size::Size;
use unicode_width::UnicodeWidthStr as _;
use yansi::Paint as _;

use crate::{
  StorePath,
  Version,
  store,
};

#[derive(Debug, Default)]
struct Diff<T> {
  old: T,
  new: T,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
enum DiffStatus {
  Added,
  Removed,
  Changed,
}

impl DiffStatus {
  fn char(self) -> impl fmt::Display {
    match self {
      Self::Added => "A".green(),
      Self::Removed => "R".red(),
      Self::Changed => "C".yellow(),
    }
  }
}

/// Writes the diff header (<<< out, >>>in) and package diff.
///
/// Returns the amount of package diffs written. Even when zero, the header will
/// be written.
pub fn write_paths_diffln(
  writer: &mut impl fmt::Write,
  path_old: &Path,
  path_new: &Path,
) -> Result<usize> {
  let mut connection = store::connect()?;

  let paths_old = connection.query_dependents(path_old).with_context(|| {
    format!(
      "failed to query dependencies of path '{path}'",
      path = path_old.display()
    )
  })?;

  log::info!(
    "found {count} packages in old closure",
    count = paths_old.len(),
  );

  let paths_new = connection.query_dependents(path_new).with_context(|| {
    format!(
      "failed to query dependencies of path '{path}'",
      path = path_new.display()
    )
  })?;
  log::info!(
    "found {count} packages in new closure",
    count = paths_new.len(),
  );

  drop(connection);

  writeln!(
    writer,
    "{arrows} {old}",
    arrows = "<<<".bold(),
    old = path_old.display(),
  )?;
  writeln!(
    writer,
    "{arrows} {new}",
    arrows = ">>>".bold(),
    new = path_new.display(),
  )?;

  writeln!(writer)?;

  #[expect(clippy::pattern_type_mismatch)]
  Ok(write_packages_diffln(
    writer,
    paths_old.iter().map(|(_, path)| path),
    paths_new.iter().map(|(_, path)| path),
  )?)
}

fn deduplicate_versions(versions: &mut Vec<Version>) {
  versions.sort_unstable();

  let mut deduplicated = Vec::new();
  let mut deduplicated_push = |mut version: Version, count: usize| {
    if count > 1 {
      write!(version, " * {count}").unwrap();
    }
    deduplicated.push(version);
  };

  let mut last_version = None::<(Version, usize)>;
  for version in versions.iter() {
    #[expect(clippy::mixed_read_write_in_expression)]
    let Some((last_version_value, count)) = last_version.take() else {
      last_version = Some((version.clone(), 1));
      continue;
    };

    if last_version_value == *version {
      last_version = Some((last_version_value, count + 1));
    } else {
      deduplicated_push(last_version_value, count);
    }
  }

  if let Some((version, count)) = last_version.take() {
    deduplicated_push(version, count);
  }

  *versions = deduplicated;
}

fn write_packages_diffln<'a>(
  writer: &mut impl fmt::Write,
  paths_old: impl Iterator<Item = &'a StorePath>,
  paths_new: impl Iterator<Item = &'a StorePath>,
) -> Result<usize, fmt::Error> {
  let mut paths = HashMap::<&str, Diff<Vec<Version>>>::new();

  for path in paths_old {
    match path.parse_name_and_version() {
      Ok((name, version)) => {
        log::debug!("parsed name: {name}");
        log::debug!("parsed version: {version:?}");

        paths
          .entry(name)
          .or_default()
          .old
          .push(version.unwrap_or(Version::from("<none>".to_owned())));
      },

      Err(error) => {
        log::warn!("error parsing old path name and version: {error}");
      },
    }
  }

  for path in paths_new {
    match path.parse_name_and_version() {
      Ok((name, version)) => {
        log::debug!("parsed name: {name}");
        log::debug!("parsed version: {version:?}");

        paths
          .entry(name)
          .or_default()
          .new
          .push(version.unwrap_or(Version::from("<none>".to_owned())));
      },

      Err(error) => {
        log::warn!("error parsing new path name and version: {error}");
      },
    }
  }

  let mut diffs = paths
    .into_iter()
    .filter_map(|(name, mut versions)| {
      deduplicate_versions(&mut versions.old);
      deduplicate_versions(&mut versions.new);

      let status = match (versions.old.len(), versions.new.len()) {
        (0, 0) => unreachable!(),
        (0, _) => DiffStatus::Added,
        (_, 0) => DiffStatus::Removed,
        (..) if versions.old != versions.new => DiffStatus::Changed,
        (..) => return None,
      };

      Some((name, versions, status))
    })
    .collect::<Vec<_>>();

  diffs.sort_by(|&(a_name, _, a_status), &(b_name, _, b_status)| {
    a_status.cmp(&b_status).then_with(|| a_name.cmp(b_name))
  });

  let name_width = diffs
    .iter()
    .map(|&(name, ..)| name.width())
    .max()
    .unwrap_or(0);

  let mut last_status = None::<DiffStatus>;

  for &(name, ref versions, status) in &diffs {
    if last_status != Some(status) {
      writeln!(
        writer,
        "{nl}{status}",
        nl = if last_status.is_some() { "\n" } else { "" },
        status = match status {
          DiffStatus::Added => "ADDED",
          DiffStatus::Removed => "REMOVED",
          DiffStatus::Changed => "CHANGED",
        }
        .bold(),
      )?;

      last_status = Some(status);
    }

    write!(
      writer,
      "[{status}] {name:<name_width$}",
      status = status.char()
    )?;

    let mut oldacc = String::new();
    let mut oldwrote = false;
    let mut newacc = String::new();
    let mut newwrote = false;

    for diff in Itertools::zip_longest(versions.old.iter(), versions.new.iter())
    {
      match diff {
        EitherOrBoth::Left(old_version) => {
          if oldwrote {
            write!(oldacc, ", ")?;
          } else {
            write!(oldacc, " ")?;
            oldwrote = true;
          }

          for old_comp in old_version {
            match old_comp {
              Ok(old_comp) => write!(oldacc, "{old}", old = old_comp.red())?,
              Err(ignored) => write!(oldacc, "{ignored}")?,
            }
          }
        },

        EitherOrBoth::Right(new_version) => {
          if newwrote {
            write!(newacc, ", ")?;
          } else {
            write!(newacc, " ")?;
            newwrote = true;
          }

          for new_comp in new_version {
            match new_comp {
              Ok(new_comp) => write!(newacc, "{new}", new = new_comp.green())?,
              Err(ignored) => write!(newacc, "{ignored}")?,
            }
          }
        },

        EitherOrBoth::Both(old_version, new_version) => {
          if old_version == new_version {
            continue;
          }

          if oldwrote {
            write!(oldacc, ", ")?;
          } else {
            write!(oldacc, " ")?;
            oldwrote = true;
          }
          if newwrote {
            write!(newacc, ", ")?;
          } else {
            write!(newacc, " ")?;
            newwrote = true;
          }

          for diff in Itertools::zip_longest(
            old_version.into_iter(),
            new_version.into_iter(),
          ) {
            match diff {
              EitherOrBoth::Left(old_comp) => {
                match old_comp {
                  Ok(old_comp) => {
                    write!(oldacc, "{old}", old = old_comp.red())?;
                  },
                  Err(ignored) => {
                    write!(oldacc, "{ignored}")?;
                  },
                }
              },

              EitherOrBoth::Right(new_comp) => {
                match new_comp {
                  Ok(new_comp) => {
                    write!(newacc, "{new}", new = new_comp.green())?;
                  },
                  Err(ignored) => {
                    write!(newacc, "{ignored}")?;
                  },
                }
              },

              EitherOrBoth::Both(old_comp, new_comp) => {
                if let Err(ignored) = old_comp {
                  write!(oldacc, "{ignored}")?;
                }

                if let Err(ignored) = new_comp {
                  write!(newacc, "{ignored}")?;
                }

                if let (Ok(old_comp), Ok(new_comp)) = (old_comp, new_comp) {
                  if old_comp == new_comp {
                    write!(oldacc, "{old}", old = old_comp.yellow())?;
                    write!(newacc, "{new}", new = new_comp.yellow())?;
                  } else {
                    write!(oldacc, "{old}", old = old_comp.red())?;
                    write!(newacc, "{new}", new = new_comp.green())?;
                  }
                }
              },
            }
          }
        },
      }
    }

    write!(
      writer,
      "{oldacc}{arrow}{newacc}",
      arrow = if !oldacc.is_empty() && !newacc.is_empty() {
        " ->"
      } else {
        ""
      }
    )?;

    writeln!(writer)?;
  }

  Ok(diffs.len())
}

/// Spawns a task to compute the data required by [`write_size_diffln`].
#[must_use]
pub fn spawn_size_diff(
  path_old: PathBuf,
  path_new: PathBuf,
) -> thread::JoinHandle<Result<(Size, Size)>> {
  log::debug!("calculating closure sizes in background");

  thread::spawn(move || {
    let mut connection = store::connect()?;

    Ok::<_, Error>((
      connection.query_closure_size(&path_old)?,
      connection.query_closure_size(&path_new)?,
    ))
  })
}

/// Writes the size difference.
pub fn write_size_diffln(
  writer: &mut impl fmt::Write,
  size_old: Size,
  size_new: Size,
) -> fmt::Result {
  let size_diff = size_new - size_old;

  writeln!(
    writer,
    "{header}: {size_old} -> {size_new}",
    header = "SIZE".bold(),
    size_old = size_old.red(),
    size_new = size_new.green(),
  )?;

  writeln!(
    writer,
    "{header}: {size_diff}",
    header = "DIFF".bold(),
    size_diff = if size_diff.bytes() > 0 {
      size_diff.green()
    } else {
      size_diff.red()
    },
  )
}
