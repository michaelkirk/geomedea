use thiserror::Error;

#[derive(Error, Debug)]
pub enum Error {
    #[error(transparent)]
    BincodeDecode(#[from] bincode::error::DecodeError),
    #[error(transparent)]
    BincodeEncode(#[from] bincode::error::EncodeError),
    #[error(transparent)]
    IO(#[from] std::io::Error),
    #[error("Only had {found} features, but expected {expected}")]
    FeatureCountMismatch { found: u64, expected: u64 },
    #[error(transparent)]
    HTTP(#[from] streaming_http_range_client::Error),
}
pub type Result<T> = std::result::Result<T, Error>;
