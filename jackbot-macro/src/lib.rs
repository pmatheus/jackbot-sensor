//! # Jackbot Macro
//! 
//! Procedural macros for the Jackbot trading system.
//! Provides code generation utilities for reducing boilerplate.

use proc_macro::TokenStream;
use quote::quote;
use syn::{parse_macro_input, DeriveInput};

/// Derive macro for generating display implementations
#[proc_macro_derive(Display)]
pub fn derive_display(input: TokenStream) -> TokenStream {
    let input = parse_macro_input!(input as DeriveInput);
    let name = &input.ident;
    
    let expanded = quote! {
        impl std::fmt::Display for #name {
            fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                write!(f, "{:?}", self)
            }
        }
    };
    
    TokenStream::from(expanded)
}

/// Derive macro for generating default debug implementations
#[proc_macro_derive(DefaultDebug)]
pub fn derive_default_debug(input: TokenStream) -> TokenStream {
    let input = parse_macro_input!(input as DeriveInput);
    let name = &input.ident;
    
    let expanded = quote! {
        impl std::fmt::Debug for #name {
            fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                f.debug_struct(stringify!(#name))
                    .finish()
            }
        }
    };
    
    TokenStream::from(expanded)
}

/// Derive macro for exchange serialization
#[proc_macro_derive(SerExchange)]
pub fn derive_ser_exchange(input: TokenStream) -> TokenStream {
    let input = parse_macro_input!(input as DeriveInput);
    let name = &input.ident;
    
    let expanded = quote! {
        impl #name {
            /// Serialize this exchange data
            pub fn serialize_exchange(&self) -> Result<Vec<u8>, serde_json::Error> {
                serde_json::to_vec(self)
            }
        }
    };
    
    TokenStream::from(expanded)
}

/// Derive macro for exchange deserialization  
#[proc_macro_derive(DeExchange)]
pub fn derive_de_exchange(input: TokenStream) -> TokenStream {
    let input = parse_macro_input!(input as DeriveInput);
    let name = &input.ident;
    
    let expanded = quote! {
        impl #name {
            /// Deserialize exchange data from bytes
            pub fn deserialize_exchange(data: &[u8]) -> Result<Self, serde_json::Error> {
                serde_json::from_slice(data)
            }
            
            /// Deserialize exchange data from string
            pub fn deserialize_exchange_str(data: &str) -> Result<Self, serde_json::Error> {
                serde_json::from_str(data)
            }
        }
    };
    
    TokenStream::from(expanded)
}

/// Derive macro for subscription kind serialization
#[proc_macro_derive(SerSubKind)]
pub fn derive_ser_sub_kind(input: TokenStream) -> TokenStream {
    let input = parse_macro_input!(input as DeriveInput);
    let name = &input.ident;
    
    let expanded = quote! {
        impl #name {
            /// Serialize this subscription kind
            pub fn serialize_sub_kind(&self) -> Result<Vec<u8>, serde_json::Error> {
                serde_json::to_vec(self)
            }
        }
    };
    
    TokenStream::from(expanded)
}

/// Derive macro for subscription kind deserialization
#[proc_macro_derive(DeSubKind)]
pub fn derive_de_sub_kind(input: TokenStream) -> TokenStream {
    let input = parse_macro_input!(input as DeriveInput);
    let name = &input.ident;
    
    let expanded = quote! {
        impl #name {
            /// Deserialize subscription kind from bytes
            pub fn deserialize_sub_kind(data: &[u8]) -> Result<Self, serde_json::Error> {
                serde_json::from_slice(data)
            }
            
            /// Deserialize subscription kind from string
            pub fn deserialize_sub_kind_str(data: &str) -> Result<Self, serde_json::Error> {
                serde_json::from_str(data)
            }
        }
    };
    
    TokenStream::from(expanded)
}