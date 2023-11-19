use thiserror::Error;

#[derive(Error, Debug)]
pub enum Error {
    #[error("Failure with bincode")]
    Bincode(#[from] bincode::Error),
    #[error("I/O error")]
    IO(#[from] std::io::Error),
    #[error("Only had {found} features, but expected {expected}")]
    FeatureCountMismatch { found: u64, expected: u64 },
    #[error("HTTP error")]
    HTTP(#[from] streaming_http_range_client::Error),
}
pub type Result<T> = std::result::Result<T, Error>;
