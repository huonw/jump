use jump::SelectBoot;
use proc_exit::{Code, ExitResult};

mod pack;
pub(crate) use pack::make as pack;

pub(crate) fn select(select_boot: SelectBoot) -> ExitResult {
    Err(Code::FAILURE.with_message(format!(
        "This Scie binary has no default boot command.\n\
            Please select from the following:\n\
            {boot_commands}\n\
            \n\
            You can select a boot command by passing it as the 1st argument or else by \
            setting the SCIE_BOOT environment variable.\n\
            {error_message}",
        boot_commands = select_boot
            .boots
            .into_iter()
            .map(|boot| if let Some(description) = boot.description {
                format!("{name}: {description}", name = boot.name)
            } else {
                boot.name
            })
            .collect::<Vec<_>>()
            .join("\n"),
        error_message = select_boot.error_message.unwrap_or_default()
    )))
}