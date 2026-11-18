//! The `#[mock]` attribute. Use it through `tpt_dev_mock::mock`.

use proc_macro::TokenStream;
use proc_macro2::TokenStream as TokenStream2;
use quote::{format_ident, quote};
use syn::{FnArg, Ident, ItemTrait, ReturnType, TraitItem, Type, parse_macro_input, parse_quote};

/// Generates a mock for a trait.
///
/// For `trait T`, it generates `MockT`, with one `pub` field per method. Each
/// field is a `Recorder` for that method, and `MockT` implements `T` by
/// routing each call through its recorder. The mock also gets `verify`, which
/// checks every `times` expectation.
///
/// Methods must take `&self` or `&mut self`, and must not be generic or async.
/// Traits must not be generic, and must not have associated types or constants.
/// Argument and return types must be owned, meaning `'static`.
#[proc_macro_attribute]
pub fn mock(args: TokenStream, item: TokenStream) -> TokenStream {
    let tr = parse_macro_input!(item as ItemTrait);
    if !args.is_empty() {
        let err = syn::Error::new(proc_macro2::Span::call_site(), "#[mock] takes no arguments")
            .to_compile_error();
        return quote!(#tr #err).into();
    }
    match generate(&tr) {
        Ok(ts) => ts.into(),
        Err(err) => {
            let err = err.to_compile_error();
            quote!(#tr #err).into()
        }
    }
}

fn generate(tr: &ItemTrait) -> syn::Result<TokenStream2> {
    if !tr.generics.params.is_empty() {
        return Err(syn::Error::new_spanned(
            &tr.generics,
            "generic traits cannot be mocked",
        ));
    }
    let trait_name = &tr.ident;
    let vis = &tr.vis;
    let mock_name = format_ident!("Mock{}", trait_name);
    let struct_doc = format!("A mock of `{trait_name}`, with one expectation recorder per method.");

    let mut fields = Vec::new();
    let mut inits = Vec::new();
    let mut impls = Vec::new();
    let mut verifies = Vec::new();

    for item in &tr.items {
        let TraitItem::Fn(f) = item else {
            return Err(syn::Error::new_spanned(
                item,
                "only methods can be mocked (no associated types or constants)",
            ));
        };
        if f.sig.asyncness.is_some() {
            return Err(syn::Error::new_spanned(
                &f.sig,
                "async methods cannot be mocked",
            ));
        }
        if !f.sig.generics.params.is_empty() {
            return Err(syn::Error::new_spanned(
                &f.sig.generics,
                "generic methods cannot be mocked",
            ));
        }

        let name = &f.sig.ident;
        let mut receiver: Option<TokenStream2> = None;
        let mut arg_types: Vec<Type> = Vec::new();
        let mut arg_names: Vec<Ident> = Vec::new();
        for input in &f.sig.inputs {
            match input {
                FnArg::Receiver(r) => {
                    if r.reference.is_none() {
                        return Err(syn::Error::new_spanned(
                            r,
                            "methods must take &self or &mut self to be mocked",
                        ));
                    }
                    receiver = Some(if r.mutability.is_some() {
                        quote!(&mut self)
                    } else {
                        quote!(&self)
                    });
                }
                FnArg::Typed(pt) => {
                    arg_types.push((*pt.ty).clone());
                    arg_names.push(format_ident!("arg{}", arg_names.len()));
                }
            }
        }
        let Some(receiver) = receiver else {
            return Err(syn::Error::new_spanned(
                &f.sig,
                "associated functions without self cannot be mocked",
            ));
        };

        let ret: Type = match &f.sig.output {
            ReturnType::Default => parse_quote!(()),
            ReturnType::Type(_, t) => (**t).clone(),
        };
        let label = name.to_string();
        let args_ty = quote!((#(#arg_types,)*));
        let args_val = quote!((#(#arg_names,)*));

        let field_doc = format!("Expectations and call count for `{label}`.");
        fields.push(quote! {
            #[doc = #field_doc]
            pub #name: ::tpt_dev_mock::Recorder<#args_ty, #ret>
        });
        inits.push(quote! { #name: ::tpt_dev_mock::Recorder::new(#label) });
        impls.push(quote! {
            fn #name(#receiver, #(#arg_names: #arg_types),*) -> #ret {
                self.#name.call(#args_val)
            }
        });
        verifies.push(quote! { self.#name.verify(); });
    }

    Ok(quote! {
        #tr

        #[doc = #struct_doc]
        #[allow(dead_code)]
        #vis struct #mock_name {
            #(#fields,)*
        }

        impl #mock_name {
            /// A mock with no expectations.
            pub fn new() -> Self {
                Self { #(#inits,)* }
            }

            /// Panics if an expectation with a call count was not met.
            pub fn verify(&self) {
                #(#verifies)*
            }
        }

        impl ::core::default::Default for #mock_name {
            fn default() -> Self {
                Self::new()
            }
        }

        impl #trait_name for #mock_name {
            #(#impls)*
        }
    })
}
