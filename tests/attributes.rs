/// Attribute integration tests.

#[path = "common/mod.rs"]
mod common;
pub use common::*;

#[path = "attributes/flat.rs"]
mod flat;
#[path = "attributes/hold.rs"]
mod hold;
#[path = "attributes/listable.rs"]
mod listable;
#[path = "attributes/one_identity.rs"]
mod one_identity;
#[path = "attributes/orderless.rs"]
mod orderless;
#[path = "attributes/protected_locked.rs"]
mod protected_locked;
