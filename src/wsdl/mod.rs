// WSDL 1.1 parser — two-pass (parse + resolve)
pub mod definitions;
pub mod parser;
pub mod resolver;

pub use definitions::WsdlDefinition;
