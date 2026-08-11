//! IO error types for Leo3.
//!
//! This module provides error handling for IO operations in Lean4.

use crate::conversion::FromLean;
use crate::err::LeanError;
use crate::ffi;
use crate::instance::LeanBound;
use crate::marker::Lean;
use crate::types::LeanString;
use std::fmt;

/// Result type for IO operations.
pub type IOResult<T> = Result<T, IOError>;

/// Error type for IO operations.
///
/// This corresponds to Lean4's `IO.Error` type.
#[derive(Debug)]
pub enum IOError {
    /// File system error (file not found, permission denied, etc.)
    Filesystem(String),
    /// User-defined error message
    UserError(String),
    /// Interrupted system call
    Interrupted,
    /// Operation not supported
    Unsupported(String),
    /// Other IO errors
    Other(String),
}

impl IOError {
    /// Create a filesystem error.
    pub fn filesystem(msg: impl Into<String>) -> Self {
        IOError::Filesystem(msg.into())
    }

    /// Create a user error.
    pub fn user_error(msg: impl Into<String>) -> Self {
        IOError::UserError(msg.into())
    }

    /// Create an unsupported operation error.
    pub fn unsupported(msg: impl Into<String>) -> Self {
        IOError::Unsupported(msg.into())
    }

    /// Create a generic IO error.
    pub fn other(msg: impl Into<String>) -> Self {
        IOError::Other(msg.into())
    }

    /// Convert Lean IO.Error object to Rust IOError
    ///
    /// Maps the 4.25+ `IO.Error` constructor table:
    ///
    /// | tag | constructor | Rust mapping |
    /// |-----|-------------|--------------|
    /// | 0 | alreadyExists | Filesystem |
    /// | 1 | otherError | Other |
    /// | 2 | resourceBusy | Other |
    /// | 3 | resourceVanished | Other |
    /// | 4 | unsupportedOperation | Unsupported |
    /// | 5 | hardwareFault | Other |
    /// | 6 | unsatisfiedConstraints | Other |
    /// | 7 | illegalOperation | Other |
    /// | 8 | protocolError | Other |
    /// | 9 | timeExpired | Other |
    /// | 10 | interrupted | Interrupted |
    /// | 11 | noFileOrDirectory | Filesystem |
    /// | 12 | invalidArgument | Other |
    /// | 13 | permissionDenied | Other |
    /// | 14 | resourceExhausted | Other |
    /// | 15 | inappropriateType | Other |
    /// | 16 | noSuchThing | Other |
    /// | 17 | unexpectedEof | Other |
    /// | 18 | userError | UserError |
    ///
    /// The human-readable message is the last object field (`details`, or
    /// `msg` for `userError`); the `osCode : UInt32` scalar field is skipped.
    ///
    /// # Safety
    ///
    /// - `err_obj` must be a valid Lean IO.Error object
    pub(crate) unsafe fn from_lean_io_error<'l>(
        lean: Lean<'l>,
        err_obj: *mut ffi::lean_object,
    ) -> Self {
        let tag = ffi::object::lean_obj_tag(err_obj);
        let num_objs = ffi::inline::lean_ctor_num_objs(err_obj);

        // Extract the message from the last object field, when present.
        let message = if num_objs > 0 {
            let field = ffi::object::lean_ctor_get(err_obj, (num_objs - 1) as u32);
            let bound: LeanBound<LeanString> =
                LeanBound::from_borrowed_ptr(lean, field as *mut ffi::lean_object);
            String::from_lean(&bound).unwrap_or_else(|_| "Unknown IO error".to_string())
        } else {
            String::new()
        };

        match tag {
            0 => IOError::Filesystem(message),  // alreadyExists
            4 => IOError::Unsupported(message), // unsupportedOperation
            10 => IOError::Interrupted,         // interrupted
            11 => IOError::Filesystem(message), // noFileOrDirectory
            17 => IOError::Other("unexpected end of file".to_string()), // unexpectedEof
            18 => IOError::UserError(message),  // userError
            _ => IOError::Other(message),
        }
    }
}

impl fmt::Display for IOError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            IOError::Filesystem(msg) => write!(f, "Filesystem error: {}", msg),
            IOError::UserError(msg) => write!(f, "User error: {}", msg),
            IOError::Interrupted => write!(f, "Operation interrupted"),
            IOError::Unsupported(msg) => write!(f, "Unsupported operation: {}", msg),
            IOError::Other(msg) => write!(f, "IO error: {}", msg),
        }
    }
}

impl std::error::Error for IOError {}

impl From<IOError> for LeanError {
    fn from(err: IOError) -> Self {
        LeanError::Other(err.to_string())
    }
}

impl From<LeanError> for IOError {
    fn from(err: LeanError) -> Self {
        IOError::Other(err.to_string())
    }
}
