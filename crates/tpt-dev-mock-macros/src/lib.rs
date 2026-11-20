//! The `#[mock]` attribute. Use it through `tpt_dev_mock::mock`.

use proc_macro::TokenStream;
use proc_macro2::TokenStream as TokenStream2;
use quote::{format_ident, quote};
use syn::parse::{Parse, ParseStream};
use syn::visit_mut::{self, VisitMut};
use syn::{
    FnArg, Ident, ItemTrait, Path, ReturnType, Signature, Token, TraitItem, Type, TypePath,
    parse_macro_input, parse_quote,
};

/// Generates a mock for a trait.
///
/// For `trait T`, it generates `MockT`, with one `pub` field per method. Each
/// field is a `Recorder` for that method, and `MockT` implements `T` by
/// routing each call through its recorder. The mock also gets `verify`, which
/// checks every `times` expectation.
///
/// Methods must take `&self` or `&mut self`, and must not be generic or async.
/// Traits must not be generic, and must not have associated constants.
/// Argument and return types must be owned, meaning `'static`.
///
/// Each associated type must be bound in the attribute, for example
/// `#[mock(Item = u32)]` for `type Item;`. Occurrences of `Self::Item` in method
/// signatures are replaced by the bound type. Bindings must be simple paths
/// (`Self::Item`); the qualified form `<Self as T>::Item` is not rewritten.
///
/// # Examples
///
/// This example is not compiled here: this crate cannot depend on the runtime
/// it generates code for without a dependency cycle. The same behaviour is
/// exercised by `tpt-dev-mock`'s tests.
///
/// ```ignore
/// use tpt_dev_mock::mock;
///
/// #[mock]
/// pub trait Counter {
///     fn add(&mut self, by: u32) -> u32;
/// }
///
/// let mut mock = MockCounter::new();
/// mock.add.expect().returns_with(|(by,)| by * 2);
/// assert_eq!(mock.add(21), 42);
/// assert_eq!(mock.add.calls(), 1);
/// ```
#[proc_macro_attribute]
pub fn mock(args: TokenStream, item: TokenStream) -> TokenStream {
    let tr = parse_macro_input!(item as ItemTrait);
    let bindings = parse_macro_input!(args as Bindings);
    match generate(&tr, &bindings.0) {
        Ok(ts) => ts.into(),
        Err(err) => {
            let err = err.to_compile_error();
            quote!(#tr #err).into()
        }
    }
}

/// The `Name = Type` list given to `#[mock(...)]`.
struct Bindings(Vec<(Ident, Type)>);

impl Parse for Bindings {
    fn parse(input: ParseStream) -> syn::Result<Self> {
        let mut bindings = Vec::new();
        while !input.is_empty() {
            let name: Ident = input.parse()?;
            input.parse::<Token![=]>()?;
            let ty: Type = input.parse()?;
            bindings.push((name, ty));
            if input.is_empty() {
                break;
            }
            input.parse::<Token![,]>()?;
        }
        Ok(Self(bindings))
    }
}

/// Replaces `Self::Name` paths with their bound types.
struct SubstituteAssoc<'a> {
    bindings: &'a [(Ident, Type)],
}

impl SubstituteAssoc<'_> {
    /// The bound type for `Self::Name`, if `ty` is that path.
    fn bound(&self, ty: &Type) -> Option<Type> {
        let Type::Path(TypePath { qself: None, path }) = ty else {
            return None;
        };
        let Path {
            leading_colon: None,
            segments,
        } = path
        else {
            return None;
        };
        if segments.len() != 2 || segments[0].ident != "Self" || !segments[1].arguments.is_empty() {
            return None;
        }
        let name = &segments[1].ident;
        self.bindings
            .iter()
            .find(|(bound, _)| bound == name)
            .map(|(_, ty)| ty.clone())
    }
}

impl VisitMut for SubstituteAssoc<'_> {
    fn visit_type_mut(&mut self, ty: &mut Type) {
        if let Some(replacement) = self.bound(ty) {
            *ty = replacement;
            return;
        }
        visit_mut::visit_type_mut(self, ty);
    }
}

fn generate(tr: &ItemTrait, bindings: &[(Ident, Type)]) -> syn::Result<TokenStream2> {
    if !tr.generics.params.is_empty() {
        return Err(syn::Error::new_spanned(
            &tr.generics,
            "generic traits cannot be mocked",
        ));
    }
    check_bindings(tr, bindings)?;
    let trait_name = &tr.ident;
    let vis = &tr.vis;
    let mock_name = format_ident!("Mock{}", trait_name);
    let struct_doc = format!("A mock of `{trait_name}`, with one expectation recorder per method.");

    let mut fields = Vec::new();
    let mut inits = Vec::new();
    let mut impls = Vec::new();
    let mut verifies = Vec::new();
    let mut assoc_types = Vec::new();

    for item in &tr.items {
        let f = match item {
            TraitItem::Fn(f) => f,
            TraitItem::Type(t) => {
                if !t.generics.params.is_empty() {
                    return Err(syn::Error::new_spanned(
                        &t.generics,
                        "generic associated types cannot be mocked",
                    ));
                }
                let (_, ty) = bindings
                    .iter()
                    .find(|(name, _)| *name == t.ident)
                    .ok_or_else(|| missing_binding(t))?;
                let name = &t.ident;
                assoc_types.push(quote! { type #name = #ty; });
                continue;
            }
            _ => {
                return Err(syn::Error::new_spanned(
                    item,
                    "only methods and associated types can be mocked (no associated constants)",
                ));
            }
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

        let mut sig: Signature = f.sig.clone();
        SubstituteAssoc { bindings }.visit_signature_mut(&mut sig);

        let name = &sig.ident;
        let mut receiver: Option<TokenStream2> = None;
        let mut arg_types: Vec<Type> = Vec::new();
        let mut arg_names: Vec<Ident> = Vec::new();
        for input in &sig.inputs {
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
                &sig,
                "associated functions without self cannot be mocked",
            ));
        };

        let ret: Type = match &sig.output {
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
            #(#assoc_types)*
            #(#impls)*
        }
    })
}

/// Rejects bindings that name no associated type in the trait.
fn check_bindings(tr: &ItemTrait, bindings: &[(Ident, Type)]) -> syn::Result<()> {
    for (name, _) in bindings {
        let declared = tr
            .items
            .iter()
            .any(|item| matches!(item, TraitItem::Type(t) if t.ident == *name));
        if !declared {
            return Err(syn::Error::new_spanned(
                name,
                format!("`{name}` is not an associated type of this trait"),
            ));
        }
    }
    Ok(())
}

fn missing_binding(t: &syn::TraitItemType) -> syn::Error {
    syn::Error::new_spanned(
        &t.ident,
        format!(
            "associated type `{}` must be bound, for example #[mock({} = u32)]",
            t.ident, t.ident
        ),
    )
}
