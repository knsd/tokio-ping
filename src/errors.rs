#[derive(Debug, Fail)]
pub enum Error {
    #[fail(display = "invalid procotol")]
    InvalidProtocol,
    #[fail(display = "internal error")]
    InternalError,
    #[fail(display = "io error: {}", error)]
    IoError {
        error: ::std::io::Error
    }
}

impl From<::std::io::Error> for Error {
    fn from(error: ::std::io::Error) -> Self {
        Error::IoError {
            error
        }
    }
}