use std::path::PathBuf;

use jb_core::Verdict;

/// Errors from the save manager. Messages are written for players.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("{what} {path}: {source}")]
    Io {
        what: &'static str,
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },
    #[error("no game with id {0}")]
    UnknownGame(String),
    #[error("no backup with id {0}")]
    UnknownSnapshot(String),
    #[error("{0} is not a folder")]
    NotAFolder(PathBuf),
    #[error("{path} can't be used as a save folder: {why}")]
    BadFolder { path: PathBuf, why: String },
    #[error("this game has no save folders yet; add one first")]
    NoFolders,
    #[error(
        "the save folders are too big to back up ({files} files, {bytes} bytes; the limit is \
         {max_files} files and {max_bytes} bytes). Pick a smaller folder"
    )]
    TooLarge {
        files: usize,
        bytes: u64,
        max_files: usize,
        max_bytes: u64,
    },
    #[error("close the game before restoring a backup ({0} is running)")]
    GameRunning(String),
    #[error("restoring is blocked for this game: {0}")]
    Blocked(Box<Verdict>),
    #[error("backup {id} is damaged: {why}")]
    Damaged { id: String, why: String },
    #[error("backup {id} is for folder {folder}, which is no longer a save folder of this game")]
    FolderNotListed { id: String, folder: String },
    #[error("bad data in {path}: {why}")]
    BadData { path: PathBuf, why: String },
    #[error("can't find the Windows folder {0}")]
    NoKnownFolder(&'static str),
    #[error("could not list running programs: {0}")]
    Process(String),
    #[error("{0}")]
    Edit(String),
}

impl Error {
    pub(crate) fn io(what: &'static str, path: impl Into<PathBuf>, source: std::io::Error) -> Self {
        Self::Io {
            what,
            path: path.into(),
            source,
        }
    }

    /// A short stable name for the UI.
    pub fn kind(&self) -> &'static str {
        match self {
            Self::Io { .. } => "io",
            Self::UnknownGame(_) => "unknownGame",
            Self::UnknownSnapshot(_) => "unknownSnapshot",
            Self::NotAFolder(_) | Self::BadFolder { .. } => "badFolder",
            Self::NoFolders => "noFolders",
            Self::TooLarge { .. } => "tooLarge",
            Self::GameRunning(_) => "gameRunning",
            Self::Blocked(_) => "blocked",
            Self::Damaged { .. } => "damaged",
            Self::FolderNotListed { .. } => "folderNotListed",
            Self::BadData { .. } => "badData",
            Self::NoKnownFolder(_) => "noKnownFolder",
            Self::Process(_) => "process",
            Self::Edit(_) => "badEdit",
        }
    }
}

pub type Result<T> = std::result::Result<T, Error>;
