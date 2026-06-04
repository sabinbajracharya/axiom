/// Concatenate stdlib source with user source, separated by a newline.
/// The stdlib defines library types (List, Map, etc.) that replace compiler built-ins.
pub(crate) fn with_stdlib(user_source: &str) -> String {
    let list = include_str!("../../../stdlib/collections/list.ax");
    let map = include_str!("../../../stdlib/collections/map.ax");
    format!("{list}\n{map}\n{user_source}")
}
