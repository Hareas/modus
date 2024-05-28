use proc_macro::TokenStream;
use syn::{DeriveInput, Ident};

fn impl_from_trait(ast: DeriveInput) -> TokenStream {
    let ident = ast.ident;

    let fields_idents: Vec<Ident> = match ast.data {
        syn::Data::Struct(_) => panic!("Structs are not supported by From"),
        syn::Data::Enum(ref data) => data.variants.iter().map(|f| f.ident.clone()).collect(),
        syn::Data::Union(_) => panic!("Unions are not supported by From"),
    };

    let mut tokens = quote::quote!();
    for variant in fields_idents {
        tokens.extend(quote::quote! {
            impl From<#variant> for #ident {
                fn from (_e: #variant) -> Self {
                    #ident::#variant
                }
            }
        });
    }
    tokens.into()
}

#[proc_macro_derive(From)]
pub fn from_derive_macro(item: TokenStream) -> TokenStream {
    let ast: DeriveInput = syn::parse(item).unwrap();
    impl_from_trait(ast)
}