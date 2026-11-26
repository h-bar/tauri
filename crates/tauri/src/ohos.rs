use std::sync::Mutex;

pub use napi_derive_ohos;
pub use napi_ohos;
pub use openharmony_ability;
pub use openharmony_ability_derive;

pub static APP: Mutex<Option<openharmony_ability::OpenHarmonyApp>> = Mutex::new(None);
