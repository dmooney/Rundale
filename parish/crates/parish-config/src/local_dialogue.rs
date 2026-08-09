//! Evidence-backed registry for production-qualified local dialogue profiles.
//!
//! A profile may be added only after `promptfoo/scripts/promotion_gate.py`
//! emits a passing receipt for the exact provider/model/request/hardware
//! contract. Keeping this registry empty is intentional when no measured
//! profile passes; setup may still expose local inference as experimental.

/// Exact provider/model pairs with a checked-in passing promotion receipt.
///
/// No fully local dialogue profile currently passes the production gate.
pub const QUALIFIED_LOCAL_DIALOGUE_PROFILES: &[(&str, &str)] = &[];

pub fn is_local_dialogue_profile_qualified(provider: &str, model: &str) -> bool {
    QUALIFIED_LOCAL_DIALOGUE_PROFILES
        .iter()
        .any(|(qualified_provider, qualified_model)| {
            provider.eq_ignore_ascii_case(qualified_provider) && model == *qualified_model
        })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bundled_qwen25_profile_is_not_production_qualified() {
        assert!(!is_local_dialogue_profile_qualified(
            "vllmmlx",
            "mlx-community/Qwen2.5-14B-Instruct-4bit"
        ));
    }
}
