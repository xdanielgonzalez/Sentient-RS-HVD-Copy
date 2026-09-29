/// An uninhabited marker for "this `Src` has no data for this field slot" — used by generated code
/// as a placeholder `Field<Src>`-adjacent type where the IDL declares a field the canonical model
/// never populates. Implements nothing here: the `Src`-specific impls (with `match *self {}`
/// unreachable bodies, since no value of this type can ever exist) are emitted by the codegen
/// alongside the generated field list, not defined in this leaf crate.
pub enum Absent {}
