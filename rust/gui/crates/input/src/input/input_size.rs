/// Input size variants
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum InputSize {
    /// Extra small input
    Xs,
    /// Small input
    Sm,
    /// Medium input (default)
    #[default]
    Md,
    /// Large input
    Lg,
}
