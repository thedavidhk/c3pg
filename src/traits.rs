use anyhow::Result;
pub use macros::{FromFile, ToFile};

pub trait FromFile {
    fn from_file<P: AsRef<std::path::Path>>(path: P) -> Result<Self>
    where
        Self: Sized;
}

pub trait ToFile {
    fn to_file<P: AsRef<std::path::Path>>(&self, path: P) -> Result<()>;
}
