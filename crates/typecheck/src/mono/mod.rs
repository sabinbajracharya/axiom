//! Monomorphization pass (M2 step 6).
//!
//! Walks the THIR, discovers all generic call sites, and produces a set of
//! concrete, specialized function instances — one per unique
//! `(generic_def_id, concrete_type_args)` pair.
//!
//! Zero-cost: no boxing, no vtables, no runtime dispatch. The cost is code
//! size (one copy per type-argument combination), mitigated by deduplication.
//!
//! Output: [`MonoResult`] — a list of [`MonoInstance`]s, each carrying the
//! mangled name, concrete parameter types, and concrete return type.  Body
//! cloning / IR lowering happens downstream (when the IR crate exists).

pub mod helpers;
mod walk;

#[cfg(test)]
mod tests;

use std::collections::{HashMap, VecDeque};

use resolver::{FnDef, HirId, Item};

use crate::thir::Thir;
use crate::types::{Ty, TypeParamId};

// ── Public types ──────────────────────────────────────────────────────────────

/// The output of monomorphization.
#[derive(Debug, Clone)]
pub struct MonoResult {
    /// All monomorphized function instances, in discovery order.
    pub instances: Vec<MonoInstance>,
}

/// A single monomorphized function instance.
#[derive(Debug, Clone)]
pub struct MonoInstance {
    /// Mangled name: `original__Type1_Type2` (e.g., `id__Int`).
    pub name: String,
    /// Original function name (e.g., `id`).
    pub original_name: String,
    /// Concrete type arguments (e.g., `[Ty::Int]`).
    pub type_args: Vec<Ty>,
    /// HirId of the original generic FnDef.
    pub original_id: HirId,
    /// Concrete parameter types after substitution.
    pub param_types: Vec<Ty>,
    /// Concrete return type after substitution.
    pub return_type: Ty,
}

// ── Entry point ───────────────────────────────────────────────────────────────

/// Monomorphize all generic function calls in the THIR.
///
/// Returns a [`MonoResult`] containing one [`MonoInstance`] per unique
/// `(FnDef, type_args)` pair.  Non-generic programs yield an empty result.
pub fn monomorphize(thir: &Thir) -> MonoResult {
    let mut mono = Monomorphizer::new(thir);
    mono.run();
    MonoResult {
        instances: mono.instances.into_values().collect(),
    }
}

// ── Internal ──────────────────────────────────────────────────────────────────

type Substitution = HashMap<TypeParamId, Ty>;

/// Dedup key: `(fn_def_id, mangled_type_suffix)`.
/// Uses a string for the type-args portion because `Ty` contains `f64`
/// (Float) and cannot implement `Eq`/`Hash`.
type InstanceKey = (HirId, String);

struct Monomorphizer<'a> {
    thir: &'a Thir,
    instances: HashMap<InstanceKey, MonoInstance>,
    /// Worklist stores `(fn_id, concrete_type_args)`.  The Vec<Ty> is only
    /// held here transiently — the HashMap key uses the mangled string.
    worklist: VecDeque<(HirId, Vec<Ty>)>,
    fn_names: HashMap<HirId, String>,
    fn_defs: HashMap<HirId, FnDef>,
    fn_param_tys: HashMap<HirId, Vec<Ty>>,
    /// Active type substitution while walking a generic body.
    /// Empty when walking non-generic entry functions.
    current_subst: Substitution,
}

impl<'a> Monomorphizer<'a> {
    fn new(thir: &'a Thir) -> Self {
        Self {
            thir,
            instances: HashMap::new(),
            worklist: VecDeque::new(),
            fn_names: HashMap::new(),
            fn_defs: HashMap::new(),
            fn_param_tys: HashMap::new(),
            current_subst: HashMap::new(),
        }
    }

    fn run(&mut self) {
        self.collect_fn_defs();
        self.collect_call_sites();
        self.process_worklist();
    }

    // ── Phase 1: index all FnDefs ──────────────────────────────────────────

    fn collect_fn_defs(&mut self) {
        for item in &self.thir.hir.items {
            if let Item::FnDef(f) = item {
                self.register_fn(f);
            }
            if let Item::ImplDef(impl_def) = item {
                for m in &impl_def.methods {
                    self.register_fn(m);
                }
            }
        }
    }

    fn register_fn(&mut self, f: &FnDef) {
        self.fn_names.insert(f.id, f.name.clone());
        self.fn_defs.insert(f.id, f.clone());
        let param_tys: Vec<Ty> = f
            .params
            .iter()
            .filter_map(|p| self.thir.types.get(&p.id).cloned())
            .collect();
        self.fn_param_tys.insert(f.id, param_tys);
    }

    // ── Phase 2: discover generic call sites in non-generic fns ────────────

    fn collect_call_sites(&mut self) {
        for item in &self.thir.hir.items {
            match item {
                Item::FnDef(f) if f.type_params.is_empty() => {
                    self.collect_from_block(&f.body);
                }
                Item::ImplDef(impl_def) => {
                    for m in &impl_def.methods {
                        if m.type_params.is_empty() {
                            self.collect_from_block(&m.body);
                        }
                    }
                }
                _ => {}
            }
        }
    }

    // ── Phase 3: process worklist (walk bodies of generic fns) ─────────────

    fn process_worklist(&mut self) {
        while let Some((fn_id, type_args)) = self.worklist.pop_front() {
            let key = self.make_key(fn_id, &type_args);
            if self.instances.contains_key(&key) {
                continue;
            }
            self.collect_generic_body(fn_id, &type_args);
        }
    }

    /// Walk the body of `fn_id` with `type_args` applied, discover nested
    /// generic calls, and register the instance.
    fn collect_generic_body(&mut self, fn_id: HirId, type_args: &[Ty]) {
        let key = self.make_key(fn_id, type_args);
        if self.instances.contains_key(&key) {
            return;
        }

        let subst = self.build_type_param_subst(fn_id, type_args);

        let param_types = self
            .fn_param_tys
            .get(&fn_id)
            .map(|pts| pts.iter().map(|t| helpers::substitute(t, &subst)).collect())
            .unwrap_or_default();

        let return_type = self
            .thir
            .types
            .get(&fn_id)
            .and_then(|ty| match ty {
                Ty::Fn(fnty) => Some(helpers::substitute(&fnty.return_type, &subst)),
                _ => None,
            })
            .unwrap_or(Ty::Unit);

        let original_name = self.fn_names.get(&fn_id).cloned().unwrap_or_default();
        let name = helpers::mangle_name(&original_name, type_args);

        self.instances.insert(
            key,
            MonoInstance {
                name,
                original_name,
                type_args: type_args.to_vec(),
                original_id: fn_id,
                param_types,
                return_type,
            },
        );

        // Walk the body for nested generic calls, carrying the active
        // substitution so arg types in nested calls get resolved.
        if let Some(f) = self.fn_defs.get(&fn_id).cloned() {
            let prev = std::mem::replace(&mut self.current_subst, subst);
            self.collect_from_block_with_subst(&f.body, &self.current_subst.clone());
            self.current_subst = prev;
        }
    }

    fn build_type_param_subst(&self, fn_id: HirId, type_args: &[Ty]) -> Substitution {
        let mut subst = Substitution::new();
        if let Some(f) = self.fn_defs.get(&fn_id) {
            for (i, tp) in f.type_params.iter().enumerate() {
                if let Some(concrete) = type_args.get(i) {
                    subst.insert(
                        TypeParamId {
                            name: tp.name.clone(),
                            index: i,
                            def_id: tp.id,
                        },
                        concrete.clone(),
                    );
                }
            }
        }
        subst
    }

    // ── Call site analysis ─────────────────────────────────────────────────

    fn visit_call(&mut self, call: &resolver::CallExpr) {
        self.visit_call_inner(call);
    }

    fn visit_call_with_subst(&mut self, call: &resolver::CallExpr, _subst: &Substitution) {
        self.visit_call_inner(call);
    }

    fn visit_call_inner(&mut self, call: &resolver::CallExpr) {
        let callee_id = match &call.callee {
            resolver::NameRef::Resolved(r) => r.def_id,
            resolver::NameRef::Unresolved(_) => return,
        };

        let param_tys = match self.fn_param_tys.get(&callee_id) {
            Some(pts) if !pts.is_empty() => pts.clone(),
            _ => return,
        };
        if !helpers::contains_type_param_tys(&param_tys) {
            return;
        }

        let arg_tys: Vec<Ty> = call
            .args
            .iter()
            .filter_map(|a| self.thir.types.get(&a.id()).cloned())
            .map(|t| helpers::substitute(&t, &self.current_subst))
            .collect();

        if arg_tys.len() != param_tys.len() {
            return;
        }

        let mut subst = Substitution::new();
        for (arg_ty, param_ty) in arg_tys.iter().zip(param_tys.iter()) {
            helpers::unify(arg_ty, param_ty, &mut subst);
        }

        let type_args = self.extract_type_args(callee_id, &subst);
        if type_args.is_empty() {
            return;
        }

        let key = self.make_key(callee_id, &type_args);
        if !self.instances.contains_key(&key) {
            self.worklist.push_back((callee_id, type_args));
        }
    }

    fn extract_type_args(&self, fn_id: HirId, subst: &Substitution) -> Vec<Ty> {
        let f = match self.fn_defs.get(&fn_id) {
            Some(f) => f,
            None => return Vec::new(),
        };
        f.type_params
            .iter()
            .enumerate()
            .filter_map(|(i, tp)| {
                subst.get(&TypeParamId {
                    name: tp.name.clone(),
                    index: i,
                    def_id: tp.id,
                })
            })
            .cloned()
            .collect()
    }

    /// Build the dedup key from a fn id and concrete type args.
    fn make_key(&self, fn_id: HirId, type_args: &[Ty]) -> InstanceKey {
        let suffix = helpers::type_args_suffix(type_args);
        (fn_id, suffix)
    }
}
