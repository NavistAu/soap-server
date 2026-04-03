// XSD schema parser — complexType, simpleType, sequence, choice, all, extension, restriction, imports
pub mod elements;
pub mod parser;
pub mod resolver;
pub mod types;

pub use types::{XsdType, ComplexType, SimpleType, TypeRegistry};
pub use elements::{XsdElement, XsdAttribute, MaxOccurs};
pub use parser::{parse_schema, RawSchema, SchemaImport, SchemaInclude, SchemaError};
