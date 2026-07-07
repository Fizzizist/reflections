#[cfg(not(test))]
pub fn app_version() -> &'static str {
    env!("CARGO_PKG_VERSION")
}

#[cfg(test)]
pub fn app_version() -> &'static str {
    "TEST"
}
