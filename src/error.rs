use std::{backtrace::{Backtrace, BacktraceStatus}, fmt::Display};

#[derive(Debug, thiserror::Error)]
pub enum ErrorType {
    #[error(transparent)]
    NvimOxiError(nvim_oxi::Error),
    #[error("other error: {0}")]
    Other(anyhow::Error)
}

impl<T> From<T> for ErrorType where T: Into<nvim_oxi::Error> {
    fn from(value: T) -> Self {
        Self::NvimOxiError(value.into())
    }
}

#[derive(Debug)]
pub struct Error {
    error: ErrorType,
    backtrace: Backtrace,
}

impl std::error::Error for Error{
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        Some(&self.error)
    }
}

impl Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.error.fmt(f)?;
        if let BacktraceStatus::Captured = self.backtrace.status() {
            writeln!(f)?;
            self.backtrace.fmt(f)
        } else {
            Ok(())
        }
    }
}

impl Error {
    pub fn other(s: impl Display) -> Self {
        ErrorType::Other(anyhow::anyhow!("{s}")).into()
    }
}

impl<T> From<T> for Error where T: Into<ErrorType> {
    fn from(value: T) -> Self {
        Self {
            error: value.into(),
            backtrace: Backtrace::capture()
        }
    }
}
