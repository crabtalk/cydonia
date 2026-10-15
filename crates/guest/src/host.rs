//! Calls an app makes to Cydonia. They reach Cydonia only from inside the
//! app's `.wasm`; called in a native build they panic.
//!
//! A call that answers later takes the message its answer becomes, and the
//! answer reaches [`crate::App::update`] as that message.

use crate::wire::{self, Answer, CallId};
use std::{any::Any, cell::RefCell, collections::HashMap};

#[cfg(target_arch = "wasm32")]
mod import {
    #[link(wasm_import_module = "cydonia")]
    unsafe extern "C" {
        pub fn set_input(key: *const u8, key_len: usize, value: *const u8, value_len: usize);
        pub fn panic(message: *const u8, len: usize);
        pub fn call(id: u32, request: *const u8, len: usize);
    }
}

/// Calls made and not yet answered, each holding the
/// `Box<dyn FnOnce(Answer) -> M>` its answer goes through.
#[derive(Default)]
struct Pending {
    next: CallId,
    waiting: HashMap<CallId, Box<dyn Any>>,
}

thread_local! {
    static PENDING: RefCell<Pending> = RefCell::default();
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

/// Make `call`; its answer becomes the message `answer` makes of it.
pub(crate) fn request<M: 'static>(call: &wire::Call, answer: impl FnOnce(Answer) -> M + 'static) {
    let answer: Box<dyn FnOnce(Answer) -> M> = Box::new(answer);
    let id = PENDING.with_borrow_mut(|pending| {
        let id = pending.next;
        pending.next = pending.next.wrapping_add(1);
        pending.waiting.insert(id, Box::new(answer));
        id
    });
    let request = serde_json::to_vec(call).expect("calls serialize");
    #[cfg(target_arch = "wasm32")]
    // SAFETY: the request outlives the call, and Cydonia only reads it.
    unsafe {
        import::call(id, request.as_ptr(), request.len())
    }
    #[cfg(not(target_arch = "wasm32"))]
    {
        let _ = (id, request);
        panic!("calls reach Cydonia only from inside wasm")
    }
}

/// The message the answer to call `id` becomes, once.
pub(crate) fn answered<M: 'static>(id: CallId, answer: Answer) -> Option<M> {
    let waiting = PENDING.with_borrow_mut(|pending| pending.waiting.remove(&id))?;
    let message = waiting.downcast::<Box<dyn FnOnce(Answer) -> M>>().ok()?;
    Some(message(answer))
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
