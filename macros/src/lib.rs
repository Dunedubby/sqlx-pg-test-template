use proc_macro::TokenStream;
use quote::quote;
use syn::{parse::Parser, MetaNameValue};

type AttributeArgs = syn::punctuated::Punctuated<syn::Meta, syn::Token![,]>;
type Error = Box<dyn std::error::Error>;
type Result<T> = std::result::Result<T, Error>;

#[derive(Default)]
struct Args {
    template_name: Option<String>,
    max_connections: Option<u32>,
}

/// Enables sqlx_db_test capabilities for a test
#[proc_macro_attribute]
pub fn test(args: TokenStream, input: TokenStream) -> TokenStream {
    let input = syn::parse_macro_input!(input as syn::ItemFn);
    let args = args;

    match expand(args, input) {
        Ok(ts) => ts,
        Err(e) => {
            if let Some(parse_err) = e.downcast_ref::<syn::Error>() {
                parse_err.to_compile_error().into()
            } else {
                let msg = e.to_string();
                quote!(::std::compile_error!(#msg)).into()
            }
        }
    }
}

/// Runs actual expansion of the `#[test]` attribute
fn expand(args: TokenStream, input: syn::ItemFn) -> Result<TokenStream> {
    let parser = AttributeArgs::parse_terminated;
    let args = parser.parse2(args.into())?;
    let args = parse_args(args)?;

    expand_with_args(input, args)
}

fn parse_args(attr_args: AttributeArgs) -> syn::Result<Args> {
    let mut args = Args::default();

    for arg in attr_args {
        let path = arg.path().clone();

        match arg {
            syn::Meta::NameValue(MetaNameValue { value, .. }) if path.is_ident("template") => {
                args.template_name = Some(parse_lit_str(&value)?);
            }

            syn::Meta::NameValue(MetaNameValue { value, .. })
                if path.is_ident("max_connections") =>
            {
                let digits = parse_lit_int(&value)?;
                let mc: u32 = digits
                    .parse()
                    .map_err(|_| syn::Error::new_spanned(value, "expected u32 number"))?;

                args.max_connections = Some(mc);
            }

            arg => {
                return Err(syn::Error::new_spanned(
                    arg,
                    r#"expected `template = "database_name"` and/or `max_connections = 5`"#,
                ))
            }
        }
    }

    Ok(args)
}

fn expand_with_args(input: syn::ItemFn, args: Args) -> Result<TokenStream> {
    let ret = &input.sig.output;
    let name = &input.sig.ident;
    let inputs = &input.sig.inputs;
    let body = &input.block;
    let attrs = &input.attrs;

    let template_name = match args.template_name {
        None => quote! { None },
        Some(name) => quote! { Some(#name.to_string()) },
    };

    let max_connections = match args.max_connections {
        None => quote! { None },
        Some(mc) => quote! { Some(#mc) },
    };

    let name_str = name.to_string();

    Ok(quote! {
        #(#attrs)*
        #[::core::prelude::v1::test]
        fn #name() #ret {
            async fn #name(#inputs) #ret {
                #body
            };

            let test_args = ::sqlx_pg_test_template::TestArgs {
                template_name: #template_name,
                max_connections: #max_connections,
                module_path: format!("{}::{}", module_path!().to_string(), #name_str),
            };

            sqlx_pg_test_template::run_test(#name, test_args)

            // TODO: check timeout of pool going out of scope. main problem is that sqlx does
            // not export core trait.
            //
            // let close_timed_out = sqlx::rt::timeout(Duration::from_secs(10), pool.close())
            //     .await
            //     .is_err();

            // if close_timed_out {
            //     eprintln!("test {test_path} held onto Pool after exiting");
            // }

        }
    }
    .into())
}

fn parse_lit_str(expr: &syn::Expr) -> syn::Result<String> {
    match expr {
        syn::Expr::Lit(syn::ExprLit {
            lit: syn::Lit::Str(lit),
            ..
        }) => Ok(lit.value()),
        _ => Err(syn::Error::new_spanned(expr, "expected string")),
    }
}

fn parse_lit_int(expr: &syn::Expr) -> syn::Result<String> {
    match expr {
        syn::Expr::Lit(syn::ExprLit {
            lit: syn::Lit::Int(lit),
            ..
        }) => Ok(lit.base10_digits().to_owned()),
        _ => Err(syn::Error::new_spanned(expr, "expected integer")),
    }
}
