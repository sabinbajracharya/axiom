//! Module system errors.

use std::path::PathBuf;

/// Errors from module graph construction and resolution.
#[derive(Debug, thiserror::Error)]
pub enum ModuleError {
    #[error("missing `main.ax` in source directory `{}`", dir.display())]
    MissingMain { dir: PathBuf },

    #[error("I/O error at `{}`: {source}", path.display())]
    Io {
        path: PathBuf,
        source: std::io::Error,
    },

    #[error("non-UTF-8 filename at `{}`", path.display())]
    NonUtf8Name { path: PathBuf },

    #[error(
        "conflicting module definitions: `{}` and `{}` both define the same module",
        file.display(),
        dir_mod.display()
    )]
    DualModuleDef { file: PathBuf, dir_mod: PathBuf },

    #[error(
        "module name collision: `{name}` defined in both `{}` and `{}`",
        first.display(),
        second.display()
    )]
    NameCollision {
        name: String,
        first: PathBuf,
        second: PathBuf,
    },
}
