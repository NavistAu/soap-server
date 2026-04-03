// WSDL 1.1 parser — two-pass (parse + resolve)
pub mod definitions;
pub mod parser;
pub mod resolver;

pub use definitions::WsdlDefinition;
pub use parser::{parse_wsdl, WsdlError};
pub use resolver::{resolve_wsdl, ResolvedWsdl, WsdlLoader, rewrite_wsdl_address};
