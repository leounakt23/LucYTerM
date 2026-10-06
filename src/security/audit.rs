//! Machine-readable security posture report for settings and CLI surfaces.

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Status {
    Enabled,
    Disabled,
    Platform,
    Review,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Finding {
    pub control: &'static str,
    pub status: Status,
    pub detail: &'static str,
}

pub fn report() -> Vec<Finding> {
    vec![
        Finding {
            control: "encrypted session store",
            status: Status::Enabled,
            detail: "AES-256-GCM with Argon2id",
        },
        Finding {
            control: "strict host-key verification",
            status: Status::Enabled,
            detail: "known_hosts required by SSH handler",
        },
        Finding {
            control: "core dumps",
            status: Status::Enabled,
            detail: "disabled during process initialization",
        },
        Finding {
            control: "agent forwarding",
            status: Status::Review,
            detail: "must be explicitly enabled per session",
        },
        Finding {
            control: "idle lock",
            status: Status::Enabled,
            detail: "available through the idle-lock state machine",
        },
    ]
}
