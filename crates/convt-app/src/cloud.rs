//! Converting on convt's cloud instead of on this computer. Only a signed-in
//! account with paid Pro can, and only after the user agrees to upload the
//! file, since the cloud is the one place a file leaves the machine.

/// Whether the Cloud choice in Quick convert can be used, and if not, why.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CloudAccess {
    /// Signed in with paid Pro; cloud conversions can run.
    Ready,
    /// Not signed in to convt.app on this computer.
    SignedOut,
    /// Signed in, but the account has no paid Pro.
    NeedsPro,
    /// Something else stops it, in words for the user (offline, not set up
    /// in this build).
    Unavailable(String),
}

impl CloudAccess {
    pub fn ready(&self) -> bool {
        matches!(self, CloudAccess::Ready)
    }

    /// What the disabled Cloud choice says on hover.
    pub fn reason(&self) -> Option<String> {
        match self {
            CloudAccess::Ready => None,
            CloudAccess::SignedOut => {
                Some("Sign in with a Pro account to convert in the cloud.".into())
            }
            CloudAccess::NeedsPro => Some("Cloud conversion is part of Pro.".into()),
            CloudAccess::Unavailable(why) => Some(why.clone()),
        }
    }
}
