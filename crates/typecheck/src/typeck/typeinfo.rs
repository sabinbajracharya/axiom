//! Generic type-definition introspection: resolve a struct's fields or an
//! enum's variant payloads *in the type's own type-parameter scope*, so a field
//! or payload written `T` comes back as `Ty::TypeParam` keyed by the type's
//! parameter. Callers substitute concrete type arguments (generic field access,
//! variant pattern binding) or infer them (struct-literal / constructor
//! inference).

use super::{TypeChecker, VariantInfo};
use crate::types::Ty;
use hir::{HirTypeParam, Item};

/// A struct's declared type parameters paired with its field types (resolved in
/// the struct's own scope). See [`TypeChecker::struct_generic_info`].
pub(super) type StructGenericInfo = (Vec<HirTypeParam>, Vec<(String, Ty)>);

/// A generic enum's type parameters paired with its variants (payloads resolved
/// in the enum's own scope). See [`TypeChecker::enum_generic_info`].
pub(super) type EnumGenericInfo = (Vec<HirTypeParam>, Vec<VariantInfo>);

impl TypeChecker {
    /// Resolve a struct's declared type parameters and field types with the
    /// struct's own type parameters in scope. A field declared `value: T`
    /// resolves to `Ty::TypeParam` keyed by the struct's parameter def_id.
    /// Returns `None` if `name` is not a user-defined struct.
    pub(super) fn struct_generic_info(&mut self, name: &str) -> Option<StructGenericInfo> {
        let sdef = self.hir.items.iter().find_map(|item| match item {
            Item::StructDef(s) if s.name == name => Some(s.clone()),
            _ => None,
        })?;
        let saved = std::mem::take(&mut self.current_type_params);
        self.current_type_params = sdef
            .type_params
            .iter()
            .map(|tp| (tp.name.clone(), tp.id, Vec::new()))
            .collect();
        let fields = sdef
            .fields
            .iter()
            .map(|f| (f.name.clone(), self.resolve_hir_ty(&f.ty)))
            .collect();
        self.current_type_params = saved;
        Some((sdef.type_params, fields))
    }

    /// A generic enum's type parameters paired with its variants, payloads
    /// resolved in the enum's own scope (so `Some(T)` comes back as
    /// `Ty::TypeParam`). `None` if `name` is not a user-defined enum.
    pub(super) fn enum_generic_info(&mut self, name: &str) -> Option<EnumGenericInfo> {
        let edef = self.hir.items.iter().find_map(|item| match item {
            Item::EnumDef(e) if e.name == name => Some(e.clone()),
            _ => None,
        })?;
        let saved = std::mem::take(&mut self.current_type_params);
        self.current_type_params = edef
            .type_params
            .iter()
            .map(|tp| (tp.name.clone(), tp.id, Vec::new()))
            .collect();
        let variants = edef
            .variants
            .iter()
            .map(|v| VariantInfo {
                name: v.name.clone(),
                def_id: v.id,
                payload: v.payload.iter().map(|t| self.resolve_hir_ty(t)).collect(),
            })
            .collect();
        self.current_type_params = saved;
        Some((edef.type_params, variants))
    }
}
