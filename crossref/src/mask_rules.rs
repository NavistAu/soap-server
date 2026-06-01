//! Default path-scoped mask rules for SOAP responses (spec §5.3). Extend per
//! scenario as volatile fields appear; never use value-pattern masks.
use crate::normalize::MaskRule;

pub fn default_masks() -> Vec<MaskRule> {
    // The controlled Echo service is fully deterministic, so the default set is
    // empty. WS-Security / WS-Addressing scenarios add path-scoped rules here,
    // e.g. MaskRule::new("Envelope/Header/Security/UsernameToken/Created").
    Vec::new()
}
