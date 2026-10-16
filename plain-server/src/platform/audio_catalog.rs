pub(crate) fn uses_system_catalog() -> bool {
    cfg!(target_os = "android")
}
