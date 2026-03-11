pub struct ProgramStatus {
    pub id: String,
    pub name: String,
    pub status: String,
}

pub enum ChannelResponse {
    Status(Vec<ProgramStatus>),
    Error(String),
    Feedback(String),
}