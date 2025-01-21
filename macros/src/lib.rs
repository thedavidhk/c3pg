extern crate proc_macro;
use proc_macro::TokenStream;
use quote::quote;
use syn::{parse_macro_input, DeriveInput};

#[proc_macro_derive(FromFile)]
pub fn file_wrapper_derive(input: TokenStream) -> TokenStream {
    let ast = parse_macro_input!(input as DeriveInput);
    let name = &ast.ident;

    let expanded = quote! {
        impl FromFile for #name {

            fn from_file<P: AsRef<std::path::Path>>(path: P) -> ::anyhow::Result<Self> where Self: Sized {
                let path_ref = path.as_ref();  // Borrow the path as a reference
                let Ok(content) = std::fs::read_to_string(path_ref) else {
                    ::anyhow::bail!("Could not read from {}", path_ref.display());
                };
                Self::from_str(content.as_str())
            }
        }
    };

    TokenStream::from(expanded)
}

#[proc_macro_derive(ToFile)]
pub fn to_file_derive(input: TokenStream) -> TokenStream {
    let ast = parse_macro_input!(input as DeriveInput);
    let name = &ast.ident;

    let expanded = quote! {
        impl ToFile for #name {
            fn to_file<P: AsRef<std::path::Path>>(&self, path: P) -> ::anyhow::Result<()> {
                let path_ref = path.as_ref();  // Borrow the path as a reference
                let Ok(result) = std::fs::write(path_ref, self.to_string()) else {
                    ::anyhow::bail!("Could not write to {}", path_ref.display());
                };
                Ok(())
            }
        }
    };

    TokenStream::from(expanded)
}
