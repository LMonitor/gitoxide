use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use bstr::{BStr, BString, ByteSlice};
use once_cell::sync::Lazy;

/// Other places to find Git in.
#[cfg(windows)]
pub(super) static ALTERNATIVE_LOCATIONS: Lazy<Vec<PathBuf>> = Lazy::new(|| {
    vec![
        "C:/Program Files/Git/mingw64/bin".into(),
        "C:/Program Files (x86)/Git/mingw32/bin".into(),
    ]
});
#[cfg(not(windows))]
pub(super) static ALTERNATIVE_LOCATIONS: Lazy<Vec<PathBuf>> = Lazy::new(|| vec![]);

#[cfg(any(windows, test))] // So this can always be tested.
fn alternative_locations_from_env<F>(var_os_func: F) -> Vec<PathBuf>
where
    F: Fn(&str) -> Option<std::ffi::OsString>,
{
    // FIXME: Define pairs of the environment variable name and the path suffix to apply, or
    // something, because right now this new function is totally wrong because it returns the
    // program files directory paths instead of the Git for Windows bin directory paths under them.
    //
    // But I am not really sure what this should do to handle the ProgramFiles environment variable
    // and figure out if it is 64-bit or 32-bit -- which we need in order to know whether to append
    // `Git/mingw64/bin` or `Git/mingw32/bin`. We do need to look at the ProgramFiles environment
    // variable in addition to the other two preferable architecture-specific ones (for which the
    // mingw64 vs. mingw32 decision is easy), at least some of the time, for two reasons.
    //
    // First, we may be on a 32-bit system. Then we do not have either of the other two variables.
    //
    // Second, the others may also absent on a 64-bit system, if a parent process whittles down the
    // variables to pass to this child process, out of an erroneous belief about which ones are
    // needed to provide access to the program files directory.
    //
    // On a 64-bit system, the way a child process inherits its ProgramFiles variable to be for its
    // own architecture -- since after all its parent's architecture could be different -- is:
    //
    //  - The value of a 64-bit child's ProgramFiles variable comes from whatever the parent passed
    //    down as ProgramW6432, if that variable was passed down.
    //
    //  - The value of a 32-bit child's ProgramFiles variable comes from whatever the parent passed
    //    down as ProgramFiles(x86), if that variable was passed down.
    //
    //  - If the variable corresponding to the child's architecture was not passed down by the
    //    parent, but ProgramFiles was passed down, then the child receives that as the value of
    //    its ProgramFiles variable. Only in this case does ProgramFiles come from ProgramFiles.
    //
    // The problem -- besides that those rules are not well known, and parent processes that seek
    // to pass down minimal environments often do not take heed of them, such that a child process
    // will get the wrong architecture's value for its ProgramFiles environment variable -- is that
    // the point of this function is to make use of environment variables in order to avoid:
    //
    //  - Calling Windows API functions explicitly, even via the higher level `windows` crate.
    //  - Accessing the Windows registry, even through the very widely used `winreg` crate.
    //  - Adding any significant new production dependencies, such as the `known-folders` crate.
    //
    // Possible solutions:
    //
    //  1. Abandon at least one of those goals.
    //  2. Check both `mingw*` subdirectories regardless of which program files Git is in.
    //  3. Inspect the names for substrings like ` (x86)` in an expected position.
    //  4. Assume the value of `ProgramFiles` is correct, i.e., is for the process's architecture.
    //
    // I think (4) is the way to go, at least until (1) is assessed. With (2), we would be checking
    // paths that there is no specific reason to think have *working* Git executables...though this
    // does have the advantage that its logic would be the same as would be needed in the local
    // program files directory (usually `%LocalAppData$/Programs`) if we ever add that. With (3),
    // the risk of getting it wrong is low, but the logic is more complex, and we lose the
    // simplicity of getting the paths from outside rather than applying assumptions about them.
    //
    // With (4), we take the ProgramFiles environment variable at its word. This is good not just
    // for abstract correctness, but also if the parent modifies these variables intentionally on a
    // 64-bit system. A parent process can't reasonably expect this to be followed, because a child
    // process may use another mechanism such as known folders. However, following it, when we are
    // using environment variables already, satisfies a weaker expectation that the environment
    // value *or* actual value (obtainable via known folders or the registry), rather than some
    // third value, is used. (4) is also a simple way to do the right thing on a 32-bit system.
    let suffix_x86 =
    let rules = [

    ];

    let names = [
        "ProgramW6432",      // 64-bit path from a 32-bit or 64-bit process on a 64-bit system.
        "ProgramFiles(x86)", // 32-bit path from a 32-bit or 64-bit process on a 64-bit system.
        "ProgramFiles",      // 32-bit path on 32-bit system. Or if the parent cleared the others.
    ];

    let mut locations = vec![];

    for path in names.into_iter().filter_map(var_os_func).map(PathBuf::from) {
        if !locations.contains(&path) {
            locations.push(path);
        }
    }

    locations
}

#[cfg(windows)]
pub(super) static EXE_NAME: &str = "git.exe";
#[cfg(not(windows))]
pub(super) static EXE_NAME: &str = "git";

/// Invoke the git executable in PATH to obtain the origin configuration, which is cached and returned.
pub(super) static EXE_INFO: Lazy<Option<BString>> = Lazy::new(|| {
    let git_cmd = |executable: PathBuf| {
        let mut cmd = Command::new(executable);
        cmd.args(["config", "-l", "--show-origin"])
            .stdin(Stdio::null())
            .stderr(Stdio::null());
        cmd
    };
    let mut cmd = git_cmd(EXE_NAME.into());
    gix_trace::debug!(cmd = ?cmd, "invoking git for installation config path");
    let cmd_output = match cmd.output() {
        Ok(out) => out.stdout,
        #[cfg(windows)]
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => {
            let executable = ALTERNATIVE_LOCATIONS.iter().find_map(|prefix| {
                let candidate = prefix.join(EXE_NAME);
                candidate.is_file().then_some(candidate)
            })?;
            gix_trace::debug!(cmd = ?cmd, "invoking git for installation config path in alternate location");
            git_cmd(executable).output().ok()?.stdout
        }
        Err(_) => return None,
    };

    first_file_from_config_with_origin(cmd_output.as_slice().into()).map(ToOwned::to_owned)
});

/// Returns the file that contains git configuration coming with the installation of the `git` file in the current `PATH`, or `None`
/// if no `git` executable was found or there were other errors during execution.
pub(super) fn install_config_path() -> Option<&'static BStr> {
    let _span = gix_trace::detail!("gix_path::git::install_config_path()");
    static PATH: Lazy<Option<BString>> = Lazy::new(|| {
        // Shortcut: in Msys shells this variable is set which allows to deduce the installation directory,
        // so we can save the `git` invocation.
        #[cfg(windows)]
        if let Some(mut exec_path) = std::env::var_os("EXEPATH").map(std::path::PathBuf::from) {
            exec_path.push("etc");
            exec_path.push("gitconfig");
            return crate::os_string_into_bstring(exec_path.into()).ok();
        }
        EXE_INFO.clone()
    });
    PATH.as_ref().map(AsRef::as_ref)
}

fn first_file_from_config_with_origin(source: &BStr) -> Option<&BStr> {
    let file = source.strip_prefix(b"file:")?;
    let end_pos = file.find_byte(b'\t')?;
    file[..end_pos].trim_with(|c| c == '"').as_bstr().into()
}

/// Given `config_path` as obtained from `install_config_path()`, return the path of the git installation base.
pub(super) fn config_to_base_path(config_path: &Path) -> &Path {
    config_path
        .parent()
        .expect("config file paths always have a file name to pop")
}

#[cfg(test)]
mod tests {
    use std::path::Path;

    #[test]
    fn config_to_base_path() {
        for (input, expected) in [
            (
                "/Applications/Xcode.app/Contents/Developer/usr/share/git-core/gitconfig",
                "/Applications/Xcode.app/Contents/Developer/usr/share/git-core",
            ),
            ("C:/git-sdk-64/etc/gitconfig", "C:/git-sdk-64/etc"),
            ("C:\\ProgramData/Git/config", "C:\\ProgramData/Git"),
            ("C:/Program Files/Git/etc/gitconfig", "C:/Program Files/Git/etc"),
        ] {
            assert_eq!(super::config_to_base_path(Path::new(input)), Path::new(expected));
        }
    }

    #[test]
    fn first_file_from_config_with_origin() {
        let macos = "file:/Applications/Xcode.app/Contents/Developer/usr/share/git-core/gitconfig	credential.helper=osxkeychain\nfile:/Users/byron/.gitconfig	push.default=simple\n";
        let win_msys =
            "file:C:/git-sdk-64/etc/gitconfig	core.symlinks=false\r\nfile:C:/git-sdk-64/etc/gitconfig	core.autocrlf=true";
        let win_cmd = "file:C:/Program Files/Git/etc/gitconfig	diff.astextplain.textconv=astextplain\r\nfile:C:/Program Files/Git/etc/gitconfig	filter.lfs.clean=gix-lfs clean -- %f\r\n";
        let win_msys_old = "file:\"C:\\ProgramData/Git/config\"	diff.astextplain.textconv=astextplain\r\nfile:\"C:\\ProgramData/Git/config\"	filter.lfs.clean=git-lfs clean -- %f\r\n";
        let linux = "file:/home/parallels/.gitconfig	core.excludesfile=~/.gitignore\n";
        let bogus = "something unexpected";
        let empty = "";

        for (source, expected) in [
            (
                macos,
                Some("/Applications/Xcode.app/Contents/Developer/usr/share/git-core/gitconfig"),
            ),
            (win_msys, Some("C:/git-sdk-64/etc/gitconfig")),
            (win_msys_old, Some("C:\\ProgramData/Git/config")),
            (win_cmd, Some("C:/Program Files/Git/etc/gitconfig")),
            (linux, Some("/home/parallels/.gitconfig")),
            (bogus, None),
            (empty, None),
        ] {
            assert_eq!(
                super::first_file_from_config_with_origin(source.into()),
                expected.map(Into::into)
            );
        }
    }

    #[cfg(windows)]
    use {
        known_folders::{get_known_folder_path, KnownFolder},
        std::ffi::{OsStr, OsString},
        std::io::ErrorKind,
        std::path::PathBuf,
        windows::core::Result as WindowsResult,
        windows::Win32::Foundation::BOOL,
        windows::Win32::System::Threading::{GetCurrentProcess, IsWow64Process},
        winreg::enums::{HKEY_LOCAL_MACHINE, KEY_QUERY_VALUE},
        winreg::RegKey,
    };

    #[cfg(windows)]
    trait Current: Sized {
        fn current() -> WindowsResult<Self>;
    }

    #[cfg(windows)]
    #[derive(Clone, Copy, Debug)]
    enum PlatformArchitecture {
        Is32on32,
        Is32on64,
        Is64on64,
    }

    #[cfg(windows)]
    impl Current for PlatformArchitecture {
        fn current() -> WindowsResult<Self> {
            // Ordinarily, we would check the target pointer width first to avoid doing extra work,
            // because if this is a 64-bit executable then the operating system is 64-bit. But this
            // is for the test suite, and doing it this way allows problems to be caught earlier if
            // a change made on a 64-bit development machine breaks the IsWow64Process() call.
            let mut wow64process = BOOL::default();
            unsafe { IsWow64Process(GetCurrentProcess(), &mut wow64process)? };

            let platform_architecture = if wow64process.as_bool() {
                Self::Is32on64
            } else if cfg!(target_pointer_width = "32") {
                Self::Is32on32
            } else {
                assert!(cfg!(target_pointer_width = "64"));
                Self::Is64on64
            };
            Ok(platform_architecture)
        }
    }

    #[cfg(windows)]
    fn ends_with_case_insensitive(text: &OsStr, suffix: &str) -> Option<bool> {
        Some(text.to_str()?.to_lowercase().ends_with(&suffix.to_lowercase()))
    }

    /// The common global program files paths on this system, by process and system architecture.
    #[cfg(windows)]
    #[derive(Clone, Debug)]
    struct ProgramFilesPaths {
        /// The program files directory used for whatever architecture this program was built for.
        current: PathBuf,

        /// The x86 program files directory regardless of the architecture of the program.
        ///
        /// If Rust gains Windows targets like ARMv7 where this is unavailable, this could fail.
        x86: PathBuf,

        /// The 64-bit program files directory if there is one.
        ///
        /// This is present on x64 and also ARM64 systems. On an ARM64 system, ARM64 and AMD64
        /// programs use the same program files directory while 32-bit x86 and ARM programs use two
        /// others. Only a 32-bit has no 64-bit program files directory.
        maybe_64bit: Option<PathBuf>,
    }

    impl ProgramFilesPaths {
        /// Gets the three common kinds of global program files paths without environment variables.
        ///
        /// The idea here is to obtain this information, which the `alternative_locations()` unit
        /// test uses to learn the expected alternative locations, without duplicating *any* of the
        /// approach used for `ALTERNATIVE_LOCATIONS`, so it can be used to test that. The approach
        /// here is also more reliable than using environment variables, but it is a bit more
        /// complex, and it requires either additional dependencies or the use of unsafe code.
        ///
        /// This gets `pf_current` and `pf_x86` by the [known folders][known-folders] system. But
        /// it gets `maybe_pf_64bit` from the registry, as the corresponding known folder is not
        /// available to 32-bit processes. See the [`KNOWNFOLDDERID`][knownfolderid] documentation.
        ///
        /// If in the future the implementation of `ALTERNATIVE_LOCATIONS` uses these techniques,
        /// then this function can be changed to use environment variables and renamed accordingly.
        ///
        /// [known-folders]: https://learn.microsoft.com/en-us/windows/win32/shell/known-folders
        /// [knownfolderid]: https://learn.microsoft.com/en-us/windows/win32/shell/knownfolderid#remarks
        fn obtain_envlessly() -> Self {
            let pf_current = get_known_folder_path(KnownFolder::ProgramFiles)
                .expect("The process architecture specific program files folder is always available.");

            let pf_x86 = get_known_folder_path(KnownFolder::ProgramFilesX86)
                .expect("The x86 program files folder will in practice always be available.");

            let maybe_pf_64bit = RegKey::predef(HKEY_LOCAL_MACHINE)
                .open_subkey_with_flags(r#"SOFTWARE\Microsoft\Windows\CurrentVersion"#, KEY_QUERY_VALUE)
                .expect("The `CurrentVersion` key exists and allows reading.")
                .get_value::<OsString, _>("ProgramW6432Dir")
                .map(PathBuf::from)
                .map_err(|error| {
                    assert_eq!(error.kind(), ErrorKind::NotFound);
                    error
                })
                .ok();

            Self {
                current: pf_current,
                x86: pf_x86,
                maybe_64bit: maybe_pf_64bit,
            }
        }

        /// Checks that the paths we got for testing are reasonable.
        ///
        /// This checks that `obtain_envlessly()` returned paths that are likely to be correct and
        /// that satisfy the most important properties based on the current system and process.
        fn validate(self) -> Self {
            match PlatformArchitecture::current().expect("Process and system 'bitness' should be available.") {
                PlatformArchitecture::Is32on32 => {
                    assert_eq!(
                        self.current.as_os_str(),
                        self.x86.as_os_str(),
                        "Our program files path is exactly identical to the 32-bit one.",
                    );
                    for arch_suffix in [" (x86)", " (Arm)"] {
                        let has_arch_suffix = ends_with_case_insensitive(self.current.as_os_str(), arch_suffix)
                            .expect("Assume the test system's important directories are valid Unicode.");
                        assert!(
                            !has_arch_suffix,
                            "The 32-bit program files directory name on a 32-bit system mentions no architecture.",
                        );
                    }
                    assert_eq!(
                        self.maybe_64bit, None,
                        "A 32-bit system has no 64-bit program files directory.",
                    );
                }
                PlatformArchitecture::Is32on64 => {
                    assert_eq!(
                        self.current.as_os_str(),
                        self.x86.as_os_str(),
                        "Our program files path is exactly identical to the 32-bit one.",
                    );
                    let pf_64bit = self
                        .maybe_64bit
                        .as_ref()
                        .expect("The 64-bit program files directory exists.");
                    assert_ne!(
                        &self.x86, pf_64bit,
                        "The 32-bit and 64-bit program files directories have different locations.",
                    );
                }
                PlatformArchitecture::Is64on64 => {
                    let pf_64bit = self
                        .maybe_64bit
                        .as_ref()
                        .expect("The 64-bit program files directory exists.");
                    assert_eq!(
                        self.current.as_os_str(),
                        pf_64bit.as_os_str(),
                        "Our program files path is exactly identical to the 64-bit one.",
                    );
                    assert_ne!(
                        &self.x86, pf_64bit,
                        "The 32-bit and 64-bit program files directories have different locations.",
                    );
                }
            }

            self
        }
    }

    /// Paths relative to process architecture specific program files directories.
    #[cfg(windows)]
    #[derive(Clone, Debug)]
    struct GitBinSuffixes<'a> {
        x86: &'a Path,
        maybe_64bit: Option<&'a Path>,
    }

    #[cfg(windows)]
    impl<'a> GitBinSuffixes<'a> {
        /// Assert that `locations` has the given prefixes, and extract the suffixes.
        fn assert_from(pf: &'a ProgramFilesPaths, locations: &'static [PathBuf]) -> Self {
            match locations {
                [primary, secondary] => {
                    let prefix_64bit = pf
                        .maybe_64bit
                        .as_ref()
                        .expect("It gives two paths only if one can be 64-bit.");
                    let suffix_64bit = primary
                        .strip_prefix(prefix_64bit)
                        .expect("It gives the 64-bit path and lists it first.");
                    let suffix_x86 = secondary
                        .strip_prefix(pf.x86.as_path())
                        .expect("It gives the 32-bit path and lists it second.");
                    Self {
                        x86: suffix_x86,
                        maybe_64bit: Some(suffix_64bit),
                    }
                }
                [only] => {
                    assert_eq!(pf.maybe_64bit, None, "It gives one path only if none can be 64-bit.");
                    Self {
                        x86: only,
                        maybe_64bit: None,
                    }
                }
                other => panic!("Got length {}, expected 1 or 2.", other.len()),
            }
        }

        /// Assert that the suffixes are the common per-architecture Git install locations.
        fn assert_architectures(&self) {
            assert_eq!(self.x86, Path::new("Git/mingw32/bin"));

            if let Some(suffix_64bit) = self.maybe_64bit {
                // When Git for Windows releases ARM64 builds, there will be another 64-bit suffix,
                // likely clangarm64. In that case, this and other assertions will need updating,
                // as there will be two separate paths to check under the same 64-bit program files
                // directory. (See the definition of ProgramFilesPaths::maybe_64bit for details.)
                assert_eq!(suffix_64bit, Path::new("Git/mingw64/bin"));
            }
        }
    }

    #[test]
    #[cfg(windows)]
    fn alternative_locations() {
        let locations = super::ALTERNATIVE_LOCATIONS.as_slice();

        // Obtain program files directory paths by other means and check that they seem correct.
        let pf = ProgramFilesPaths::obtain_envlessly().validate();

        // Check that `ALTERNATIVE_LOCATIONS` correspond to them, with the correct subdirectories.
        GitBinSuffixes::assert_from(&pf, locations).assert_architectures();

        // FIXME: Assert that the directory separators are `/` in the underlying `OsString`s.
    }

    #[test]
    #[cfg(not(windows))]
    fn alternative_locations() {
        assert!(super::ALTERNATIVE_LOCATIONS.is_empty());
    }
}
