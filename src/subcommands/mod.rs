pub mod new;
pub mod versions;

use crate::command_line::Software;
use crate::{folia, papermc};

/// The name the PaperMC API uses for `software`.
fn project_name(software: Software) -> &'static str {
    match software {
        Software::Paper => papermc::PROJECT_NAME,
        Software::Folia => folia::PROJECT_NAME,
    }
}
