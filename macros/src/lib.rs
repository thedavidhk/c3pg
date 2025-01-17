extern crate proc_macro;
use proc_macro::TokenStream;
use quote::quote;
use syn::{parse_macro_input, DeriveInput};

#[proc_macro_derive(FileWrapper)]
pub fn file_wrapper_derive(input: TokenStream) -> TokenStream {
    let ast = parse_macro_input!(input as DeriveInput);
    let name = &ast.ident;

    let expanded = quote! {
        impl FileWrapper for #name {
            fn from_file<P: AsRef<std::path::Path>>(path: P) -> Result<Self, std::io::Error> where Self: Sized {
                let path_ref = path.as_ref();  // Borrow the path as a reference
                let content = std::fs::read_to_string(path_ref).map_err(|e| {
                    std::io::Error::new(
                        std::io::ErrorKind::Other,
                        format!("Could not read from {}: {}", path_ref.display(), e),
                    )
                })?;
                Self::from_str(content.as_str()).map_err(|e| {
                    std::io::Error::new(
                        std::io::ErrorKind::Other,
                        format!("Could not parse from {}: {}", path_ref.display(), e),
                    )
                })
            }

            fn to_file<P: AsRef<std::path::Path>>(&self, path: P) -> Result<(), std::io::Error> {
                let path_ref = path.as_ref();  // Borrow the path as a reference
                std::fs::write(path_ref, self.to_string()).map_err(|e| {
                    std::io::Error::new(
                        std::io::ErrorKind::Other,
                        format!("Could not write to {}: {}", path_ref.display(), e),
                    )
                })
            }
        }
    };

    TokenStream::from(expanded)
}
