// Copyright 2016 Kyle Mayes
//
// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at
//
//     http://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.

//! Provides helper functionality.

use std::env;
use std::process::{Command};
use std::path::{Path, PathBuf};

use glob;

use libc::{c_int};

use super::{CXVersion};

//================================================
// Macros
//================================================

// try_opt! ______________________________________

macro_rules! try_opt {
    ($option:expr) => ({
        match $option {
            Some(some) => some,
            None => return None,
        }
    });
}

//================================================
// Structs
//================================================

/// A `clang` executable.
#[derive(Clone, Debug)]
pub struct Clang {
    /// The path to this `clang` executable.
    pub path: PathBuf,
    /// The version of this `clang` executable if it could be parsed.
    pub version: Option<CXVersion>,
    /// The directories searched by this `clang` executable for C headers.
    pub c_search_paths: Vec<PathBuf>,
    /// The directories searched by this `clang` executable for C++ headers.
    pub cpp_search_paths: Vec<PathBuf>,
    /// The directories searched by this `clang` executable for Objective-C headers.
    pub objc_search_paths: Vec<PathBuf>,
}

impl Clang {
    //- Constructors -----------------------------

    fn new(path: PathBuf) -> Clang {
        let version = parse_version(&path);
        let c_search_paths = parse_search_paths(&path, "c");
        let cpp_search_paths = parse_search_paths(&path, "c++");
        let objc_search_paths = parse_search_paths(&path, "c++");
        Clang {
            path: path,
            version: version,
            c_search_paths: c_search_paths,
            cpp_search_paths: cpp_search_paths,
            objc_search_paths: objc_search_paths,
        }
    }

    /// Returns a `clang` executable if one can be found.
    ///
    /// If the `CLANG_PATH` environment variable is set, that is the instance of `clang` used.
    /// Otherwise, a series of directories are searched. First, If a path is supplied, that is the
    /// first directory searched. Then, the directory returned by `llvm-config --bindir` is
    /// searched. On OS X systems, `xcodebuild -find clang` will next be queried. Last, the
    /// directories in the system's `PATH` are searched.
    pub fn find(path: Option<&Path>) -> Option<Clang> {
        if let Ok(path) = env::var("CLANG_PATH") {
            return Some(Clang::new(path.into()));
        }

        let mut paths = vec![];
        if let Some(path) = path {
            paths.push(path.into());
        }
        if let Ok(path) = run_llvm_config(&["--bindir"]) {
            paths.push(path.into());
        }
        if cfg!(target_os="macos") {
            if let Ok((path, _)) = run("xcodebuild", &["-find", "clang"]) {
                paths.push(path.into());
            }
        }
        paths.extend(env::split_paths(&env::var("PATH").unwrap()));

        let default = format!("clang{}", env::consts::EXE_SUFFIX);
        let versioned = format!("clang-[0-9]*{}", env::consts::EXE_SUFFIX);
        let patterns = &[&default[..], &versioned[..]];
        for path in paths {
            if let Some(path) = find(&path, patterns) {
                return Some(Clang::new(path));
            }
        }
        None
    }
}

//================================================
// Functions
//================================================

/// Returns the first match to the supplied glob patterns in the supplied directory if there are any
/// matches.
fn find(directory: &Path, patterns: &[&str]) -> Option<PathBuf> {
    for pattern in patterns {
        let pattern = directory.join(pattern).to_string_lossy().into_owned();
        if let Some(path) = try_opt!(glob::glob(&pattern).ok()).filter_map(|p| p.ok()).next() {
            return Some(path);
        }
    }
    None
}

/// Attempts to run an executable, returning the `stdout` and `stderr` output if successful.
fn run(executable: &str, arguments: &[&str]) -> Result<(String, String), String> {
    Command::new(executable).args(arguments).output().map(|o| {
        let stdout = String::from_utf8_lossy(&o.stdout).into_owned();
        let stderr = String::from_utf8_lossy(&o.stderr).into_owned();
        (stdout, stderr)
    }).map_err(|_| format!("could not run executable: `{}`", executable))
}

/// Runs `clang`, returning the `stdout` and `stderr` output.
fn run_clang(path: &Path, arguments: &[&str]) -> (String, String) {
    run(&path.to_string_lossy().into_owned(), arguments).unwrap()
}

/// Runs `llvm-config`, returning the `stdout` output if successful.
fn run_llvm_config(arguments: &[&str]) -> Result<String, String> {
    run(&env::var("LLVM_CONFIG_PATH").unwrap_or("llvm-config".into()), arguments).map(|(o, _)| o)
}

/// Parses a version number if possible, ignoring trailing non-digit characters.
fn parse_version_number(number: &str) -> Option<c_int> {
    number.chars().take_while(|c| c.is_digit(10)).collect::<String>().parse().ok()
}

/// Parses the version from the output of a `clang` executable if possible.
fn parse_version(path: &Path) -> Option<CXVersion> {
    let output = run_clang(path, &["--version"]).0;
    let start = try_opt!(output.find("version ")) + 8;
    let mut numbers = try_opt!(output[start..].split_whitespace().nth(0)).split('.');
    let major = try_opt!(numbers.next().and_then(parse_version_number));
    let minor = try_opt!(numbers.next().and_then(parse_version_number));
    let subminor = numbers.next().and_then(parse_version_number).unwrap_or(0);
    Some(CXVersion { Major: major, Minor: minor, Subminor: subminor })
}

/// Parses the search paths from the output of a `clang` executable.
fn parse_search_paths(path: &Path, language: &str) -> Vec<PathBuf> {
    let output = run_clang(path, &["-E", "-x", language, "-", "-v"]).1;
    let include_start = "#include <...> search starts here:";
    let start = output.find(include_start).expect(include_start) + include_start.len();
    let end = output.find("End of search list.").expect("End of search list");
    let paths = output[start..end].replace("(framework directory)", "");
    paths.lines().filter(|l| !l.is_empty()).map(|l| Path::new(l.trim()).into()).collect()
}
