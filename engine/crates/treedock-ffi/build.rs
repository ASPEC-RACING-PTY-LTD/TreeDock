// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
//! Generates the C# P/Invoke bindings for the desktop app from `src/lib.rs`.

fn main() {
    println!("cargo:rerun-if-changed=src/lib.rs");
    println!("cargo:rerun-if-changed=../treedock-core/src/treemap.rs");
    csbindgen::Builder::default()
        .input_extern_file("src/lib.rs")
        .input_extern_file("../treedock-core/src/treemap.rs")
        .csharp_dll_name("treedock_engine")
        .csharp_namespace("TreeDock.Engine")
        .csharp_class_name("Native")
        .csharp_class_accessibility("internal")
        .generate_csharp_file("../../../app/TreeDock.App/Engine/Native.g.cs")
        .expect("generate C# bindings");
}
