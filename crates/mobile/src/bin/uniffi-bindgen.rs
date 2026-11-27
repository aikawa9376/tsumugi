//! Writes the Kotlin bindings of this crate's library, as the app's build
//! asks.

fn main() {
    uniffi::uniffi_bindgen_main()
}
