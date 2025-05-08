use clap::Parser;
use core::str;
use dixlib::error::AppError;
use dixlib::print;
use dixlib::store;
use log::{debug, error};
use regex::Regex;
use std::{
    collections::{HashMap, HashSet},
    string::ToString,
    sync::OnceLock,
    thread,
};
use yansi::Paint;

// Use type alias for Result with our custom error type
type Result<T> = std::result::Result<T, AppError>;

#[derive(Parser, Debug)]
#[command(name = "dix")]
#[command(version = "1.0")]
#[command(about = "Diff Nix stuff", long_about = None)]
#[command(version, about, long_about = None)]
struct Args {
    path: std::path::PathBuf,
    path2: std::path::PathBuf,

    /// Print the whole store paths
    #[arg(short, long)]
    paths: bool,

    /// Print the closure size
    #[arg(long, short)]
    closure_size: bool,

    /// Verbosity level: -v for debug, -vv for trace
    #[arg(short, long, action = clap::ArgAction::Count)]
    verbose: u8,

    /// Silence all output except errors
    #[arg(short, long)]
    quiet: bool,
}

#[derive(Debug, Clone)]
struct Package<'a> {
    name: &'a str,
    versions: HashSet<&'a str>,
    /// Save if a package is a dependency of another package
    is_dep: bool,
}

impl<'a> Package<'a> {
    fn new(name: &'a str, version: &'a str, is_dep: bool) -> Self {
        let mut versions = HashSet::new();
        versions.insert(version);
        Self {
            name,
            versions,
            is_dep,
        }
    }

    fn add_version(&mut self, version: &'a str) {
        self.versions.insert(version);
    }
}

#[allow(clippy::cognitive_complexity, clippy::too_many_lines)]
fn main() {
    let args = Args::parse();

    // Configure logger based on verbosity flags and environment variables
    // Respects RUST_LOG environment variable if present.
    // XXX:We can also dedicate a specific env variable for this tool, if we want to.
    let env = env_logger::Env::default().filter_or(
        "RUST_LOG",
        if args.quiet {
            "error"
        } else {
            match args.verbose {
                0 => "info",
                1 => "debug",
                _ => "trace",
            }
        },
    );

    // Build and initialize the logger
    env_logger::Builder::from_env(env)
        .format_timestamp(Some(env_logger::fmt::TimestampPrecision::Seconds))
        .init();

    println!("<<< {}", args.path.to_string_lossy());
    println!(">>> {}", args.path2.to_string_lossy());

    // handles to the threads collecting closure size information
    // We do this as early as possible because nix is slow.
    let closure_size_handles = if args.closure_size {
        debug!("Calculating closure sizes in background");
        let path = args.path.clone();
        let path2 = args.path2.clone();
        Some((
            thread::spawn(move || store::get_closure_size(&path)),
            thread::spawn(move || store::get_closure_size(&path2)),
        ))
    } else {
        None
    };

    // Get package lists and handle potential errors
    let package_list_pre = match store::get_packages(&args.path) {
        Ok(packages) => {
            debug!("Found {} packages in first closure", packages.len());
            packages.into_iter().map(|(_, path)| path).collect()
        }
        Err(e) => {
            error!(
                "Error getting packages from path {}: {}",
                args.path.display(),
                e
            );
            eprintln!(
                "Error getting packages from path {}: {}",
                args.path.display(),
                e
            );
            Vec::new()
        }
    };

    let package_list_post = match store::get_packages(&args.path2) {
        Ok(packages) => {
            debug!("Found {} packages in second closure", packages.len());
            packages.into_iter().map(|(_, path)| path).collect()
        }
        Err(e) => {
            error!(
                "Error getting packages from path {}: {}",
                args.path2.display(),
                e
            );
            eprintln!(
                "Error getting packages from path {}: {}",
                args.path2.display(),
                e
            );
            Vec::new()
        }
    };

    // Map from packages of the first closure to their version
    let mut pre = HashMap::<&str, HashSet<&str>>::new();
    let mut post = HashMap::<&str, HashSet<&str>>::new();

    for p in &package_list_pre {
        match get_version(&**p) {
            Ok((name, version)) => {
                pre.entry(name).or_default().insert(version);
            }
            Err(e) => {
                debug!("Error parsing package version: {e}");
            }
        }
    }

    for p in &package_list_post {
        match get_version(&**p) {
            Ok((name, version)) => {
                post.entry(name).or_default().insert(version);
            }
            Err(e) => {
                debug!("Error parsing package version: {e}");
            }
        }
    }

    // Compare the package names of both versions
    let pre_keys: HashSet<&str> = pre.keys().copied().collect();
    let post_keys: HashSet<&str> = post.keys().copied().collect();

    // Difference gives us added and removed packages
    let added: HashSet<&str> = &post_keys - &pre_keys;

    let removed: HashSet<&str> = &pre_keys - &post_keys;
    // Get the intersection of the package names for version changes
    let changed: HashSet<&str> = &pre_keys & &post_keys;

    debug!("Added packages: {}", added.len());
    debug!("Removed packages: {}", removed.len());
    debug!(
        "Changed packages: {}",
        changed
            .iter()
            .filter(|p| !p.is_empty()
                && match (pre.get(*p), post.get(*p)) {
                    (Some(ver_pre), Some(ver_post)) => ver_pre != ver_post,
                    _ => false,
                })
            .count()
    );

    println!("Difference between the two generations:");
    println!();

    let width_changes = changed
        .iter()
        .filter(|&&p| match (pre.get(p), post.get(p)) {
            (Some(version_pre), Some(version_post)) => version_pre != version_post,
            _ => false,
        });

    let col_width = added
        .iter()
        .chain(removed.iter())
        .chain(width_changes)
        .map(|p| p.len())
        .max()
        .unwrap_or_default();

    print::print_added(&added, &post, col_width);
    print::print_removed(&removed, &pre, col_width);
    print::print_changes(&changed, &pre, &post, col_width);

    if let Some((pre_handle, post_handle)) = closure_size_handles {
        match (pre_handle.join(), post_handle.join()) {
            (Ok(Ok(pre_size)), Ok(Ok(post_size))) => {
                let pre_size = pre_size / 1024 / 1024;
                let post_size = post_size / 1024 / 1024;
                debug!("Pre closure size: {pre_size} MiB");
                debug!("Post closure size: {post_size} MiB");

                println!("{}", "Closure Size:".underline().bold());
                println!("Before: {pre_size} MiB");
                println!("After: {post_size} MiB");
                println!("Difference: {} MiB", post_size - pre_size);
            }
            (Ok(Err(e)), _) | (_, Ok(Err(e))) => {
                error!("Error getting closure size: {e}");
                eprintln!("Error getting closure size: {e}");
            }
            _ => {
                error!("Failed to get closure size information due to a thread error");
                eprintln!("Error: Failed to get closure size information due to a thread error");
            }
        }
    }
}

// Returns a reference to the compiled regex pattern.
// The regex is compiled only once.
fn store_path_regex() -> &'static Regex {
    static REGEX: OnceLock<Regex> = OnceLock::new();
    REGEX.get_or_init(|| {
        Regex::new(r"(.+?)(-([0-9].*?))?$")
            .expect("Failed to compile regex pattern for nix store paths")
    })
}

/// Parses a nix store path to extract the packages name and version
///
/// This function first drops the inputs first 44 chars, since that is exactly the length of the /nix/store/... prefix. Then it matches that against our store path regex.
///
/// # Returns
///
/// * Result<(&'a str, &'a str)> - The Package's name and version, or an error if
///   one or both cannot be retrieved.
fn get_version<'a>(pack: impl Into<&'a str>) -> Result<(&'a str, &'a str)> {
    let path = pack.into();

    // We can strip the path since it _always_ follows the format
    // /nix/store/<...>-<program_name>-......
    // This part is exactly 44 chars long, so we just remove it.
    let stripped_path = &path[44..];
    debug!("Stripped path: {stripped_path}");

    // Match the regex against the input
    if let Some(cap) = store_path_regex().captures(stripped_path) {
        // Handle potential missing captures safely
        let name = cap.get(1).map_or("", |m| m.as_str());
        let mut version = cap.get(2).map_or("<none>", |m| m.as_str());

        if version.starts_with('-') {
            version = &version[1..];
        }

        if name.is_empty() {
            return Err(AppError::ParseError {
                message: format!("Failed to extract name from path: {path}"),
                context: "get_version".to_string(),
                source: None,
            });
        }

        return Ok((name, version));
    }

    Err(AppError::ParseError {
        message: format!("Path does not match expected nix store format: {path}"),
        context: "get_version".to_string(),
        source: None,
    })
}
