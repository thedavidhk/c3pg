use anyhow::Result;

pub trait FileWrapper {
    fn from_file<P: AsRef<std::path::Path>>(path: P) -> Result<Self, std::io::Error>
    where
        Self: Sized;
    fn to_file<P: AsRef<std::path::Path>>(&self, path: P) -> Result<(), std::io::Error>;
}
