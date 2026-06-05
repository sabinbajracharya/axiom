/// Concatenate stdlib source with user source, separated by a newline.
/// The stdlib defines library types (List, Map, etc.) that replace compiler built-ins,
/// and extern function declarations (io) that map to VM builtins.
pub(crate) fn with_stdlib(user_source: &str) -> String {
    let list = include_str!("../../../stdlib/collections/list.ax");
    let map = include_str!("../../../stdlib/collections/map.ax");
    let io = include_str!("../../../stdlib/io.ax");
    format!("{list}\n{map}\n{io}\n{user_source}")
}
