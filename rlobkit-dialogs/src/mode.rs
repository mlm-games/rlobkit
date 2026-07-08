use std::num::NonZeroUsize;

#[derive(Debug, Clone, Default)]
pub enum RlobKitMode {
    #[default]
    Single,
    Multiple {
        limit: Option<NonZeroUsize>,
    },
}
