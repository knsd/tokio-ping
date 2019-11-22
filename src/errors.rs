use std::fmt;

use failure::{Fail, Context, Backtrace};

#[derive(Debug)]
pub struct Error {
    inner: Context<ErrorKind>
}

impl Fail for Error {
    fn cause(&self) -> Option<&dyn Fail> {
        self.inner.cause()
    }

    fn backtrace(&self) -> Option<&Backtrace> {
        self.inner.backtrace()
    }
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        fmt::Display::fmt(&self.inner, f)
    }
}

impl From<ErrorKind> for Error {
    fn from(kind: ErrorKind) -> Error {
        Error { inner: Context::new(kind) }
    }
}

impl From<Context<ErrorKind>> for Error {
    fn from(inner: Context<ErrorKind>) -> Error {
        Error { inner: inner }
    }
}

impl From<::std::io::Error> for Error {
    fn from(error: ::std::io::Error) -> Self {
        Error {
            inner: Context::new(ErrorKind::IoError {
                error
            })
        }
    }
}

#[derive(Debug, Fail)]
pub enum ErrorKind {
    #[fail(display = "invalid procotol")]
    InvalidProtocol,
    #[fail(display = "internal error")]
    InternalError,
    #[fail(display = "io error: {}", error)]
    IoError {
        error: ::std::io::Error
    }
}
