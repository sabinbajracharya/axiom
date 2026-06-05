//! File discovery — scans a source directory and builds a `ModuleGraph`.
//!
//! Rules (design doc §1.2):
//! - `main.ax` at the source root is the entry point (required).
//! - `foo.ax` + `foo/` siblings → `foo/` children attach to the `foo` module.
//! - `foo/mod.ax` inside `foo/` → the directory IS the module.
//! - Error if both `foo.ax` AND `foo/mod.ax` exist for the same name.
//! - Error on name collisions (case-insensitive, OS-safe).
//! - Error on non-UTF8 filenames.

use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};

use crate::error::ModuleError;
use crate::graph::{ModuleEntry, ModuleGraph, ModuleId, Visibility};

/// Shorthand for the name→path maps used during discovery.
type PathMap = HashMap<String, PathBuf>;

/// Build a `ModuleGraph` from a source directory.
pub fn discover(src_dir: &Path) -> Result<ModuleGraph, ModuleError> {
    let main_path = src_dir.join("main.ax");
    if !main_path.exists() {
        return Err(ModuleError::MissingMain {
            dir: src_dir.to_path_buf(),
        });
    }

    let mut modules: Vec<ModuleEntry> = Vec::new();
    let mut name_index: PathMap = HashMap::new();

    let root_source = fs::read_to_string(&main_path).map_err(|e| ModuleError::Io {
        path: main_path.clone(),
        source: e,
    })?;
    modules.push(ModuleEntry {
        path: PathBuf::from("main.ax"),
        name: String::new(),
        source: root_source,
        parent: None,
        children: Vec::new(),
        vis: Visibility::Pub,
    });
    let root = ModuleId(0);

    discover_children(
        src_dir,
        &PathBuf::from(""),
        root,
        &mut modules,
        &mut name_index,
    )?;

    Ok(ModuleGraph { modules, root })
}

/// Collect `.ax` files and subdirectories from `dir`, filtering out `main.ax`
/// (already registered) and `mod.ax` (handled in the directory loop).
fn collect_entries(dir: &Path, rel_path: &Path) -> Result<(PathMap, PathMap), ModuleError> {
    let entries = fs::read_dir(dir).map_err(|e| ModuleError::Io {
        path: dir.to_path_buf(),
        source: e,
    })?;

    let mut ax_files: PathMap = HashMap::new();
    let mut subdirs: PathMap = HashMap::new();

    for entry in entries {
        let entry = entry.map_err(|e| ModuleError::Io {
            path: dir.to_path_buf(),
            source: e,
        })?;
        let path = entry.path();
        let file_type = entry.file_type().map_err(|e| ModuleError::Io {
            path: path.clone(),
            source: e,
        })?;

        if file_type.is_file() {
            if path.extension().and_then(|e| e.to_str()) == Some("ax") {
                let stem = path
                    .file_stem()
                    .and_then(|s| s.to_str())
                    .ok_or_else(|| ModuleError::NonUtf8Name { path: path.clone() })?;
                // Skip root main.ax (already registered) and mod.ax (handled below).
                if stem == "main" && rel_path.as_os_str().is_empty() {
                    continue;
                }
                if stem == "mod" {
                    continue;
                }
                ax_files.insert(stem.to_string(), path.clone());
            }
        } else if file_type.is_dir() {
            let dir_name = path
                .file_name()
                .and_then(|s| s.to_str())
                .ok_or_else(|| ModuleError::NonUtf8Name { path: path.clone() })?
                .to_string();
            subdirs.insert(dir_name, path.clone());
        }
    }

    Ok((ax_files, subdirs))
}

/// Register a module and insert its name into the index.
fn register_module(
    module_name: &str,
    path: PathBuf,
    parent: ModuleId,
    modules: &mut Vec<ModuleEntry>,
    name_index: &mut PathMap,
) -> Result<ModuleId, ModuleError> {
    check_name_collision(module_name, name_index, &path)?;

    let source = fs::read_to_string(&path).map_err(|e| ModuleError::Io {
        path: path.clone(),
        source: e,
    })?;
    let child_id = ModuleId(modules.len());
    modules[parent.0].children.push(child_id);
    name_index.insert(module_name.to_lowercase(), path.clone());

    modules.push(ModuleEntry {
        path,
        name: module_name.to_string(),
        source,
        parent: Some(parent),
        children: Vec::new(),
        vis: Visibility::Private,
    });

    Ok(child_id)
}

/// Recursively discover modules under `dir`, attaching them to `parent`.
fn discover_children(
    dir: &Path,
    rel_path: &Path,
    parent: ModuleId,
    modules: &mut Vec<ModuleEntry>,
    name_index: &mut PathMap,
) -> Result<(), ModuleError> {
    let (ax_files, subdirs) = collect_entries(dir, rel_path)?;

    // Process each .ax file (non-mod, non-main).
    for (stem, ax_path) in &ax_files {
        let sub_dir = subdirs.get(stem);
        let mod_ax = ax_path
            .parent()
            .map(|p| p.join(stem).join("mod.ax"))
            .filter(|p| p.exists());

        // Conflict: foo.ax AND foo/mod.ax both exist.
        if let Some(ref mod_ax_path) = mod_ax {
            return Err(ModuleError::DualModuleDef {
                file: ax_path.clone(),
                dir_mod: mod_ax_path.clone(),
            });
        }

        let child_rel = rel_path.join(format!("{stem}.ax"));
        let module_name = rel_to_module_name(&child_rel);
        let child_id = register_module(&module_name, ax_path.clone(), parent, modules, name_index)?;

        // If a sibling directory exists (without mod.ax), discover its children.
        if let Some(sub_dir_path) = sub_dir {
            let child_rel_dir = rel_path.join(stem);
            discover_children(sub_dir_path, &child_rel_dir, child_id, modules, name_index)?;
        }
    }

    // Process directories that have mod.ax (and no sibling .ax file).
    for (dir_name, dir_path) in &subdirs {
        if ax_files.contains_key(dir_name) {
            continue; // already handled as foo.ax + foo/ pattern
        }

        let mod_ax_path = dir_path.join("mod.ax");
        if !mod_ax_path.exists() {
            continue; // stray directory, not a module
        }

        let module_name = rel_to_module_name(&rel_path.join(dir_name));
        let child_id = register_module(&module_name, mod_ax_path, parent, modules, name_index)?;

        let child_rel_dir = rel_path.join(dir_name);
        discover_children(dir_path, &child_rel_dir, child_id, modules, name_index)?;
    }

    Ok(())
}

/// Convert a relative file path to a `::`-separated module name.
/// e.g. `bar/one.ax` → `bar::one`, `main.ax` → `""` (root).
fn rel_to_module_name(rel: &Path) -> String {
    let mut parts: Vec<String> = Vec::new();
    for component in rel.components() {
        let s = component.as_os_str().to_string_lossy();
        if let Some(stem) = s.strip_suffix(".ax") {
            if !stem.is_empty() && stem != "mod" {
                parts.push(stem.to_string());
            }
        } else {
            parts.push(s.to_string());
        }
    }
    parts.join("::")
}

/// Check for a case-insensitive module name collision.
fn check_name_collision(name: &str, name_index: &PathMap, path: &Path) -> Result<(), ModuleError> {
    let lower = name.to_lowercase();
    if let Some(first) = name_index.get(&lower) {
        return Err(ModuleError::NameCollision {
            name: name.to_string(),
            first: first.clone(),
            second: path.to_path_buf(),
        });
    }
    Ok(())
}
