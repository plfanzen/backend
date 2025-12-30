// SPDX-FileCopyrightText: 2025 Aaron Dewes <aaron@nirvati.org>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

use std::{cell::RefCell, rc::Rc};

use boa_engine::{
    JsError, JsNativeError, JsObject, JsResult, JsString, Module,
    module::{ModuleLoader, Referrer},
};
use boa_runtime::RuntimeExtension;

struct DummyLoader;

impl ModuleLoader for DummyLoader {
    async fn load_imported_module(
        self: Rc<Self>,
        _referrer: Referrer,
        _specifier: JsString,
        _context: &RefCell<&mut boa_engine::Context>,
    ) -> JsResult<Module> {
        Err(JsError::from(JsNativeError::error().with_message(
            "Imports are not supported in n5i's JS runtime",
        )))
    }

    fn init_import_meta(
        self: Rc<Self>,
        _import_meta: &JsObject,
        _module: &Module,
        _context: &mut boa_engine::Context,
    ) {
        // Do nothing
    }
}

pub fn create_boa_context() -> boa_engine::Context {
    let mut ctx = boa_engine::Context::builder()
        .module_loader(Rc::new(DummyLoader))
        .build()
        .unwrap();

    (
        boa_runtime::extensions::ConsoleExtension::default(),
        boa_runtime::extensions::EncodingExtension,
        // boa_runtime::extensions::MicrotaskExtension,
        boa_runtime::extensions::TimeoutExtension,
        boa_runtime::extensions::StructuredCloneExtension,
        boa_runtime::extensions::UrlExtension,
        tera_with_js::crypto::CryptoExtension,
    )
        .register(None, &mut ctx)
        .unwrap();

    ctx
}
