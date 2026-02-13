use anyhow::Result;
pub use macros::{FromFile, ToFile};

pub trait FromFile {
    /// Deserialize `Self` from the file at `path`.
    ///
    /// # Errors
    ///
    /// Returns an error if the file cannot be read or its contents cannot be
    /// parsed into `Self`.
    fn from_file<P: AsRef<std::path::Path>>(path: P) -> Result<Self>
    where
        Self: Sized;
}

pub trait ToFile {
    /// Serialize `self` and write it to the file at `path`.
    ///
    /// # Errors
    ///
    /// Returns an error if `self` cannot be serialized or the file cannot be
    /// written.
    fn to_file<P: AsRef<std::path::Path>>(&self, path: P) -> Result<()>;
}
