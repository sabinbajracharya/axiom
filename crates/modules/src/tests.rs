//! Tests for module graph discovery.
#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::fs;
use std::path::Path;

use crate::discover::discover;
use crate::error::ModuleError;

/// Helper: create a temp directory with the given files, run a test, then clean up.
fn with_project(files: &[(&str, &str)], f: impl FnOnce(&Path)) {
    let dir = tempfile::tempdir().expect("create temp dir");
    let root = dir.path();
    for (rel, contents) in files {
        let path = root.join(rel);
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).expect("create parent dir");
        }
        fs::write(&path, contents).expect("write file");
    }
    f(root);
    dir.close().expect("cleanup temp dir");
}

#[test]
fn test_flat_main_only() {
    with_project(&[("main.ax", "fn main() { }")], |root| {
        let graph = discover(root).unwrap();
        assert_eq!(graph.modules.len(), 1);
        assert_eq!(graph.modules[0].name, "");
        assert!(graph.modules[0].parent.is_none());
        assert!(graph.modules[0].children.is_empty());
    });
}

#[test]
fn test_main_with_sibling_module() {
    with_project(
        &[("main.ax", "use bar;"), ("bar.ax", "fn bar() { }")],
        |root| {
            let graph = discover(root).unwrap();
            assert_eq!(graph.modules.len(), 2);
            assert_eq!(graph.modules[0].name, ""); // root
            assert_eq!(graph.modules[1].name, "bar");
            assert_eq!(graph.modules[0].children.len(), 1);
            assert_eq!(graph.modules[1].parent, Some(graph.root));
        },
    );
}

#[test]
fn test_nested_directory_children() {
    with_project(
        &[
            ("main.ax", "use bar;"),
            ("bar.ax", "fn bar() { }"),
            ("bar/one.ax", "fn one() { }"),
            ("bar/two.ax", "fn two() { }"),
        ],
        |root| {
            let graph = discover(root).unwrap();
            assert_eq!(graph.modules.len(), 4);
            // root -> bar -> bar::one, bar::two
            let bar = graph.modules.iter().find(|m| m.name == "bar").unwrap();
            assert_eq!(bar.children.len(), 2);
            let one = graph.modules.iter().find(|m| m.name == "bar::one").unwrap();
            assert_eq!(
                one.parent,
                Some(
                    graph
                        .modules
                        .iter()
                        .position(|m| m.name == "bar")
                        .map(crate::graph::ModuleId)
                        .unwrap()
                )
            );
        },
    );
}

#[test]
fn test_mod_ax_pattern() {
    with_project(
        &[("main.ax", "use quux;"), ("quux/mod.ax", "fn quux() { }")],
        |root| {
            let graph = discover(root).unwrap();
            assert_eq!(graph.modules.len(), 2);
            assert_eq!(graph.modules[1].name, "quux");
        },
    );
}

#[test]
fn test_mod_ax_with_children() {
    with_project(
        &[
            ("main.ax", ""),
            ("foo/mod.ax", "fn foo() { }"),
            ("foo/child.ax", "fn child() { }"),
        ],
        |root| {
            let graph = discover(root).unwrap();
            assert_eq!(graph.modules.len(), 3);
            let foo = graph.modules.iter().find(|m| m.name == "foo").unwrap();
            assert_eq!(foo.children.len(), 1);
            let child = graph
                .modules
                .iter()
                .find(|m| m.name == "foo::child")
                .unwrap();
            assert_eq!(
                child.parent,
                Some(
                    graph
                        .modules
                        .iter()
                        .position(|m| m.name == "foo")
                        .map(crate::graph::ModuleId)
                        .unwrap()
                )
            );
        },
    );
}

#[test]
fn test_error_missing_main() {
    with_project(&[("bar.ax", "fn bar() { }")], |root| {
        let err = discover(root).unwrap_err();
        assert!(matches!(err, ModuleError::MissingMain { .. }));
    });
}

#[test]
fn test_error_dual_module_def() {
    with_project(
        &[
            ("main.ax", ""),
            ("foo.ax", "fn foo() { }"),
            ("foo/mod.ax", "fn foo_mod() { }"),
        ],
        |root| {
            let err = discover(root).unwrap_err();
            assert!(matches!(err, ModuleError::DualModuleDef { .. }));
        },
    );
}

#[test]
fn test_topo_order_root_first() {
    with_project(
        &[("main.ax", ""), ("bar.ax", ""), ("bar/one.ax", "")],
        |root| {
            let graph = discover(root).unwrap();
            let order = graph.topo_order();
            assert_eq!(order[0], graph.root);
            // All modules appear exactly once.
            assert_eq!(order.len(), graph.modules.len());
        },
    );
}

#[test]
fn test_find_by_name() {
    with_project(
        &[("main.ax", ""), ("bar.ax", ""), ("bar/one.ax", "")],
        |root| {
            let graph = discover(root).unwrap();
            assert!(graph.find_by_name("bar").is_some());
            assert!(graph.find_by_name("bar::one").is_some());
            assert!(graph.find_by_name("nonexistent").is_none());
        },
    );
}

#[test]
fn test_multiple_sibling_modules() {
    with_project(
        &[("main.ax", ""), ("a.ax", ""), ("b.ax", ""), ("c.ax", "")],
        |root| {
            let graph = discover(root).unwrap();
            assert_eq!(graph.modules.len(), 4);
            assert_eq!(graph.modules[0].children.len(), 3);
        },
    );
}
