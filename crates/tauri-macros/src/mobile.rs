// Copyright 2019-2024 Tauri Programme within The Commons Conservancy
// SPDX-License-Identifier: Apache-2.0
// SPDX-License-Identifier: MIT

use proc_macro::TokenStream;
use proc_macro2::TokenStream as TokenStream2;
use quote::{format_ident, quote};
use std::env::var;
use syn::{ItemFn, parse_macro_input, spanned::Spanned};

fn get_env_var(name: &str, error: &mut Option<TokenStream2>, function: &ItemFn) -> TokenStream2 {
  match var(name) {
    Ok(value) => {
      let ident = format_ident!("{value}");
      quote!(#ident)
    }
    Err(_) => {
      error.replace(
        syn::Error::new(
          function.span(),
          format!("`{name}` env var not set, do you have a build script with tauri-build?",),
        )
        .into_compile_error(),
      );
      quote!()
    }
  }
}

fn parse_extra_protocols(attributes: TokenStream) -> syn::Result<Vec<String>> {
  if attributes.is_empty() {
    return Ok(Vec::new());
  }
  let mut protocols = Vec::new();
  let parser = syn::meta::parser(|meta| {
    if meta.path.is_ident("protocols") {
      let value: syn::LitStr = meta.value()?.parse()?;
      for p in value.value().split(',') {
        let p = p.trim();
        if !p.is_empty() {
          protocols.push(p.to_string());
        }
      }
      Ok(())
    } else {
      Err(meta.error("unsupported mobile_entry_point attribute (expected `protocols = \"...\"`)"))
    }
  });
  syn::parse::Parser::parse(parser, attributes)?;
  Ok(protocols)
}

pub fn entry_point(attributes: TokenStream, item: TokenStream) -> TokenStream {
  let extra_protocols = match parse_extra_protocols(attributes) {
    Ok(p) => p,
    Err(e) => return e.into_compile_error().into(),
  };
  let mut all_protocols = vec![
    "tauri".to_string(),
    "ipc".to_string(),
    "asset".to_string(),
    "isolation".to_string(),
  ];
  for p in extra_protocols {
    if !all_protocols.iter().any(|existing| existing == &p) {
      all_protocols.push(p);
    }
  }
  let ohos_protocols = all_protocols.join(",");

  let function = parse_macro_input!(item as ItemFn);
  let function_name = function.sig.ident.clone();

  let mut error = None;
  let domain = get_env_var("TAURI_ANDROID_PACKAGE_NAME_PREFIX", &mut error, &function);
  let app_name = get_env_var("TAURI_ANDROID_PACKAGE_NAME_APP_NAME", &mut error, &function);

  let (wrapper, wrapper_name) = if function.sig.asyncness.is_some() {
    let wrapper_name = syn::Ident::new(&format!("{function_name}_wrapper"), function_name.span());
    (
      quote! {
        #function

        fn #wrapper_name() {
          ::tauri::async_runtime::block_on(#function_name());
        }
      },
      wrapper_name,
    )
  } else {
    (
      quote! {
        #function
      },
      function_name,
    )
  };

  if let Some(e) = error {
    quote!(#e).into()
  } else {
    quote!(
      fn stop_unwind<F: FnOnce() -> T, T>(f: F) -> T {
        match std::panic::catch_unwind(std::panic::AssertUnwindSafe(f)) {
          Ok(t) => t,
          Err(err) => {
            eprintln!("attempt to unwind out of `rust` with err: {:?}", err);
            std::process::abort()
          }
        }
      }

      #wrapper

      fn _start_app() {
        #[cfg(target_os = "ios")]
        ::tauri::log_stdout();
        #[cfg(target_os = "android")]
        {
          ::tauri::android_binding!(#domain, #app_name, _start_app, ::tauri::wry);
        }
        stop_unwind(#wrapper_name);
      }

      // be careful when renaming this, the `start_app` symbol is checked by the CLI
      #[cfg(target_os = "ios")]
      #[unsafe(no_mangle)]
      #[inline(never)]
      pub extern "C" fn start_app() {
        _start_app()
      }

      #[cfg(target_env = "ohos")]
      #[allow(unused_imports)]
      use ::tauri::ohos::{
        napi_derive_ohos, napi_ohos, openharmony_ability, openharmony_ability_derive,
      };

      #[cfg(target_env = "ohos")]
      #[napi_derive_ohos::napi]
      pub fn tauri_ohos_plugin_initialize(callback: napi_ohos::bindgen_prelude::Function<'_, String, ()>, files: String, cache: String, temp: String) -> napi_ohos::Result<()> {
        ::tauri::plugin::mobile::initialize_ohos_plugin_bridge(callback, files, cache, temp)
      }

      #[cfg(target_env = "ohos")]
      #[napi_derive_ohos::napi]
      pub fn tauri_ohos_plugin_response(id: i32, success: bool, payload: String) -> napi_ohos::Result<()> {
        ::tauri::plugin::mobile::ohos_plugin_response(id, success, payload)
      }

      #[cfg(target_env = "ohos")]
      #[napi_derive_ohos::napi]
      pub fn tauri_ohos_plugin_close() {
        ::tauri::plugin::mobile::close_ohos_plugin_bridge()
      }

      #[cfg(target_env = "ohos")]
      #[::tauri::ohos::openharmony_ability_derive::ability(webview, protocol = #ohos_protocols)]
      pub fn openharmony(app: ::tauri::ohos::openharmony_ability::OpenHarmonyApp) {
        ::tauri::ohos::APP.lock().unwrap().replace(app);
        _start_app()
      }
    )
    .into()
  }
}
