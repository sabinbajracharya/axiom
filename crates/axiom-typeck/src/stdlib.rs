/// Concatenate stdlib source with user source, separated by a newline.
/// The stdlib defines library types (List, etc.) that replace compiler built-ins.
pub(crate) fn with_stdlib(user_source: &str) -> String {
    let stdlib = include_str!("../../../stdlib/collections/list.ax");
    format!("{stdlib}\n{user_source}")
}
