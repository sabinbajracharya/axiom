//! Pass 1: Collect item signatures, struct definitions, enum definitions,
//! trait definitions, and impl blocks into the type environment.

use super::{FieldInfo, ImplInfo, Mutability, StructInfo, TypeChecker, VariantInfo};
use crate::types::{EnumTy, ErrorSetTy, FnTy, InstanceTy, StructTy, Ty};
use parser::ast::AstNode;
use resolver::hir_types::{ErrorSetDef, Item};
use resolver::*;
use std::collections::HashMap;

impl TypeChecker {
    pub(super) fn collect_pass(&mut self) {
        // The core trait declarations (Deinit/Equatable/Hashable/Ord) AND their
        // primitive impls now live in `stdlib/core/*.ax`, collected via
        // collect_trait_defs / collect_impl_defs like any other code. The
        // compiler only registers the irreducible *floor* methods (as_bytes,
        // len, hash_raw, and the List/Map intrinsic stand-ins).
        self.register_builtin_methods();
        self.collect_struct_defs();
        self.register_struct_deinit_impls();
        self.collect_enum_defs();
        self.collect_error_set_defs();
        self.collect_fn_sigs();
        self.collect_trait_defs();
        self.collect_impl_defs();
    }

    fn collect_struct_defs(&mut self) {
        // First pass: resolve field types and collect struct infos.
        // We also track (field_hir_id, resolved_ty) for populating the TypeMap.
        struct StructCollect {
            info: StructInfo,
            field_ids: Vec<(HirId, Ty)>,
        }

        let collected: Vec<StructCollect> = self
            .hir
            .items
            .iter()
            .filter_map(|item| match item {
                Item::StructDef(s) => {
                    let mut field_ids = Vec::new();
                    let fields = s
                        .fields
                        .iter()
                        .map(|f| {
                            let ty = self.resolve_hir_ty(&f.ty);
                            field_ids.push((f.id, ty.clone()));
                            FieldInfo {
                                name: f.name.clone(),
                                ty,
                            }
                        })
                        .collect();
                    Some(StructCollect {
                        info: StructInfo {
                            name: s.name.clone(),
                            def_id: s.id,
                            fields,
                        },
                        field_ids,
                    })
                }
                _ => None,
            })
            .collect();

        // Second pass: register in env, TypeMap, and field table.
        for sc in &collected {
            // Populate TypeMap for struct field declarations.
            for (fid, fty) in &sc.field_ids {
                self.types.insert(*fid, fty.clone());
            }
            let field_types: Vec<(String, Ty)> = sc
                .info
                .fields
                .iter()
                .map(|f| (f.name.clone(), f.ty.clone()))
                .collect();
            self.env.define(
                sc.info.name.clone(),
                Ty::Struct(StructTy {
                    name: sc.info.name.clone(),
                    def_id: sc.info.def_id,
                }),
                sc.info.def_id,
                Mutability::Immutable,
            );
            self.register_struct_fields(&sc.info.name, &field_types);
        }
    }

    /// Register `Deinit` auto-impls for every user-defined struct.
    ///
    /// A struct's drop conceptually calls `drop` on each field. All primitive
    /// fields already have Deinit from `register_builtin_impls`; nested structs
    /// get their own Deinit impl from this same pass. The bound-checker resolves
    /// the chain at check time.
    fn register_struct_deinit_impls(&mut self) {
        for item in &self.hir.items {
            if let Item::StructDef(s) = item {
                self.impl_table.push(ImplInfo {
                    trait_name: Some("Deinit".to_string()),
                    type_name: s.name.clone(),
                    methods: vec![],
                    subscripts: vec![],
                    type_params: vec![],
                    type_param_bounds: HashMap::new(),
                });
            }
        }
    }

    fn collect_enum_defs(&mut self) {
        // Clone the enum defs out first so we can resolve each variant's payload
        // in the enum's own type-param scope (which borrows `self` mutably).
        let enum_defs: Vec<EnumDef> = self
            .hir
            .items
            .iter()
            .filter_map(|item| match item {
                Item::EnumDef(e) => Some(e.clone()),
                _ => None,
            })
            .collect();

        for e in &enum_defs {
            let saved = std::mem::take(&mut self.current_type_params);
            self.current_type_params = e
                .type_params
                .iter()
                .map(|tp| (tp.name.clone(), tp.id, Vec::new()))
                .collect();
            let variants: Vec<VariantInfo> = e
                .variants
                .iter()
                .map(|v| VariantInfo {
                    name: v.name.clone(),
                    def_id: v.id,
                    payload: v.payload.iter().map(|t| self.resolve_hir_ty(t)).collect(),
                })
                .collect();
            self.current_type_params = saved;

            // A generic enum's values are `Ty::Instance` (carrying the type
            // arguments, like generic structs); a plain enum stays `Ty::Enum`.
            let self_ty = if e.type_params.is_empty() {
                Ty::Enum(EnumTy {
                    name: e.name.clone(),
                    def_id: e.id,
                })
            } else {
                let args = e
                    .type_params
                    .iter()
                    .enumerate()
                    .map(|(i, tp)| {
                        Ty::TypeParam(crate::types::TypeParamId {
                            name: tp.name.clone(),
                            index: i,
                            def_id: tp.id,
                        })
                    })
                    .collect();
                Ty::Instance(InstanceTy {
                    name: e.name.clone(),
                    def_id: e.id,
                    args,
                })
            };
            self.env
                .define(e.name.clone(), self_ty.clone(), e.id, Mutability::Immutable);
            self.register_enum_variants(&e.name, &variants, &self_ty);
        }
    }

    fn collect_error_set_defs(&mut self) {
        let sets: Vec<ErrorSetDef> = self
            .hir
            .items
            .iter()
            .filter_map(|item| match item {
                Item::ErrorSetDef(e) => Some(e.clone()),
                _ => None,
            })
            .collect();

        for e in &sets {
            let name = e.name.clone();
            let def_id = resolver::HirId(e.id.0);
            let mut variant_names = Vec::new();

            for v in &e.variants {
                let var_name = v.name.clone();
                let var_def_id = resolver::HirId(v.id.0);
                variant_names.push(var_name.clone());

                let fn_ty = Ty::Fn(FnTy {
                    params: vec![],
                    return_type: Box::new(Ty::ErrorSet(ErrorSetTy {
                        name: name.clone(),
                        def_id,
                        variant_names: vec![],
                    })),
                });
                self.env
                    .define(var_name, fn_ty, var_def_id, Mutability::Immutable);
            }

            self.env.define(
                name.clone(),
                Ty::ErrorSet(ErrorSetTy {
                    name: name.clone(),
                    def_id,
                    variant_names: variant_names.clone(),
                }),
                def_id,
                Mutability::Immutable,
            );
        }
    }

    fn collect_fn_sigs(&mut self) {
        // The prelude's `print`/`println` signatures are seeded first so they are
        // available in every compilation path — including the single-file check
        // path, whose unit does *not* contain the `stdlib/std/io.ax` FnDef bodies
        // (it resolves their names through module exports only). User or
        // in-unit definitions of the same name override these below.
        // See `docs/string-format-and-print-retire.md`.
        self.inject_prelude_sigs();
        // Clone out the in-unit FnDefs first: `register_fn_sig` borrows `self`
        // mutably, which can't coexist with iterating `&self.hir.items`. These
        // are registered after the prelude so an in-unit definition overrides it.
        let fn_defs: Vec<FnDef> = self
            .hir
            .items
            .iter()
            .filter_map(|item| match item {
                Item::FnDef(f) => Some(f.clone()),
                _ => None,
            })
            .collect();
        for f in &fn_defs {
            self.register_fn_sig(f);
        }
    }

    /// Register one function's signature in the type environment. Shared by the
    /// in-unit collection pass and the prelude injection.
    fn register_fn_sig(&mut self, f: &FnDef) {
        // Set type param scope so resolve_hir_ty can resolve T, U, etc.
        self.current_type_params = f
            .type_params
            .iter()
            .map(|tp| {
                let bounds = tp.bounds.iter().map(|b| name_text(&b.name)).collect();
                (tp.name.clone(), tp.id, bounds)
            })
            .collect();
        // Store bounds by type param HirId for bound checking at call sites.
        for (_, tp_id, bounds) in &self.current_type_params {
            self.type_param_bounds.insert(*tp_id, bounds.clone());
        }
        let param_types: Vec<Ty> = f
            .params
            .iter()
            .map(|p| {
                p.ty.as_ref()
                    .map(|t| self.resolve_hir_ty(t))
                    .unwrap_or(Ty::Error)
            })
            .collect();
        let return_type = f
            .return_type
            .as_ref()
            .map(|t| self.resolve_hir_ty(t))
            .unwrap_or(Ty::Unit);
        let fn_ty = Ty::Fn(FnTy {
            params: param_types,
            return_type: Box::new(return_type),
        });
        self.env
            .define(f.name.clone(), fn_ty, f.id, Mutability::Immutable);
        self.current_type_params.clear();
    }

    /// Seed the prelude's function signatures (`print`/`println`) into the
    /// environment from the bundled `stdlib/std/io.ax`. This makes them genuinely
    /// the stdlib functions — `String`-only — in every path, retiring the old
    /// hand-written generic stand-in. Signatures only: the bodies are *not*
    /// added to `hir.items`, so THIR dumps stay focused on user code.
    fn inject_prelude_sigs(&mut self) {
        const PRELUDE_IO: &str = include_str!("../../../../crates/stdlib/source/std/io.ax");
        let result = parser::parse(PRELUDE_IO);
        let Some(root) = parser::ast::SourceFile::cast(result.tree) else {
            return;
        };
        let (items, _defs, _diags, _next) = resolver::lower_structural(&root, PRELUDE_IO, 0);
        for item in &items {
            if let Item::FnDef(f) = item {
                self.register_fn_sig(f);
            }
        }
    }
}

// Re-export `name_text` from collect_subscripts for use here and in sibling
// modules (`collect::name_text`).
pub(super) use super::collect_subscripts::name_text;
