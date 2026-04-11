#[derive(Debug, Clone, Default)]
pub enum RlobKitMode {
    #[default]
    Single,
    Multiple {
        limit: Option<usize>,
    },
}
