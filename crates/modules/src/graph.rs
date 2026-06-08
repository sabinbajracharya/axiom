//! Module graph — the in-memory representation of a multi-file Axiom project.
//!
//! Each `.ax` source file becomes one module. The graph captures parent/child
//! relationships derived from the directory structure (design doc §1.2).

use std::path::PathBuf;

/// Opaque handle into `ModuleGraph.modules`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct ModuleId(pub usize);

/// Module-level visibility (design doc §1.3).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Visibility {
    /// Visible outside the project (available to dependents).
    Pub,
    /// Visible only inside this project (default).
    Private,
}

/// One module in the project — maps to a single `.ax` file on disk.
#[derive(Debug)]
pub struct ModuleEntry {
    /// Path relative to the project source root.
    pub path: PathBuf,
    /// Dot-separated module name derived from the path (e.g. `bar::one`).
    pub name: String,
    /// File contents (UTF-8).
    pub source: String,
    /// Parent module, if any (root has no parent).
    pub parent: Option<ModuleId>,
    /// Direct child modules.
    pub children: Vec<ModuleId>,
    /// Module-level visibility (from `pub mod` vs bare `mod`).
    pub vis: Visibility,
}

/// The full module graph for one project.
#[derive(Debug)]
pub struct ModuleGraph {
    pub modules: Vec<ModuleEntry>,
    pub root: ModuleId,
}

impl ModuleGraph {
    /// Look up a module by its dot-separated name (e.g. `"bar::one"`).
    pub fn find_by_name(&self, name: &str) -> Option<ModuleId> {
        self.modules
            .iter()
            .position(|m| m.name == name)
            .map(ModuleId)
    }

    /// Borrow a module entry by id.
    pub fn get(&self, id: ModuleId) -> &ModuleEntry {
        &self.modules[id.0]
    }

    /// Borrow a module entry by id (mutable).
    pub fn get_mut(&mut self, id: ModuleId) -> &mut ModuleEntry {
        &mut self.modules[id.0]
    }

    /// Walk modules in topological order (parents before children).
    /// The root is always first.
    pub fn topo_order(&self) -> Vec<ModuleId> {
        let mut order = Vec::with_capacity(self.modules.len());
        let mut stack = vec![self.root];
        while let Some(id) = stack.pop() {
            order.push(id);
            let entry = self.get(id);
            // Push children in reverse so the first child is visited first.
            for child in entry.children.iter().rev() {
                stack.push(*child);
            }
        }
        order
    }

    /// Merge another graph into this one. The other graph's root becomes a
    /// child of this graph's root. All module IDs in the other graph are
    /// re-indexed to avoid conflicts.
    pub fn merge(&mut self, other: ModuleGraph) {
        let offset = self.modules.len();
        let other_root_new = ModuleId(other.root.0 + offset);

        // Re-index and append all modules from the other graph.
        for mut entry in other.modules {
            // Re-index parent.
            if let Some(ref mut parent) = entry.parent {
                parent.0 += offset;
            }
            // Re-index children.
            for child in &mut entry.children {
                child.0 += offset;
            }
            self.modules.push(entry);
        }

        // Attach the other graph's root as a child of our root.
        self.modules[self.root.0].children.push(other_root_new);
        // Set the other root's parent to our root.
        self.modules[other_root_new.0].parent = Some(self.root);
    }
}
