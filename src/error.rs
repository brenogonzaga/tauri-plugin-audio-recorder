use serde::{ser::Serializer, Serialize};

pub type Result<T> = std::result::Result<T, Error>;

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error(transparent)]
    Io(#[from] std::io::Error),

    #[error("Recording error: {0}")]
    Recording(String),

    #[error("Permission denied: microphone access not granted")]
    PermissionDenied,

    #[error("No recording in progress")]
    NotRecording,

    #[error("Already recording")]
    AlreadyRecording,

    #[error("Audio device not found")]
    DeviceNotFound,

    #[error("Invalid channel: {0}")]
    InvalidChannel(String),

    #[error("Unsupported audio format")]
    UnsupportedFormat,

    #[error("Invalid path: {0}")]
    InvalidPath(String),

    #[cfg(mobile)]
    #[error(transparent)]
    PluginInvoke(#[from] tauri::plugin::mobile::PluginInvokeError),
}

impl Serialize for Error {
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_str(self.to_string().as_ref())
    }
}
