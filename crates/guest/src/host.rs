//! Calls an app makes to Cydonia. They reach Cydonia only from inside the
//! app's `.wasm`; called in a native build they panic.

#[cfg(target_arch = "wasm32")]
mod import {
    #[link(wasm_import_module = "cydonia")]
    unsafe extern "C" {
        pub fn set_input(key: *const u8, key_len: usize, value: *const u8, value_len: usize);
        pub fn panic(message: *const u8, len: usize);
    }
}

/// Put `value` in the input with this key, replacing its text. Its `on_input`
/// does not hear it.
pub fn set_input(key: &str, value: &str) {
    #[cfg(target_arch = "wasm32")]
    // SAFETY: both pointers are valid for their lengths for the whole call,
    // and Cydonia only reads them.
    unsafe {
        import::set_input(key.as_ptr(), key.len(), value.as_ptr(), value.len())
    }
    #[cfg(not(target_arch = "wasm32"))]
    {
        let _ = (key, value);
        panic!("set_input reaches Cydonia only from inside wasm")
    }
}

/// Send each panic's message to Cydonia before the panic traps.
pub(crate) fn hook_panics() {
    #[cfg(target_arch = "wasm32")]
    std::panic::set_hook(Box::new(|info| {
        let message = info.to_string();
        // SAFETY: the message outlives the call, and Cydonia only reads it.
        unsafe { import::panic(message.as_ptr(), message.len()) }
    }));
}
