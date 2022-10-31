use jump::Jump;
use proc_exit::{Code, ExitResult};
use std::path::PathBuf;

pub(crate) fn make(jump: Jump, path: PathBuf) -> ExitResult {
    Err(Code::FAILURE.with_message(format!(
        "TODO(John Sirois): Implement boot-pack for {path}: {jump:#?}",
        path = path.display()
    )))
}