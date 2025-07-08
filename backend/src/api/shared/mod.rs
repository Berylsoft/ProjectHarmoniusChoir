pub mod project_user;

pub mod submit {
    use strum::{EnumString, IntoStaticStr};

    #[derive(
        Debug, Clone, Copy, PartialEq, Eq, Hash, IntoStaticStr, EnumString,
    )]
    pub enum Status {
        Rejected,
        Passed,
    }
}
