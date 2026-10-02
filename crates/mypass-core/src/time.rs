//! Horloge wasm-safe. SystemTime::now() compile en wasm32-unknown-unknown
//! mais panique à l'exécution : sur cette cible on passe par js_sys::Date.
//! Toute heure courante du crate DOIT venir d'ici (la fusion repose sur
//! LastModificationTime — une horloge morte casse la sync silencieusement).

#[cfg(not(target_arch = "wasm32"))]
pub fn unix_now() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::SystemTime::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

#[cfg(target_arch = "wasm32")]
pub fn unix_now() -> u64 {
    (js_sys::Date::now() / 1000.0) as u64
}
